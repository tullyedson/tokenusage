use super::*;
use crate::{
    credentials::ISecretStore,
    routing::{
        config::{AccountRouting, ModelMapping, RouterSettings},
        engine::{RouteAccount, RouterEngine},
    },
};
use axum::{
    body::to_bytes,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, Mutex,
    },
};

const NOW: i64 = 1_800_000_000;
const MODEL: &str = "glm-5.3-flash";

#[derive(Default)]
struct Secrets(Mutex<BTreeMap<(String, String), String>>);
impl ISecretStore for Secrets {
    fn get(&self, id: &str, field: &str) -> Result<Option<Zeroizing<String>>, String> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&(id.into(), field.into()))
            .cloned()
            .map(Zeroizing::new))
    }
    fn set(&self, id: &str, field: &str, value: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .insert((id.into(), field.into()), value.into());
        Ok(())
    }
    fn delete(&self, id: &str, field: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(&(id.into(), field.into()));
        Ok(())
    }
}

fn usage() -> Value {
    json!({"usage":{
        "rolling":{"percent":10,"status":"ok","resetsAt":NOW+300},
        "weekly":{"percent":50,"status":"ok","resetsAt":NOW+600},
        "monthly":{"percent":30,"status":"ok","resetsAt":NOW+900}
    }})
}

#[test]
fn all_windows_gate_go_and_the_last_exhausted_window_controls_retry() {
    assert!(check_go_allowance(&usage(), NOW).is_ok());
    for window in ["rolling", "weekly", "monthly"] {
        let mut value = usage();
        value["usage"][window]["status"] = json!("rate-limited");
        assert!(matches!(
            check_go_allowance(&value, NOW),
            Err(RouteFailure::Unavailable {
                account_wide: true,
                ..
            })
        ));
        value["usage"][window]["status"] = json!("ok");
        value["usage"][window]["percent"] = json!(100.01);
        assert!(check_go_allowance(&value, NOW).is_err());
    }
    let mut value = usage();
    value["usage"]["rolling"]["percent"] = json!(100);
    value["usage"]["weekly"]["percent"] = json!(100);
    assert!(
        matches!(check_go_allowance(&value, NOW), Err(RouteFailure::Unavailable { retry_at, .. }) if retry_at == NOW+600)
    );
    for (key, bad) in [
        ("percent", json!(-1)),
        ("percent", Value::Null),
        ("resetsAt", json!("bad")),
        ("resetsAt", json!(NOW - 1)),
        ("status", json!("unknown")),
    ] {
        let mut value = usage();
        value["usage"]["monthly"][key] = bad;
        assert!(check_go_allowance(&value, NOW).is_err(), "{key}");
    }
    assert!(check_go_allowance(&json!({"usage":{}}), NOW).is_err());
}

#[test]
fn subscription_modes_must_be_explicit_and_cannot_enable_credit_plan_routing() {
    for kind in [SubscriptionKind::OpenCodeGo, SubscriptionKind::OllamaCloud] {
        let adapter = SubscriptionProvider::new(kind);
        let mut config = ProviderConfig::default();
        assert!(adapter.validate(&config).is_ok()); // Existing monitoring settings migrate unchanged.
        config.routing.enabled = true;
        for mode in ["", "unconfirmed", "paid", "monthly_credits"] {
            config.fields.insert("routing_billing".into(), mode.into());
            assert!(adapter.validate(&config).is_err());
        }
        config
            .fields
            .insert("routing_billing".into(), kind.billing_mode().into());
        assert!(adapter.validate(&config).is_ok());
    }
}

