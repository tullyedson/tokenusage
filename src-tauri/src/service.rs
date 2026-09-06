use crate::{
    browser::BrowserSession,
    credentials::{self, ISecretStore, WindowsCredentialStore},
    model::*,
    persistence, provider_settings,
    providers::{self, ConnectionOutcome, FetchContext, IUsageProvider},
};
use futures_util::future::join_all;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

pub struct UsageService {
    pub browser: BrowserSession,
    app: AppHandle,
    path: PathBuf,
    settings: Mutex<Settings>,
    reports: Mutex<BTreeMap<String, ProviderReport>>,
    refresh_locks: BTreeMap<String, Mutex<()>>,
    active: std::sync::Mutex<BTreeMap<String, Arc<AtomicBool>>>,
    providers: Vec<Arc<dyn IUsageProvider>>,
    secrets: Arc<dyn ISecretStore>,
    startup_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub providers: Vec<ProviderDefinition>,
    pub settings: Settings,
    pub reports: Vec<ProviderReport>,
    pub startup_error: Option<String>,
    pub configured_secrets: BTreeMap<String, Vec<String>>,
}

impl UsageService {
    pub fn new(app: AppHandle, root: PathBuf) -> Self {
        let path = root.join("settings.json");
        let (settings, startup_error) = match persistence::load(&path) {
            Ok(settings) => (settings, None),
            Err(error) => (Settings::default(), Some(error)),
        };
        let providers = providers::registry();
        let refresh_locks = providers
            .iter()
            .map(|p| (p.definition().id.to_string(), Mutex::new(())))
            .collect();
        Self {
            browser: BrowserSession::new(app.clone(), root),
            app,
            path,
            settings: Mutex::new(settings),
            reports: Mutex::new(BTreeMap::new()),
            refresh_locks,
            active: std::sync::Mutex::new(BTreeMap::new()),
            providers,
            secrets: Arc::new(WindowsCredentialStore),
            startup_error,
        }
    }
    pub async fn bootstrap(&self) -> Bootstrap {
        let mut configured_secrets = BTreeMap::new();
        let mut startup_error = self.startup_error.clone();
        for provider in &self.providers {
            let definition = provider.definition();
            let mut saved = Vec::new();
            for field in definition.fields.iter().filter(|f| f.kind == "secret") {
                match self.secrets.get(definition.id, field.key) {
                    Ok(Some(_)) => saved.push(field.key.to_string()),
                    Ok(None) => (),
                    Err(error) => {
                        startup_error.get_or_insert(error);
                    }
                }
            }
            configured_secrets.insert(definition.id.into(), saved);
        }
        Bootstrap {
            providers: self.providers.iter().map(|p| p.definition()).collect(),
            settings: self.settings.lock().await.clone(),
            reports: self.reports().await,
            startup_error,
            configured_secrets,
        }
    }
    pub async fn reports(&self) -> Vec<ProviderReport> {
        self.reports.lock().await.values().cloned().collect()
    }
    pub async fn interval(&self) -> u64 {
        self.settings.lock().await.refresh_minutes
    }
    fn provider(&self, id: &str) -> Result<Arc<dyn IUsageProvider>, String> {
        self.providers
            .iter()
            .find(|p| p.definition().id == id)
            .cloned()
            .ok_or_else(|| "Unknown provider.".into())
    }
    fn writable(&self) -> Result<(), String> {
        self.startup_error
            .as_ref()
            .map_or(Ok(()), |e| Err(e.clone()))
    }
    pub async fn save_provider(
        &self,
        id: &str,
        enabled: bool,
        fields: BTreeMap<String, String>,
        secrets: BTreeMap<String, String>,
    ) -> Result<(), String> {
        self.writable()?;
        let provider = self.provider(id)?;
        let definition = provider.definition();
        let changes = provider_settings::validate(&definition, &fields, secrets)?;
        let mut settings = self.settings.lock().await;
        let mut next = settings.clone();
        let old = settings.providers.get(id).cloned().unwrap_or_default();
        next.providers.insert(
            id.into(),
            ProviderConfig {
                enabled,
                fields,
                session_generation: old.session_generation,
                revision: old.revision.saturating_add(1),
            },
        );
        // Cancel before touching the vault, including when the settings save later fails.
        if !changes.is_empty() {
            self.cancel(id);
        }
        credentials::update_with(self.secrets.as_ref(), id, &changes, || {
            persistence::save(&self.path, &next)
        })?;
        *settings = next;
        self.cancel(id);
        self.reports.lock().await.remove(id);
        drop(settings);
        if !enabled {
            self.browser.close(id, old.session_generation);
        }
        let _ = self.app.emit("settings-changed", ());
        Ok(())
    }
    pub async fn save_interval(&self, minutes: u64) -> Result<(), String> {
        self.writable()?;
        if !(1..=60).contains(&minutes) {
            return Err("Refresh every 1 to 60 minutes.".into());
        }
        let mut settings = self.settings.lock().await;
        let mut next = settings.clone();
        next.refresh_minutes = minutes;
        persistence::save(&self.path, &next)?;
        *settings = next;
        Ok(())
    }
    pub async fn sign_in(&self, id: &str) -> Result<String, String> {
        let provider = self.provider(id)?;
        let config = self
            .settings
            .lock()
            .await
            .providers
            .get(id)
            .cloned()
            .unwrap_or_default();
        if !config.enabled {
            return Err("Enable and save this provider before signing in.".into());
        }
        let outcome = provider
            .connect(
                &FetchContext {
                    browser: self.browser.clone(),
                    cancelled: Arc::new(AtomicBool::new(false)),
                    secrets: self.secrets.clone(),
                },
                &config,
            )
            .await?;
        match outcome {
            ConnectionOutcome::BrowserOpened => {
                Ok("Finish sign-in and close its window to read usage.".into())
            }
            ConnectionOutcome::Ready => {
                self.refresh(Some(id)).await;
                let reports = self.reports.lock().await;
                match reports.get(id) {
                    Some(report) if report.error.is_some() => {
                        Err(report.error.clone().unwrap_or_default())
                    }
                    Some(report) if report.snapshot.is_some() => {
                        Ok("Account connected. Usage updated.".into())
                    }
                    _ => Err("The connection changed. Connect the account again.".into()),
                }
            }
        }
    }
    pub async fn forget(&self, id: &str) -> Result<(), String> {
        self.writable()?;
        let provider = self.provider(id)?;
        self.cancel(id);
        if let Some(config) = self.settings.lock().await.providers.get(id) {
            self.browser.close(id, config.session_generation);
        }
        let lock = self.refresh_locks.get(id).ok_or("Unknown provider.")?;
        let _refresh = lock.lock().await;
        let config = self
            .settings
            .lock()
            .await
            .providers
            .get(id)
            .cloned()
            .unwrap_or_default();
        if let Some(spec) = provider.browser_spec() {
            self.browser.forget(id, spec, &config).await?;
        }
        let mut settings = self.settings.lock().await;
        let mut next = settings.clone();
        next.providers.insert(
            id.into(),
            ProviderConfig {
                session_generation: config.session_generation.saturating_add(1),
                ..ProviderConfig::default()
            },
        );
        let changes = provider
            .definition()
            .fields
            .iter()
            .filter(|f| f.kind == "secret")
            .map(|f| (f.key.to_string(), None))
            .collect();
        credentials::update_with(self.secrets.as_ref(), id, &changes, || {
            persistence::save(&self.path, &next)
        })?;
        *settings = next;
        self.reports.lock().await.remove(id);
        let _ = self.app.emit("settings-changed", ());
        Ok(())
    }
    pub async fn refresh(&self, only: Option<&str>) {
        let jobs = self
            .providers
            .iter()
            .filter(|p| only.is_none_or(|id| p.definition().id == id))
            .map(|p| self.refresh_one(p.clone()));
        join_all(jobs).await;
    }
    fn cancel(&self, id: &str) {
        if let Ok(active) = self.active.lock() {
            if let Some(cancelled) = active.get(id) {
                cancelled.store(true, Ordering::Release);
            }
        }
    }
    async fn refresh_one(&self, provider: Arc<dyn IUsageProvider>) {
        let id = provider.definition().id;
        let Some(lock) = self.refresh_locks.get(id) else {
            return;
        };
        let _refresh = lock.lock().await;
        let settings = self.settings.lock().await;
        let Some(config) = settings.providers.get(id).filter(|c| c.enabled).cloned() else {
            return;
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Ok(mut active) = self.active.lock() {
            active.insert(id.into(), cancelled.clone());
        }
        drop(settings);
        {
            let mut reports = self.reports.lock().await;
            let report = reports
                .entry(id.into())
                .or_insert_with(|| ProviderReport::empty(id));
            report.refreshing = true;
        }
        let _ = self.app.emit("usage-updated", self.reports().await);
        let context = FetchContext {
            browser: self.browser.clone(),
            cancelled,
            secrets: self.secrets.clone(),
        };
        let result = provider.fetch(&context, &config).await;
        if let Ok(mut active) = self.active.lock() {
            active.remove(id);
        }
        let settings = self.settings.lock().await;
        if settings.providers.get(id) != Some(&config) {
            return;
        }
        // A completed request can race cancellation. Do not publish a reading from a
        // transient replacement key if its settings transaction was rolled back.
        let result = if context.cancelled.load(Ordering::Acquire) {
            Err("Refresh cancelled.".into())
        } else {
            result
        };
        {
            let mut reports = self.reports.lock().await;
            let report = reports
                .entry(id.into())
                .or_insert_with(|| ProviderReport::empty(id));
            report.refreshing = false;
            report.attempted_at = Some(chrono::Utc::now().timestamp());
            match result {
                Ok(snapshot) => {
                    report.snapshot = Some(snapshot);
                    report.updated_at = report.attempted_at;
                    report.error = None;
                }
                Err(error) => {
                    report.error = Some(error);
                }
            }
        }
        drop(settings);
        let _ = self.app.emit("usage-updated", self.reports().await);
    }
}
