use super::{
    config::RouterSettings,
    engine::{error, RouteAccount, RouterEngine},
};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouterStatus {
    pub running: bool,
    pub base_url: String,
    pub token_configured: bool,
    pub error: Option<String>,
}
struct Listener {
    port: u16,
    task: tokio::task::JoinHandle<()>,
}
pub struct RouterRuntime {
    pub engine: Arc<RouterEngine>,
    listener: Mutex<Option<Listener>>,
    status: Mutex<RouterStatus>,
}

impl RouterRuntime {
    pub fn new(secrets: Arc<dyn crate::credentials::ISecretStore>) -> Self {
        Self {
            engine: Arc::new(RouterEngine::new(secrets)),
            listener: Mutex::new(None),
            status: Mutex::new(RouterStatus {
                running: false,
                base_url: "http://127.0.0.1:43129/v1".into(),
                token_configured: false,
                error: None,
            }),
        }
    }
    pub async fn status(&self) -> RouterStatus {
        let listener = self.listener.lock().await;
        let mut status = self.status.lock().await.clone();
        if listener.as_ref().is_some_and(|l| l.task.is_finished()) {
            status.running = false;
            status.error = Some(
                "The local router stopped unexpectedly. Save routing settings to restart it."
                    .into(),
            );
        }
        status
    }
    pub async fn apply(&self, settings: RouterSettings, accounts: Vec<RouteAccount>) {
        let mut listener = self.listener.lock().await;
        self.engine.configure(settings.clone(), accounts).await;
        let token = self.engine.secrets.get("router", "client_token");
        let token_configured = matches!(&token, Ok(Some(value)) if valid_token(value));
        let mut status = RouterStatus {
            running: false,
            base_url: format!("http://127.0.0.1:{}/v1", settings.port),
            token_configured,
            error: token.err(),
        };
        let wanted = settings.enabled && token_configured;
        if listener
            .as_ref()
            .is_some_and(|l| !wanted || l.port != settings.port || l.task.is_finished())
        {
            if let Some(old) = listener.take() {
                old.task.abort();
                let _ = old.task.await;
            }
        }
        if wanted && listener.is_none() {
            match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, settings.port))
                .await
            {
                Ok(tcp) => {
                    let app = application(self.engine.clone(), settings.port);
                    *listener = Some(Listener {
                        port: settings.port,
                        task: tokio::spawn(async move {
                            let _ = axum::serve(tcp, app).await;
                        }),
                    });
                }
                Err(_) => {
                    status.error = Some(
                        "Could not bind the local router port. Choose an unused port in Settings."
                            .into(),
                    )
                }
            }
        }
        if settings.enabled && !token_configured {
            status.error = Some("Generate and save a client key before enabling routing.".into());
        }
        status.running = listener.is_some();
        *self.status.lock().await = status;
    }
}

#[derive(Clone)]
struct HttpState {
    engine: Arc<RouterEngine>,
    port: u16,
}
pub fn application(engine: Arc<RouterEngine>, port: u16) -> Router {
    Router::new()
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .with_state(HttpState { engine, port })
}

pub fn valid_token(token: &str) -> bool {
    (32..=256).contains(&token.len())
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}

fn authorized(state: &HttpState, headers: &HeaderMap) -> bool {
    // Do not accept browser cross-origin requests or arbitrary DNS names resolving to loopback.
    if headers.contains_key("origin") {
        return false;
    }
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if host != format!("127.0.0.1:{}", state.port) && host != format!("localhost:{}", state.port) {
        return false;
    }
    let Some(candidate) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return false;
    };
    let Ok(Some(token)) = state.engine.secrets.get("router", "client_token") else {
        return false;
    };
    if !valid_token(&token) || candidate.len() != token.len() {
        return false;
    }
    candidate
        .bytes()
        .zip(token.bytes())
        .fold(0u8, |different, (a, b)| different | (a ^ b))
        == 0
}
async fn models(State(state): State<HttpState>, headers: HeaderMap) -> Response {
    if !authorized(&state, &headers) {
        return error(StatusCode::UNAUTHORIZED, "unauthorized", "A valid local client key and loopback Host are required. Browser origins are not accepted.");
    }
    match state.engine.model_list().await {
        Ok(models) => Json(models).into_response(),
        Err(_) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "configuration_changed",
            "Settings changed while discovering models. Retry the model list.",
        ),
    }
}
async fn chat(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if !authorized(&state, &headers) {
        return error(StatusCode::UNAUTHORIZED, "unauthorized", "A valid local client key and loopback Host are required. Browser origins are not accepted.");
    }
    let session = headers
        .get("x-ai-usage-session")
        .or_else(|| headers.get("x-opencode-session"));
    let instance = headers.get("x-ai-usage-instance");
    let session = match session.map(|value| value.to_str()).transpose() {
        Ok(value) => value,
        Err(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_session",
                "Invalid session header.",
            )
        }
    };
    let instance = match instance.map(|value| value.to_str()).transpose() {
        Ok(value) => value,
        Err(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_instance",
                "Invalid instance header.",
            )
        }
    };
    state
        .engine
        .route_with_identity(body, session, instance)
        .await
}

#[cfg(test)]
mod tests;
