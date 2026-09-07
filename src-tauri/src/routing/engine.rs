use super::{
    allowance::{AllowanceProbe, ProbeOnDrop},
    catalog::{self, CatalogContext, ModelCatalog, ModelLibrary},
    config::{RouteMode, RouterSettings},
    distribution::LoadDistribution,
    metadata::InferenceModel,
    metrics::{AllowanceSnapshot, TokenUsage},
    reports::{RequestStatus, RequestTrace, RouteTarget, RoutingReport, RoutingReports},
};
use crate::{credentials::ISecretStore, model::ProviderConfig};
use async_trait::async_trait;
use axum::{
    body::{Body, Bytes},
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceDefinition {
    pub description: &'static str,
}

pub struct InferenceContext<'a> {
    pub account_id: &'a str,
    pub config: &'a ProviderConfig,
    pub secrets: &'a dyn ISecretStore,
    pub client: &'a reqwest::Client,
    pub now: i64,
    pub session_id: Option<&'a str>,
}

pub struct PreparedRequest {
    pub url: reqwest::Url,
    pub body: Value,
    pub key: Option<Zeroizing<String>>,
    pub headers: reqwest::header::HeaderMap,
    pub allowance_before: Option<AllowanceSnapshot>,
}

#[derive(Clone, Debug)]
pub enum RouteFailure {
    // Retry eligibility is rechecked at this time. It is never a fabricated balance.
    Unavailable {
        reason: &'static str,
        retry_at: i64,
        account_wide: bool,
    },
    Invalid(&'static str),
    Failed(&'static str),
}

/// Implementations must enforce local, free, or included-only billing at the provider
/// boundary. A usage reading alone is not permission to spend API wallet credits.
#[async_trait]
pub trait IInferenceProvider: Send + Sync {
    fn definition(&self) -> InferenceDefinition;
    fn validate(&self, config: &ProviderConfig) -> Result<(), String>;
    async fn models(&self, context: &InferenceContext<'_>) -> Result<Vec<InferenceModel>, String>;
    async fn prepare(
        &self,
        context: &InferenceContext<'_>,
        request: &Value,
        upstream: &str,
    ) -> Result<PreparedRequest, RouteFailure>;
    /// Optional reporting read. It never grants eligibility or changes routing policy.
    async fn observe_allowance(
        &self,
        _context: &InferenceContext<'_>,
    ) -> Option<AllowanceSnapshot> {
        None
    }
}

#[derive(Clone)]
pub struct RouteAccount {
    pub id: String,
    pub config: ProviderConfig,
    pub provider: Arc<dyn IInferenceProvider>,
    pub serial: Arc<Mutex<()>>,
}

#[derive(Clone)]
struct Configuration {
    settings: RouterSettings,
    accounts: Vec<RouteAccount>,
    cancelled: CancellationToken,
    distribution: Arc<LoadDistribution>,
}

struct CallerIdentity {
    session: String,
    affinity: Option<String>,
}

pub struct RouterEngine {
    config: RwLock<Configuration>,
    pub client: reqwest::Client,
    pub secrets: Arc<dyn ISecretStore>,
    cooldowns: Mutex<BTreeMap<(String, String), i64>>,
    concurrency: Arc<Semaphore>,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    catalog: ModelCatalog,
    reports: Arc<RoutingReports>,
    allowance_probes: Arc<Semaphore>,
}

impl RouterEngine {
    pub fn new(secrets: Arc<dyn ISecretStore>) -> Self {
        Self {
            config: RwLock::new(Configuration {
                settings: Default::default(),
                accounts: vec![],
                cancelled: CancellationToken::new(),
                distribution: Arc::new(LoadDistribution::default()),
            }),
            client: super::network::client_builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(180))
                .build()
                .expect("Could not initialize the model HTTP client"),
            secrets,
            cooldowns: Mutex::new(BTreeMap::new()),
            concurrency: Arc::new(Semaphore::new(8)),
            clock: Arc::new(|| chrono::Utc::now().timestamp()),
            catalog: ModelCatalog::default(),
            reports: Arc::new(RoutingReports::default()),
            allowance_probes: Arc::new(Semaphore::new(4)),
        }
    }
    #[cfg(test)]
    pub fn with_clock(mut self, clock: Arc<dyn Fn() -> i64 + Send + Sync>) -> Self {
        self.clock = clock;
        self
    }

    pub async fn configure(&self, settings: RouterSettings, mut accounts: Vec<RouteAccount>) {
        let mut config = self.config.write().await;
        config.cancelled.cancel();
        // A save must not let a new request overlap a cancelled old stream on an account.
        for account in &mut accounts {
            if let Some(old) = config.accounts.iter().find(|a| a.id == account.id) {
                account.serial = old.serial.clone();
            }
        }
        *config = Configuration {
            settings,
            accounts,
            cancelled: CancellationToken::new(),
            distribution: Arc::new(LoadDistribution::default()),
        };
        self.cooldowns.lock().await.clear();
    }

    pub async fn model_library(&self, force: bool) -> Result<ModelLibrary, String> {
        let config = self.config.read().await.clone();
        let catalogs = self
            .catalog
            .read(
                CatalogContext {
                    accounts: &config.accounts,
                    secrets: self.secrets.as_ref(),
                    client: &self.client,
                    cancelled: &config.cancelled,
                    now: (self.clock)(),
                },
                force,
            )
            .await?;
        if config.cancelled.is_cancelled() {
            return Err("Account settings changed. Refresh models again.".into());
        }
        Ok(catalog::library(
            &config.accounts,
            catalogs,
            &config.settings.pools,
        ))
    }
    pub async fn model_list(&self) -> Result<Value, String> {
        let library = self.model_library(false).await?;
        Ok(
            json!({"object":"list","data":library.pools.into_iter().filter(|p| p.available && !p.pool.members.is_empty()).map(|p| json!({"id":p.pool.name,"object":"model","created":0,"owned_by":"ai-usage","routing_mode":p.pool.mode,"context_length":p.limits.context,"max_output_tokens":p.limits.output,"limit":p.limits})).collect::<Vec<_>>()}),
        )
    }
    pub async fn cancel_requests(&self) {
        self.config.read().await.cancelled.cancel();
    }
    pub fn routing_report(&self) -> RoutingReport {
        self.reports.snapshot()
    }
    pub fn clear_routing_history(&self) {
        self.reports.clear_history();
    }
    pub async fn cancellation(&self) -> CancellationToken {
        self.config.read().await.cancelled.clone()
    }

    pub async fn route(self: &Arc<Self>, request: Value) -> Response {
        self.route_with_session(request, None).await
    }

    pub async fn route_with_session(
        self: &Arc<Self>,
        request: Value,
        session: Option<&str>,
    ) -> Response {
        self.route_with_identity(request, session, None).await
    }

    pub async fn route_with_identity(
        self: &Arc<Self>,
        request: Value,
        session: Option<&str>,
        instance: Option<&str>,
    ) -> Response {
        self.route_measured(request, session, instance, None).await
    }

    pub async fn route_measured(
        self: &Arc<Self>,
        request: Value,
        session: Option<&str>,
        instance: Option<&str>,
        request_bytes: Option<u64>,
    ) -> Response {
        let caller = match instance {
            Some(value) if valid_session(value) => Some(format!("instance:{value}")),
            Some(_) => return error(StatusCode::BAD_REQUEST, "invalid_instance", "Use an opaque instance ID of at most 200 letters, numbers, underscores or hyphens."),
            None => session.map(|value| format!("session:{value}")),
        };
        let requested = match validate_request(&request) {
            Ok(model) => model.to_owned(),
            Err(message) => return error(StatusCode::BAD_REQUEST, "invalid_request", message),
        };
        let session = match session {
            Some(value) if valid_session(value) => value.to_owned(),
            Some(_) => return error(
                StatusCode::BAD_REQUEST,
                "invalid_session",
                "Use an opaque session ID of at most 200 letters, numbers, underscores or hyphens.",
            ),
            None => {
                let mut bytes = [0u8; 16];
                if getrandom::fill(&mut bytes).is_err() {
                    return error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "session_unavailable",
                        "Could not create a request session.",
                    );
                }
                format!(
                    "ai-usage-{}",
                    bytes
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>()
                )
            }
        };
        let config = self.config.read().await.clone();
        if !config.settings.enabled {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "router_disabled",
                "Routing is disabled.",
            );
        }
        let permit = match self.concurrency.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                return error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "router_busy",
                    "Eight requests are already active. Retry later.",
                )
            }
        };
        let started = self
            .reports
            .begin(requested, request["stream"].as_bool().unwrap_or(false));
        let request_id = started.id().to_owned();
        started.request_bytes(request_bytes);
        let mut trace = Some(started);
        let mut response = self
            .route_tracked(
                request,
                CallerIdentity {
                    session,
                    affinity: caller,
                },
                config,
                permit,
                &mut trace,
            )
            .await;
        if let Some(trace) = trace {
            let status = response.status().as_u16();
            let (outcome, message) = match status {
                200..=299 => (RequestStatus::Completed, "Completion received."),
                503 => (RequestStatus::Cancelled, "Routing configuration changed."),
                404 => (RequestStatus::Failed, "No model pool has this name. Refresh models or check the client model name."),
                429 => (RequestStatus::Failed, "Every pool entry is unavailable. Expand the routing steps for reasons and retry times."),
                504 => (RequestStatus::Failed, "The routing request timed out."),
                400 => (RequestStatus::Failed, "The pool is empty or the selected provider cannot serve this request."),
                _ => (RequestStatus::Failed, "The upstream request failed or returned an invalid response. It was not replayed."),
            };
            trace.finish(outcome, Some(status), message);
        }
        if let Ok(value) = HeaderValue::from_str(&request_id) {
            response
                .headers_mut()
                .insert("x-ai-usage-request-id", value);
        }
        response
    }

    async fn route_tracked(
        self: &Arc<Self>,
        request: Value,
        identity: CallerIdentity,
        config: Configuration,
        permit: tokio::sync::OwnedSemaphorePermit,
        trace: &mut Option<RequestTrace>,
    ) -> Response {
        let requested = request["model"]
            .as_str()
            .expect("Validated model")
            .to_owned();
        let mut reasons = Vec::new();
        let mut next_retry = None;
        let deadline = tokio::time::sleep(Duration::from_secs(180));
        tokio::pin!(deadline);
        let pool = if let Some(pool) = config
            .settings
            .pools
            .iter()
            .find(|pool| pool.name == requested)
        {
            pool.clone()
        } else {
            let library = tokio::select! {
                _ = config.cancelled.cancelled() => return cancelled_response(),
                _ = &mut deadline => return error(StatusCode::GATEWAY_TIMEOUT, "timeout", "Model discovery timed out."),
                result = self.model_library(false) => match result { Ok(library) => library, Err(_) => return cancelled_response() },
            };
            match library.pools.into_iter().find(|pool| pool.pool.name == requested) {
                Some(pool) => pool.pool,
                None => return error(StatusCode::NOT_FOUND, "model_not_found", "No model or pool has this name. Open Models in AI Usage and refresh the provider catalogs."),
            }
        };
        if pool.members.is_empty() {
            return error(
                StatusCode::BAD_REQUEST,
                "empty_pool",
                "This model pool is empty. Add provider models on the Models page, then save.",
            );
        }
        let catalogs = self.catalog.cached(&config.accounts, (self.clock)()).await;
        let limits = catalog::library(&config.accounts, catalogs, std::slice::from_ref(&pool))
            .pools
            .into_iter()
            .find(|entry| entry.pool.name == pool.name)
            .and_then(|entry| entry.limits.context);
        trace.as_ref().expect("Active report").context_limit(limits);
        let mut remaining = (0..pool.members.len()).collect::<Vec<_>>();
        let mut tried = 0;
        while !remaining.is_empty() {
            if config.cancelled.is_cancelled() {
                return cancelled_response();
            }
            let candidates = if pool.mode == RouteMode::LoadDistribution {
                let now = (self.clock)();
                let cooldowns = self.cooldowns.lock().await;
                remaining
                    .iter()
                    .copied()
                    .filter(|index| {
                        let member = &pool.members[*index];
                        config.accounts.iter().any(|account| {
                            account.id == member.account_id
                                && catalog::account_issue(account).is_none()
                        }) && ![&member.model, &String::new()].into_iter().any(|model| {
                            cooldowns
                                .get(&(member.account_id.clone(), model.clone()))
                                .is_some_and(|until| *until > now)
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![remaining[0]]
            };
            let assignment =
                config
                    .distribution
                    .reserve(&pool, identity.affinity.as_deref(), &candidates);
            let index = assignment
                .as_ref()
                .map_or(remaining[0], |value| value.index);
            let sticky = assignment.as_ref().is_some_and(|value| value.sticky);
            let selection = if pool.mode == RouteMode::Failover {
                "failover"
            } else if sticky {
                "sticky"
            } else {
                "distributed"
            };
            remaining.retain(|candidate| *candidate != index);
            let member = &pool.members[index];
            trace
                .as_ref()
                .expect("Active report")
                .selection(pool.mode, tried);
            tried += 1;
            let mut target = RouteTarget {
                account_id: member.account_id.clone(),
                account_label: String::new(),
                provider_id: String::new(),
                model: member.model.clone(),
                position: index + 1,
            };
            let Some(account) = config
                .accounts
                .iter()
                .find(|account| account.id == member.account_id)
            else {
                trace.as_ref().expect("Active report").attempt(
                    target,
                    "skipped",
                    "Account is not connected for routing. Open Settings.",
                    None,
                );
                reasons.push(json!({"account":member.account_id,"model":member.model,"reason":"Account is not connected for routing. Open Settings."}));
                continue;
            };
            target.account_label = account.config.label.clone();
            target.provider_id = account.config.provider_type(&account.id).to_owned();
            if let Some(reason) = catalog::account_issue(account) {
                let report_reason = if !account.config.enabled {
                    "Account is disabled. Enable it in Settings."
                } else if !account.config.routing.enabled {
                    "This account is excluded from model pools. Enable it in Settings."
                } else {
                    "Account setup is incomplete or invalid. Open Settings."
                };
                trace.as_ref().expect("Active report").attempt(
                    target,
                    "skipped",
                    report_reason,
                    None,
                );
                reasons.push(json!({"account":account.id,"model":member.model,"reason":reason}));
                continue;
            }
            let now = (self.clock)();
            let cooldown = {
                let cooldowns = self.cooldowns.lock().await;
                [
                    cooldowns.get(&(account.id.clone(), member.model.clone())),
                    cooldowns.get(&(account.id.clone(), String::new())),
                ]
                .into_iter()
                .flatten()
                .copied()
                .max()
                .filter(|until| *until > now)
            };
            if let Some(until) = cooldown {
                trace.as_ref().expect("Active report").attempt(
                    target,
                    "skipped",
                    "Waiting for the next quota or availability check.",
                    Some(until),
                );
                next_retry = Some(next_retry.map_or(until, |old: i64| old.min(until)));
                reasons.push(json!({"account":account.id,"model":member.model,"reason":"Waiting for the next quota or availability check.","retry_at":until}));
                continue;
            }
            let cancelled = &config.cancelled;
            trace.as_ref().expect("Active report").progress(
                target.clone(),
                RequestStatus::Waiting,
                "Waiting for this account's earlier request to finish.",
            );
            let serial = tokio::select! {
                biased;
                _ = cancelled.cancelled() => return cancelled_response(),
                _ = &mut deadline => return error(StatusCode::GATEWAY_TIMEOUT, "timeout", "Routing timed out."),
                lock = account.serial.clone().lock_owned() => lock,
            };
            // A preceding request might have exhausted the account while this one waited.
            let now = (self.clock)();
            let blocked = self
                .cooldowns
                .lock()
                .await
                .iter()
                .filter(|((id, m), until)| {
                    id == &account.id && (m.is_empty() || m == &member.model) && **until > now
                })
                .map(|(_, until)| *until)
                .max();
            if let Some(until) = blocked {
                trace.as_ref().expect("Active report").attempt(
                    target,
                    "skipped",
                    "A preceding request reached this account's limit.",
                    Some(until),
                );
                next_retry = Some(next_retry.map_or(until, |old: i64| old.min(until)));
                reasons.push(json!({"account":account.id,"model":member.model,"reason":"A preceding request reached this account's limit.","retry_at":until}));
                continue;
            }
            let context = InferenceContext {
                account_id: &account.id,
                config: &account.config,
                secrets: self.secrets.as_ref(),
                client: &self.client,
                now,
                session_id: Some(&identity.session),
            };
            trace.as_ref().expect("Active report").progress(
                target.clone(),
                RequestStatus::Checking,
                "Checking model and included allowance eligibility.",
            );
            let prepared = tokio::select! {
                biased;
                _ = cancelled.cancelled() => return cancelled_response(),
                _ = &mut deadline => return error(StatusCode::GATEWAY_TIMEOUT, "timeout", "Routing timed out."),
                result = account.provider.prepare(&context, &request, &member.model) => result,
            };
            let mut allowance_before = None;
            let result = match prepared {
                Ok(mut prepared) => {
                    allowance_before = prepared.allowance_before.take();
                    trace.as_ref().expect("Active report").progress(
                        target.clone(),
                        RequestStatus::Connecting,
                        "Contacting this provider. Waiting for its response.",
                    );
                    tokio::select! {
                    biased;
                    _ = cancelled.cancelled() => return cancelled_response(),
                    _ = &mut deadline => return error(StatusCode::GATEWAY_TIMEOUT, "timeout", "Routing timed out."),
                    result = send(&self.client, prepared, (self.clock)()) => result,
                    }
                }
                Err(failure) => Err(failure),
            };
            match result {
                Ok(mut response) => {
                    if cancelled.is_cancelled() {
                        return cancelled_response();
                    }
                    let report = trace.as_ref().expect("Active report");
                    report.response_started();
                    let probe = ProbeOnDrop(allowance_before.map(|before| AllowanceProbe {
                        before,
                        account: account.clone(),
                        client: self.client.clone(),
                        secrets: self.secrets.clone(),
                        clock: self.clock.clone(),
                        cancelled: cancelled.clone(),
                        slots: self.allowance_probes.clone(),
                        update: report.allowance_update(),
                    }));
                    trace.as_ref().expect("Active report").attempt(
                        target.clone(),
                        "selected",
                        if pool.mode == RouteMode::Failover {
                            "Provider accepted the request."
                        } else if sticky {
                            "Kept this caller on its assigned server."
                        } else {
                            "Assigned an available server by current load and caller distribution."
                        },
                        None,
                    );
                    let stream = request["stream"].as_bool().unwrap_or(false);
                    let content_type = response
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");
                    if stream && !content_type.starts_with("text/event-stream") {
                        return error(
                            StatusCode::BAD_GATEWAY,
                            "invalid_upstream",
                            "The selected server did not return an event stream.",
                        );
                    }
                    let mut output = if stream {
                        let stream_trace = trace.take().expect("Active report");
                        stream_trace.progress(
                            target,
                            RequestStatus::Streaming,
                            "Receiving the provider's response stream.",
                        );
                        let cancel = cancelled.clone();
                        let alias = requested.clone();
                        let (sender, receiver) =
                            tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(4);
                        // The worker owns upstream I/O and permits so cancellation also
                        // releases them when a downstream client stops reading.
                        tokio::spawn(async move {
                            let _probe = probe;
                            let (_serial, _permit, _assignment) = (serial, permit, assignment);
                            let mut aliases = super::stream::AliasStream::default();
                            loop {
                                let result = tokio::select! {
                                    _ = cancel.cancelled() => { let _ = sender.try_send(Err(std::io::Error::other("Routing configuration changed."))); stream_trace.finish(RequestStatus::Cancelled, Some(200), "Routing configuration changed during the stream."); return; },
                                    _ = sender.closed() => return,
                                    result = response.chunk() => result,
                                };
                                let finished = matches!(result, Ok(None));
                                let item = match result {
                                    Ok(Some(bytes)) => {
                                        stream_trace.response_bytes(bytes.len());
                                        aliases.push(&bytes, &alias).map(Bytes::from)
                                    }
                                    Ok(None) => Ok(Bytes::from(aliases.finish(&alias))),
                                    Err(_) => {
                                        let _ = sender.try_send(Err(std::io::Error::other("Upstream stream interrupted; request was not replayed.")));
                                        stream_trace.finish(RequestStatus::Failed, Some(200), "Upstream stream interrupted; request was not replayed.");
                                        return;
                                    }
                                };
                                stream_trace.tokens(aliases.tokens());
                                let failed = item.is_err();
                                if item.as_ref().is_ok_and(Bytes::is_empty) && !finished {
                                    continue;
                                }
                                tokio::select! {
                                    _ = cancel.cancelled() => { let _ = sender.try_send(Err(std::io::Error::other("Routing configuration changed."))); stream_trace.finish(RequestStatus::Cancelled, Some(200), "Routing configuration changed during the stream."); return; },
                                    result = sender.send(item) => if result.is_err() { return; },
                                }
                                if finished || failed {
                                    if !failed && aliases.completed() {
                                        stream_trace.finish(
                                            RequestStatus::Completed,
                                            Some(200),
                                            "Provider stream completed.",
                                        );
                                    } else {
                                        stream_trace.finish(RequestStatus::Failed, Some(200), "The stream ended with an error or without a completion marker; request was not replayed.");
                                    }
                                    return;
                                }
                            }
                        });
                        let chunks = futures_util::stream::unfold(receiver, |mut receiver| async {
                            receiver.recv().await.map(|item| (item, receiver))
                        });
                        Response::builder()
                            .status(200)
                            .header("content-type", "text/event-stream")
                            .body(Body::from_stream(chunks))
                            .expect("Static response headers")
                    } else {
                        let _probe = probe;
                        let report = trace.as_ref().expect("Active report");
                        let body = tokio::select! { _ = cancelled.cancelled() => return cancelled_response(), result = read_json_observed(&mut response, 8 * 1024 * 1024, |bytes| report.response_bytes(bytes)) => result };
                        match body {
                                Ok(mut body) if body["choices"].is_array() && body.get("error").is_none() => {
                                    report.tokens(TokenUsage::from_response(&body));
                                    body["model"] = json!(requested); Json(body).into_response()
                                },
                                _ => return error(StatusCode::BAD_GATEWAY, "invalid_upstream", "The selected server returned an invalid completion; request was not replayed."),
                            }
                    };
                    let headers = output.headers_mut();
                    headers.insert("cache-control", HeaderValue::from_static("no-store"));
                    headers.insert(
                        "x-ai-usage-route-mode",
                        HeaderValue::from_static(pool.mode.as_str()),
                    );
                    headers.insert("x-ai-usage-selection", HeaderValue::from_static(selection));
                    for (name, value) in [
                        ("x-ai-usage-account", &account.id),
                        ("x-ai-usage-model", &requested),
                        ("x-ai-usage-upstream-model", &member.model),
                        ("x-ai-usage-requested-model", &requested),
                    ] {
                        if let Ok(value) = HeaderValue::from_str(value) {
                            headers.insert(name, value);
                        }
                    }
                    return output;
                }
                Err(RouteFailure::Unavailable {
                    reason,
                    retry_at,
                    account_wide,
                }) => {
                    let until = retry_at.max((self.clock)() + 1);
                    trace.as_ref().expect("Active report").attempt(
                        target,
                        "skipped",
                        reason,
                        Some(until),
                    );
                    let mut cooldowns = self.cooldowns.lock().await;
                    // Configuration changes cancel first, then clear this same map.
                    // Check under its lock so a late error cannot block a new account revision.
                    if cancelled.is_cancelled() {
                        return cancelled_response();
                    }
                    cooldowns.insert(
                        (
                            account.id.clone(),
                            if account_wide {
                                String::new()
                            } else {
                                member.model.clone()
                            },
                        ),
                        until,
                    );
                    next_retry = Some(next_retry.map_or(until, |old: i64| old.min(until)));
                    reasons
                        .push(json!({"account":account.id,"model":member.model,"reason":reason}));
                }
                Err(RouteFailure::Invalid(message)) => {
                    trace
                        .as_ref()
                        .expect("Active report")
                        .attempt(target, "failed", message, None);
                    return error(StatusCode::BAD_REQUEST, "unsupported_request", message);
                }
                Err(RouteFailure::Failed(message)) => {
                    trace
                        .as_ref()
                        .expect("Active report")
                        .attempt(target, "failed", message, None);
                    return error(StatusCode::BAD_GATEWAY, "upstream_failed", message);
                }
            }
        }
        let mut result = (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error":{"type":"allowance_unavailable","message":"No entry in this model pool is currently eligible. Check the account reasons below. Plan-only routing is enabled.","attempts":reasons,"retry_at":next_retry}}))).into_response();
        if let Some(until) = next_retry {
            if let Ok(value) = HeaderValue::from_str(&(until - (self.clock)()).max(1).to_string()) {
                result.headers_mut().insert("retry-after", value);
            }
        }
        result
    }
}

pub fn validate_request(request: &Value) -> Result<&str, &'static str> {
    let object = request
        .as_object()
        .ok_or("Send a JSON chat completion object.")?;
    let model = request["model"]
        .as_str()
        .filter(|m| super::config::valid_model(m))
        .ok_or("A valid model is required.")?;
    let messages = request["messages"]
        .as_array()
        .filter(|messages| !messages.is_empty())
        .ok_or("messages must be a non-empty array of chat messages.")?;
    // The HTTP body limit bounds input size. Message count is not a token/context limit;
    // long tool conversations must retain every message and tool-call/result pair.
    if messages.iter().any(|message| {
        !message.is_object()
            || !message["role"]
                .as_str()
                .is_some_and(|role| !role.is_empty())
    }) {
        return Err("Each chat message must be an object with a non-empty string role.");
    }
    if object.contains_key("stream") && !request["stream"].is_boolean() {
        return Err("stream must be true or false.");
    }
    // An allowlist prevents paid extensions, upstream fallbacks, callbacks, and routing overrides.
    const ALLOWED: &[&str] = &[
        "model",
        "messages",
        "stream",
        "stream_options",
        "temperature",
        "top_p",
        "max_tokens",
        "max_completion_tokens",
        "stop",
        "seed",
        "frequency_penalty",
        "presence_penalty",
        "response_format",
        "tools",
        "tool_choice",
        "parallel_tool_calls",
        "logprobs",
        "top_logprobs",
        "reasoning_effort",
        "n",
    ];
    if object.keys().any(|key| !ALLOWED.contains(&key.as_str())) {
        return Err("This request contains unsupported fields. Provider routing, paid plugins, and service tier overrides are not accepted.");
    }
    if request["tools"]
        .as_array()
        .is_some_and(|tools| tools.iter().any(|t| t["type"] != "function"))
    {
        return Err("Only caller-executed function tools are supported.");
    }
    Ok(model)
}

pub fn valid_session(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}

async fn send(
    client: &reqwest::Client,
    prepared: PreparedRequest,
    now: i64,
) -> Result<reqwest::Response, RouteFailure> {
    let mut request = client
        .post(prepared.url)
        .headers(prepared.headers)
        .json(&prepared.body);
    if let Some(key) = prepared.key {
        let mut header = HeaderValue::from_str(&format!("Bearer {}", key.as_str()))
            .map_err(|_| RouteFailure::Invalid("The saved key is invalid."))?;
        header.set_sensitive(true);
        request = request.header("authorization", header);
    }
    let response = request.send().await.map_err(|e| {
        if e.is_connect() {
            RouteFailure::Unavailable {
                reason: "Server could not be reached.",
                retry_at: now + 15,
                account_wide: true,
            }
        } else {
            RouteFailure::Failed(
                "Connection interrupted after submission. The request was not replayed.",
            )
        }
    })?;
    match response.status().as_u16() {
        200..=299 => Ok(response),
        429 => Err(RouteFailure::Unavailable { reason: "Provider rate limit reached.", retry_at: retry_time(response.headers(), now), account_wide: true }),
        401 | 403 => Err(RouteFailure::Unavailable { reason: "Account authorization failed. Check its connection.", retry_at: now + 60, account_wide: true }),
        404 => Err(RouteFailure::Unavailable { reason: "Model or endpoint unavailable.", retry_at: now + 60, account_wide: false }),
        503 => Err(RouteFailure::Unavailable { reason: "Server temporarily unavailable.", retry_at: retry_time(response.headers(), now), account_wide: true }),
        402 => Err(RouteFailure::Unavailable { reason: "Provider requires paid credits. Paid fallback is disabled.", retry_at: now + 60, account_wide: true }),
        _ => Err(RouteFailure::Failed("Provider rejected the request. It was not replayed; check model compatibility and server status.")),
    }
}

pub fn retry_time(headers: &reqwest::header::HeaderMap, now: i64) -> i64 {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.parse::<i64>()
                .ok()
                .map(|s| now.saturating_add(s.clamp(1, 2_678_400)))
                .or_else(|| {
                    chrono::DateTime::parse_from_rfc2822(v)
                        .ok()
                        .map(|d| d.timestamp())
                })
        })
        .filter(|time| *time > now)
        .unwrap_or(now + 60)
}

pub async fn read_json(response: &mut reqwest::Response, limit: usize) -> Result<Value, String> {
    read_json_observed(response, limit, |_| {}).await
}

async fn read_json_observed(
    response: &mut reqwest::Response,
    limit: usize,
    mut observed: impl FnMut(usize),
) -> Result<Value, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Could not read the server response.")?
    {
        observed(chunk.len());
        if bytes.len() + chunk.len() > limit {
            return Err("Server response is too large.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Server returned invalid JSON.".into())
}

pub fn error(status: StatusCode, kind: &str, message: &str) -> Response {
    (
        status,
        Json(json!({"error":{"type":kind,"message":message}})),
    )
        .into_response()
}
fn cancelled_response() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "configuration_changed",
        "Routing settings changed. This request was cancelled.",
    )
}