#[derive(Clone, Default)]
struct Fixture {
    usage: Arc<Mutex<Value>>,
    calls: Arc<Mutex<Vec<(String, Value, HeaderMap)>>>,
    limited: Arc<Mutex<Vec<String>>>,
}
async fn catalog() -> Json<Value> {
    Json(json!({"object":"list","data":[{"id":MODEL}]}))
}
async fn quota(State(f): State<Fixture>) -> Json<Value> {
    Json(f.usage.lock().unwrap().clone())
}
async fn chat(
    State(f): State<Fixture>,
    Path(account): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    f.calls
        .lock()
        .unwrap()
        .push((account.clone(), body.clone(), headers));
    if f.limited.lock().unwrap().contains(&account) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "60")],
            "private quota error",
        )
            .into_response();
    }
    if body["stream"] == true {
        return ([("content-type", "text/event-stream")], "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"fixture\",\"type\":\"function\",\"function\":{\"name\":\"example\",\"arguments\":\"{}\"}}]}}]}\n\ndata: [DONE]\n\n").into_response();
    }
    Json(json!({"model":MODEL,"choices":[{"message":{"role":"assistant","content":"fixture"}}]}))
        .into_response()
}
struct Server {
    base: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn server(f: Fixture) -> Server {
    let tcp = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let base = format!("http://127.0.0.1:{}/", tcp.local_addr().unwrap().port());
    let app = Router::new()
        .route("/{account}/models", get(catalog))
        .route("/{account}/usage", get(quota))
        .route("/{account}/chat/completions", post(chat))
        .with_state(f);
    Server {
        base,
        task: tokio::spawn(async move {
            axum::serve(tcp, app).await.unwrap();
        }),
    }
}
fn account(server: &Server, id: &str, kind: SubscriptionKind) -> RouteAccount {
    RouteAccount {
        id: id.into(),
        config: ProviderConfig {
            enabled: true,
            routing: AccountRouting {
                enabled: true,
                models: vec![ModelMapping {
                    model: MODEL.into(),
                    upstream: MODEL.into(),
                }],
            },
            fields: [("routing_billing".into(), kind.billing_mode().into())].into(),
            ..Default::default()
        },
        provider: Arc::new(SubscriptionProvider {
            kind,
            base: format!("{}{id}/", server.base).parse().unwrap(),
        }),
        serial: Arc::new(tokio::sync::Mutex::new(())),
    }
}
async fn engine(server: &Server, clock: Arc<AtomicI64>) -> Arc<RouterEngine> {
    let store = Arc::new(Secrets::default());
    for id in ["go", "cloud"] {
        store
            .set(id, "api_key", &format!("fictional_{id}_key"))
            .unwrap();
    }
    let engine = Arc::new(
        RouterEngine::new(store).with_clock(Arc::new(move || clock.load(Ordering::SeqCst))),
    );
    engine
        .configure(
            RouterSettings {
                enabled: true,
                account_order: vec!["go".into(), "cloud".into()],
                ..Default::default()
            },
            vec![
                account(server, "cloud", SubscriptionKind::OllamaCloud),
                account(server, "go", SubscriptionKind::OpenCodeGo),
            ],
        )
        .await;
    engine
}
fn prompt() -> Value {
    json!({"model":MODEL,"messages":[{"role":"user","content":"fictional fixture"}],"tools":[{"type":"function","function":{"name":"example","parameters":{"type":"object"}}}]})
}

#[tokio::test]
async fn go_preflight_hands_the_same_model_to_cloud_and_returns_after_reset() {
    let f = Fixture {
        usage: Arc::new(Mutex::new(usage())),
        ..Default::default()
    };
    let server = server(f.clone()).await;
    let clock = Arc::new(AtomicI64::new(NOW));
    let engine = engine(&server, clock.clone()).await;
    let response = engine
        .route_with_session(prompt(), Some("session_fixture"))
        .await;
    assert_eq!(response.headers()["x-ai-usage-account"], "go");
    f.usage.lock().unwrap()["usage"]["rolling"]["percent"] = json!(100);
    let response = engine.route(prompt()).await;
    assert_eq!(response.headers()["x-ai-usage-account"], "cloud");
    let mut streaming = prompt();
    streaming["stream"] = json!(true);
    let response = engine.route(streaming).await;
    assert_eq!(response.headers()["x-ai-usage-account"], "cloud");
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    assert!(std::str::from_utf8(&bytes).unwrap().contains("tool_calls"));
    assert!(std::str::from_utf8(&bytes)
        .unwrap()
        .ends_with("data: [DONE]\n\n"));
    clock.store(NOW + 300, Ordering::SeqCst);
    f.usage.lock().unwrap()["usage"]["rolling"] =
        json!({"percent":0,"status":"ok","resetsAt":NOW+600});
    let response = engine.route(prompt()).await;
    assert_eq!(response.headers()["x-ai-usage-account"], "go");
    let calls = f.calls.lock().unwrap();
    assert_eq!(
        calls.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(),
        ["go", "cloud", "cloud", "go"]
    );
    for (id, body, headers) in calls.iter() {
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["tools"], prompt()["tools"]);
        assert_eq!(
            headers["authorization"],
            format!("Bearer fictional_{id}_key")
        );
        assert!(headers["user-agent"]
            .to_str()
            .unwrap()
            .starts_with("AI-Usage/"));
        assert_eq!(headers.contains_key("x-opencode-session"), id == "go");
    }
    assert_eq!(calls[0].2["x-opencode-session"], "session_fixture");
}

#[tokio::test]
async fn race_at_provider_limit_fails_over_and_both_exhausted_stop_without_paid_endpoint() {
    let f = Fixture {
        usage: Arc::new(Mutex::new(usage())),
        ..Default::default()
    };
    let server = server(f.clone()).await;
    let engine = engine(&server, Arc::new(AtomicI64::new(NOW))).await;
    f.limited.lock().unwrap().push("go".into());
    let response = engine.route(prompt()).await;
    assert_eq!(response.headers()["x-ai-usage-account"], "cloud");
    f.limited.lock().unwrap().push("cloud".into());
    let response = engine.route(prompt()).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()["retry-after"], "60");
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    assert!(!std::str::from_utf8(&bytes)
        .unwrap()
        .contains("private quota error"));
    assert_eq!(f.calls.lock().unwrap().len(), 3);
    assert_eq!(
        engine.route(prompt()).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(f.calls.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn unknown_allowance_unknown_models_and_unconfirmed_billing_never_generate() {
    let f = Fixture::default();
    let server = server(f.clone()).await;
    let engine = engine(&server, Arc::new(AtomicI64::new(NOW))).await;
    let config = account(&server, "go", SubscriptionKind::OpenCodeGo);
    engine
        .configure(
            RouterSettings {
                enabled: true,
                ..Default::default()
            },
            vec![config],
        )
        .await;
    assert_eq!(
        engine.route(prompt()).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    *f.usage.lock().unwrap() = usage();
    let mut config = account(&server, "go", SubscriptionKind::OpenCodeGo);
    config.config.fields.clear();
    engine
        .configure(
            RouterSettings {
                enabled: true,
                ..Default::default()
            },
            vec![config],
        )
        .await;
    assert_eq!(
        engine.route(prompt()).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    let mut config = account(&server, "go", SubscriptionKind::OpenCodeGo);
    config.config.routing.models[0].upstream = "missing".into();
    engine
        .configure(
            RouterSettings {
                enabled: true,
                ..Default::default()
            },
            vec![config],
        )
        .await;
    assert_eq!(
        engine.route(prompt()).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        engine
            .route_with_session(prompt(), Some("contains spaces"))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert!(f.calls.lock().unwrap().is_empty());
}
