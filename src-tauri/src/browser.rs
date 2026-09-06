use crate::{model::ProviderConfig, providers::BrowserSpec};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::{
    webview::{NewWindowResponse, PageLoadEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tokio::sync::{oneshot, Mutex};

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct BrowserSession {
    app: AppHandle,
    root: PathBuf,
    creation: Arc<Mutex<()>>,
}

impl BrowserSession {
    pub fn new(app: AppHandle, root: PathBuf) -> Self {
        Self {
            app,
            root,
            creation: Arc::new(Mutex::new(())),
        }
    }
    fn label(id: &str, generation: u64) -> String {
        format!("provider-{id}-{generation}")
    }

    async fn ensure(
        &self,
        id: &str,
        spec: &BrowserSpec,
        generation: u64,
        visible: bool,
    ) -> Result<WebviewWindow, String> {
        let _guard = self.creation.lock().await;
        let label = Self::label(id, generation);
        if let Some(window) = self.app.get_webview_window(&label) {
            if visible {
                window.show().map_err(|_| "Could not show sign-in.")?;
                let _ = window.set_focus();
            }
            return Ok(window);
        }
        let profile = self
            .root
            .join("sessions")
            .join(format!("{id}-{generation}"));
        std::fs::create_dir_all(&profile)
            .map_err(|_| "Could not create the private sign-in session.")?;
        let popup_app = self.app.clone();
        let popup_profile = profile.clone();
        let popup_prefix = format!("auth-{id}-{generation}-");
        let popup_parent = label.clone();
        let window = WebviewWindowBuilder::new(
            &self.app,
            &label,
            WebviewUrl::External(spec.url.parse().map_err(|_| "Invalid provider URL.")?),
        )
        .title(format!(
            "{} | Sign in, then close this window to refresh AI Usage",
            id.to_uppercase()
        ))
        .inner_size(1100.0, 820.0)
        .visible(visible)
        .focused(visible)
        .data_directory(profile)
        .on_navigation(|url| url.scheme() == "https" || url.as_str() == "about:blank")
        .on_new_window(move |url, features| {
            let interactive = popup_app
                .get_webview_window(&popup_parent)
                .is_some_and(|window| window.is_visible().unwrap_or(false));
            if url.scheme() != "https" || !interactive {
                return NewWindowResponse::Deny;
            }
            let popup_label = format!(
                "{popup_prefix}{}",
                REQUEST_ID.fetch_add(1, Ordering::Relaxed)
            );
            match WebviewWindowBuilder::new(
                &popup_app,
                popup_label,
                WebviewUrl::External("about:blank".parse().expect("static URL")),
            )
            .title("Provider sign-in")
            .window_features(features)
            .data_directory(popup_profile.clone())
            .on_navigation(|url| url.scheme() == "https" || url.as_str() == "about:blank")
            .build()
            {
                Ok(window) => NewWindowResponse::Create { window },
                Err(_) => NewWindowResponse::Deny,
            }
        })
        .on_page_load(|window, payload| {
            if payload.event() == PageLoadEvent::Finished {
                let _ = window.eval("window.__aiUsageReady = true;");
            }
        })
        .build()
        .map_err(|_| {
            "Could not open the provider. Check that Microsoft Edge WebView2 is installed."
        })?;
        let close_window = window.clone();
        let close_app = self.app.clone();
        let close_id = id.to_string();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = close_window.hide();
                let app = close_app.clone();
                let id = close_id.clone();
                tauri::async_runtime::spawn(async move {
                    let service = app
                        .state::<Arc<crate::service::UsageService>>()
                        .inner()
                        .clone();
                    service.refresh(Some(&id)).await;
                });
            }
        });
        Ok(window)
    }

    pub async fn sign_in(
        &self,
        id: &str,
        spec: BrowserSpec,
        config: &ProviderConfig,
    ) -> Result<(), String> {
        self.ensure(id, &spec, config.session_generation, true)
            .await?;
        Ok(())
    }

    pub async fn read(
        &self,
        id: &str,
        spec: BrowserSpec,
        config: &ProviderConfig,
        cancelled: &AtomicBool,
    ) -> Result<Value, String> {
        if cancelled.load(Ordering::Acquire) {
            return Err("Refresh cancelled.".into());
        }
        let window = self
            .ensure(id, &spec, config.session_generation, false)
            .await?;
        if cancelled.load(Ordering::Acquire) {
            let _ = window.destroy();
            return Err("Refresh cancelled.".into());
        }
        let result = tokio::time::timeout(Duration::from_secs(40), self.read_window(&window, &spec, config, cancelled)).await
            .unwrap_or_else(|_| Err("The provider did not return usage in time. Open sign-in to finish login or a browser check, then refresh.".into()));
        if !window.is_visible().unwrap_or(false) {
            let _ = window.destroy();
        }
        result
    }

    async fn read_window(
        &self,
        window: &WebviewWindow,
        spec: &BrowserSpec,
        config: &ProviderConfig,
        cancelled: &AtomicBool,
    ) -> Result<Value, String> {
        let request = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let hosts = serde_json::to_string(spec.hosts).map_err(|_| "Invalid provider hosts.")?;
        let fields =
            serde_json::to_string(&config.fields).map_err(|_| "Invalid provider fields.")?;
        let script = format!(
            r#"(() => {{
          if (!{hosts}.includes(location.hostname) || location.protocol !== 'https:') return 'signin';
          if (!window.__aiUsageReady || document.readyState !== 'complete') return 'loading';
          window.__aiUsageResult = {{ request: {request}, pending: true }};
          Promise.resolve(({reader})({fields})).then(data => {{ window.__aiUsageResult = {{ request: {request}, data }}; }})
            .catch(error => {{ window.__aiUsageResult = {{ request: {request}, error: String(error && error.message || 'Usage request failed').slice(0, 400) }}; }});
          return 'started';
        }})()"#,
            reader = spec.script
        );
        for _ in 0..35 {
            if cancelled.load(Ordering::Acquire) {
                return Err("Refresh cancelled.".into());
            }
            let result = evaluate(window, &script).await?;
            if result.as_str() == Some("started") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        for _ in 0..60 {
            if cancelled.load(Ordering::Acquire) {
                return Err("Refresh cancelled.".into());
            }
            let v = evaluate(window, &format!("window.__aiUsageResult && window.__aiUsageResult.request === {request} ? window.__aiUsageResult : null")).await?;
            if v["request"] == json!(request) {
                if let Some(error) = v["error"].as_str() {
                    return Err(error.chars().take(400).collect());
                }
                if !v["data"].is_null() {
                    return Ok(v["data"].clone());
                }
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        Err("Sign in to the provider, then refresh. A login or verification page may still be open.".into())
    }

    pub fn close(&self, id: &str, generation: u64) {
        if let Some(window) = self.app.get_webview_window(&Self::label(id, generation)) {
            let _ = window.destroy();
        }
        let prefix = format!("auth-{id}-{generation}-");
        for (label, window) in self.app.webview_windows() {
            if label.starts_with(&prefix) {
                let _ = window.destroy();
            }
        }
    }

    pub async fn forget(
        &self,
        id: &str,
        spec: BrowserSpec,
        config: &ProviderConfig,
    ) -> Result<(), String> {
        let window = self
            .ensure(id, &spec, config.session_generation, false)
            .await?;
        window
            .clear_all_browsing_data()
            .map_err(|_| "Could not clear the provider's saved browser session.")?;
        self.close(id, config.session_generation);
        Ok(())
    }
}

async fn evaluate(window: &WebviewWindow, script: &str) -> Result<Value, String> {
    let (send, receive) = oneshot::channel();
    let sender = std::sync::Mutex::new(Some(send));
    window
        .eval_with_callback(script, move |data| {
            if let Ok(mut sender) = sender.lock() {
                if let Some(sender) = sender.take() {
                    let _ = sender.send(data);
                }
            }
        })
        .map_err(|_| "The provider window closed during refresh.")?;
    let data = tokio::time::timeout(Duration::from_secs(5), receive)
        .await
        .map_err(|_| "The provider browser is not responding.")?
        .map_err(|_| "The provider browser closed during refresh.")?;
    if data.len() > 128 * 1024 {
        return Err("The provider returned too much usage data.".into());
    }
    serde_json::from_str(&data).map_err(|_| "The provider returned unreadable usage data.".into())
}
