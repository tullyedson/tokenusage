use crate::{
    browser::BrowserSession,
    credentials::{self, ISecretStore, WindowsCredentialStore},
    model::*,
    persistence, provider_settings,
    providers::{self, ConnectionOutcome, FetchContext, IUsageProvider},
    routing::{
        config::{self, AccountRouting, ModelPool, RouterSettings},
        engine::{InferenceContext, InferenceDefinition, RouteAccount},
        RouterRuntime, RouterStatus,
    },
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
    pub router: RouterRuntime,
    app: AppHandle,
    path: PathBuf,
    settings: Mutex<Settings>,
    reports: Mutex<BTreeMap<String, ProviderReport>>,
    refresh_locks: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
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
    pub inference: BTreeMap<String, InferenceDefinition>,
    pub router: RouterStatus,
}

impl UsageService {
    pub fn new(app: AppHandle, root: PathBuf) -> Self {
        let path = root.join("settings.json");
        let (settings, startup_error) = match persistence::load(&path) {
            Ok(settings) => (settings, None),
            Err(error) => (Settings::default(), Some(error)),
        };
        let secrets: Arc<dyn ISecretStore> = Arc::new(WindowsCredentialStore);
        Self {
            browser: BrowserSession::new(app.clone(), root),
            router: RouterRuntime::new(secrets.clone()),
            app,
            path,
            settings: Mutex::new(settings),
            reports: Mutex::new(BTreeMap::new()),
            refresh_locks: Mutex::new(BTreeMap::new()),
            active: std::sync::Mutex::new(BTreeMap::new()),
            providers: providers::registry(),
            secrets,
            startup_error,
        }
    }
    pub async fn initialize_router(&self) {
        let settings = self.settings.lock().await;
        self.sync_router(&settings).await;
    }
    async fn sync_router(&self, settings: &Settings) {
        let accounts = settings
            .providers
            .iter()
            .filter_map(|(id, config)| {
                let provider = self.provider(config.provider_type(id)).ok()?.inference()?;
                Some(RouteAccount {
                    id: id.clone(),
                    config: config.clone(),
                    provider,
                    serial: Arc::new(Mutex::new(())),
                })
            })
            .collect();
        self.router.apply(settings.routing.clone(), accounts).await;
    }
    pub async fn bootstrap(&self) -> Bootstrap {
        let settings = self.settings.lock().await.clone();
        let mut configured_secrets = BTreeMap::new();
        let mut startup_error = self.startup_error.clone();
        let mut accounts = settings.providers.clone();
        for p in &self.providers {
            accounts.entry(p.definition().id.into()).or_default();
        }
        for (id, config) in &accounts {
            let Ok(provider) = self.provider(config.provider_type(id)) else {
                continue;
            };
            let mut saved = Vec::new();
            for field in provider
                .definition()
                .fields
                .iter()
                .filter(|f| f.kind == "secret")
            {
                match self.secrets.get(id, field.key) {
                    Ok(Some(_)) => saved.push(field.key.to_string()),
                    Ok(None) => (),
                    Err(error) => {
                        startup_error.get_or_insert(error);
                    }
                }
            }
            configured_secrets.insert(id.clone(), saved);
        }
        Bootstrap {
            providers: self.providers.iter().map(|p| p.definition()).collect(),
            settings,
            reports: self.reports().await,
            startup_error,
            configured_secrets,
            inference: self
                .providers
                .iter()
                .filter_map(|p| {
                    p.inference()
                        .map(|i| (p.definition().id.into(), i.definition()))
                })
                .collect(),
            router: self.router.status().await,
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
    async fn account(&self, id: &str) -> Result<(Arc<dyn IUsageProvider>, ProviderConfig), String> {
        if !config::valid_account(id) {
            return Err("Invalid account ID.".into());
        }
        let config = self
            .settings
            .lock()
            .await
            .providers
            .get(id)
            .cloned()
            .unwrap_or_default();
        Ok((self.provider(config.provider_type(id))?, config))
    }
    async fn refresh_lock(&self, id: &str) -> Arc<Mutex<()>> {
        self.refresh_locks
            .lock()
            .await
            .entry(id.into())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
    pub async fn add_account(&self, provider_type: &str) -> Result<String, String> {
        self.writable()?;
        let definition = self.provider(provider_type)?.definition();
        let mut settings = self.settings.lock().await;
        if settings.providers.len() >= 64 {
            return Err("At most 64 accounts are supported.".into());
        }
        let mut bytes = [0u8; 12];
        getrandom::fill(&mut bytes).map_err(|_| "Could not create an account identifier.")?;
        let id = format!(
            "account-{}",
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
        let mut next = settings.clone();
        next.providers.insert(
            id.clone(),
            ProviderConfig {
                provider_type: provider_type.into(),
                label: format!("{} account", definition.name),
                ..Default::default()
            },
        );
        persistence::save(&self.path, &next)?;
        *settings = next;
        self.sync_router(&settings).await;
        let _ = self.app.emit("settings-changed", ());
        Ok(id)
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn save_provider(
        &self,
        id: &str,
        enabled: bool,
        label: String,
        fields: BTreeMap<String, String>,
        secrets: BTreeMap<String, String>,
        routing: AccountRouting,
    ) -> Result<(), String> {
        self.writable()?;
        if !config::valid_account(id) || label.len() > 100 || label.chars().any(char::is_control) {
            return Err("Use a short account label without control characters.".into());
        }
        let mut settings = self.settings.lock().await;
        let old = settings.providers.get(id).cloned().unwrap_or_default();
        let provider = self.provider(old.provider_type(id))?;
        let changes = provider_settings::validate(&provider.definition(), &fields, secrets)?;
        let mut updated = ProviderConfig {
            enabled,
            label,
            fields,
            routing,
            revision: old.revision.saturating_add(1),
            ..old.clone()
        };
        if let Some(inference) = provider.inference() {
            inference.validate(&updated)?;
        } else {
            updated.routing.enabled = false;
        }
        let mut next = settings.clone();
        next.providers.insert(id.into(), updated);
        self.cancel(id);
        self.router.engine.cancel_requests().await;
        if let Err(error) = credentials::update_with(self.secrets.as_ref(), id, &changes, || {
            persistence::save(&self.path, &next)
        }) {
            self.sync_router(&settings).await;
            return Err(error);
        }
        *settings = next;
        self.cancel(id);
        self.reports.lock().await.remove(id);
        self.sync_router(&settings).await;
        drop(settings);
        if !enabled {
            self.browser.close(id, old.session_generation);
        }
        let _ = self.app.emit("settings-changed", ());
        Ok(())
    }
    pub async fn save_routing(
        &self,
        mut routing: RouterSettings,
        client_token: String,
    ) -> Result<(), String> {
        let client_token = zeroize::Zeroizing::new(client_token);
        self.writable()?;
        let mut settings = self.settings.lock().await;
        routing.pools = settings.routing.pools.clone();
        config::validate(&routing, &settings.providers.keys().cloned().collect())?;
        if !client_token.is_empty() && !crate::routing::server_token_valid(&client_token) {
            return Err("Client keys need 32 to 256 letters, numbers, underscores or hyphens. Use Generate key.".into());
        }
        if routing.enabled
            && client_token.is_empty()
            && self.secrets.get("router", "client_token")?.is_none()
        {
            return Err("Generate and save a client key first.".into());
        }
        let mut next = settings.clone();
        next.routing = routing;
        let changes = if client_token.is_empty() {
            BTreeMap::new()
        } else {
            BTreeMap::from([("client_token".into(), Some(client_token))])
        };
        self.router.engine.cancel_requests().await;
        if let Err(error) =
            credentials::update_with(self.secrets.as_ref(), "router", &changes, || {
                persistence::save(&self.path, &next)
            })
        {
            self.sync_router(&settings).await;
            return Err(error);
        }
        *settings = next;
        self.sync_router(&settings).await;
        let _ = self.app.emit("settings-changed", ());
        Ok(())
    }
    pub async fn save_model_pools(&self, pools: Vec<ModelPool>) -> Result<(), String> {
        self.writable()?;
        let mut settings = self.settings.lock().await;
        config::validate_pools(&pools, &settings.providers.keys().cloned().collect())?;
        let mut next = settings.clone();
        next.routing.pools = pools;
        persistence::save(&self.path, &next)?;
        *settings = next;
        self.sync_router(&settings).await;
        let _ = self.app.emit("settings-changed", ());
        Ok(())
    }
    pub async fn discover_models(&self, id: &str) -> Result<Vec<String>, String> {
        let (provider, config) = self.account(id).await?;
        let cancelled = self.router.engine.cancellation().await;
        if !config.enabled {
            return Err("Enable and save this account before listing models.".into());
        }
        let adapter = provider
            .inference()
            .ok_or("This connection supports usage monitoring only.")?;
        let context = InferenceContext {
            account_id: id,
            config: &config,
            secrets: self.secrets.as_ref(),
            client: &self.router.engine.client,
            now: chrono::Utc::now().timestamp(),
            session_id: None,
        };
        let result = tokio::select! { result = adapter.models(&context) => result?, _ = cancelled.cancelled() => return Err("The account changed. List models again.".into()) };
        if cancelled.is_cancelled() || self.settings.lock().await.providers.get(id) != Some(&config)
        {
            return Err("The account changed. List models again.".into());
        }
        Ok(result.into_iter().map(|model| model.id).collect())
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
        let (provider, config) = self.account(id).await?;
        if !config.enabled {
            return Err("Enable and save this account before signing in.".into());
        }
        let outcome = provider
            .connect(
                &FetchContext {
                    account_id: id.into(),
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
        let (provider, config) = self.account(id).await?;
        self.cancel(id);
        self.router.engine.cancel_requests().await;
        self.browser.close(id, config.session_generation);
        let lock = self.refresh_lock(id).await;
        let _refresh = lock.lock().await;
        let mut settings = self.settings.lock().await;
        self.router.engine.cancel_requests().await;
        let config = settings.providers.get(id).cloned().unwrap_or_default();
        if let Some(spec) = provider.browser_spec() {
            if let Err(error) = self.browser.forget(id, spec, &config).await {
                self.sync_router(&settings).await;
                return Err(error);
            }
        }
        let mut next = settings.clone();
        next.providers.insert(
            id.into(),
            ProviderConfig {
                provider_type: config.provider_type.clone(),
                label: config.label.clone(),
                session_generation: config.session_generation.saturating_add(1),
                revision: config.revision.saturating_add(1),
                ..Default::default()
            },
        );
        let changes = provider
            .definition()
            .fields
            .iter()
            .filter(|f| f.kind == "secret")
            .map(|f| (f.key.to_string(), None))
            .collect();
        if let Err(error) = credentials::update_with(self.secrets.as_ref(), id, &changes, || {
            persistence::save(&self.path, &next)
        }) {
            self.sync_router(&settings).await;
            return Err(error);
        }
        *settings = next;
        self.reports.lock().await.remove(id);
        self.sync_router(&settings).await;
        let _ = self.app.emit("settings-changed", ());
        Ok(())
    }
    pub async fn refresh(&self, only: Option<&str>) {
        let accounts = self.settings.lock().await.providers.clone();
        let jobs = accounts
            .iter()
            .filter(|(id, config)| config.enabled && only.is_none_or(|only| only == id.as_str()))
            .filter_map(|(id, config)| {
                self.provider(config.provider_type(id))
                    .ok()
                    .map(|provider| self.refresh_one(id.clone(), provider))
            });
        join_all(jobs).await;
    }
    fn cancel(&self, id: &str) {
        if let Ok(active) = self.active.lock() {
            if let Some(cancelled) = active.get(id) {
                cancelled.store(true, Ordering::Release);
            }
        }
    }
    async fn refresh_one(&self, id: String, provider: Arc<dyn IUsageProvider>) {
        let lock = self.refresh_lock(&id).await;
        let _refresh = lock.lock().await;
        let settings = self.settings.lock().await;
        let Some(config) = settings.providers.get(&id).filter(|c| c.enabled).cloned() else {
            return;
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Ok(mut active) = self.active.lock() {
            active.insert(id.clone(), cancelled.clone());
        }
        drop(settings);
        self.reports
            .lock()
            .await
            .entry(id.clone())
            .or_insert_with(|| ProviderReport::empty(&id))
            .refreshing = true;
        let _ = self.app.emit("usage-updated", self.reports().await);
        let context = FetchContext {
            account_id: id.clone(),
            browser: self.browser.clone(),
            cancelled,
            secrets: self.secrets.clone(),
        };
        let result = provider.fetch(&context, &config).await;
        if let Ok(mut active) = self.active.lock() {
            active.remove(&id);
        }
        let settings = self.settings.lock().await;
        if settings.providers.get(&id) != Some(&config) {
            return;
        }
        let result = if context.cancelled.load(Ordering::Acquire) {
            Err("Refresh cancelled.".into())
        } else {
            result
        };
        {
            let mut reports = self.reports.lock().await;
            let report = reports
                .entry(id.clone())
                .or_insert_with(|| ProviderReport::empty(&id));
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
