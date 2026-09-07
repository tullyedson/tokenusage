use super::*;
use crate::{
    credentials::ISecretStore,
    model::ProviderConfig,
    routing::{
        config::{AccountRouting, ModelPool, PoolMember},
        engine::*,
        providers::HttpProvider,
    },
};
use async_trait::async_trait;
use axum::{body::to_bytes, extract::Path};
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Mutex as StdMutex,
    },
};
use zeroize::Zeroizing;

const CLIENT_KEY: &str = "fictional_local_client_key_for_tests_only_000000";
#[derive(Default)]
struct MemoryStore(StdMutex<BTreeMap<(String, String), String>>);
impl ISecretStore for MemoryStore {
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
struct TestServer {
    base: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn serve(app: Router) -> TestServer {
    let tcp = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let port = tcp.local_addr().unwrap().port();
    TestServer {
        base: format!("http://127.0.0.1:{port}"),
        task: tokio::spawn(async move {
            axum::serve(tcp, app).await.unwrap();
        }),
    }
}

#[derive(Clone, Default)]
struct Upstream {
    calls: Arc<StdMutex<Vec<(String, Value, String)>>>,
    limit_first: Arc<AtomicBool>,
    server_error: Arc<AtomicBool>,
}
async fn completion(
    State(state): State<Upstream>,
    Path(account): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    state.calls.lock().unwrap().push((
        account.clone(),
        body.clone(),
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .into(),
    ));
    if state.server_error.load(Ordering::SeqCst) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if account == "first" && state.limit_first.swap(false, Ordering::SeqCst) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "60")],
            "fictional private upstream error",
        )
            .into_response();
    }
    if body["stream"] == true {
        return (
            [("content-type", "text/event-stream")],
            "data: {\"choices\":[{\"delta\":{\"content\":\"sample\"}}]}\n\ndata: [DONE]\n\n",
        )
            .into_response();
    }
    Json(json!({"id":"fixture","object":"chat.completion","model":body["model"],"choices":[{"message":{"role":"assistant","content":"sample"},"finish_reason":"stop","index":0}]})).into_response()
}
struct Adapter {
    base: String,
    blocked_until: Arc<AtomicI64>,
    clock: Arc<AtomicI64>,
}
#[async_trait]
impl IInferenceProvider for Adapter {
    fn definition(&self) -> InferenceDefinition {
        InferenceDefinition {
            description: "Fictional test provider",
        }
    }
    fn validate(&self, _: &ProviderConfig) -> Result<(), String> {
        Ok(())
    }
    async fn models(&self, _: &InferenceContext<'_>) -> Result<Vec<String>, String> {
        Ok(vec!["server-x".into()])
    }
    async fn prepare(
        &self,
        ctx: &InferenceContext<'_>,
        request: &Value,
        upstream: &str,
    ) -> Result<PreparedRequest, RouteFailure> {
        let reset = self.blocked_until.load(Ordering::SeqCst);
        if ctx.account_id == "first" && reset > self.clock.load(Ordering::SeqCst) {
            return Err(RouteFailure::Unavailable {
                reason: "Included allowance exhausted.",
                retry_at: reset,
                account_wide: true,
            });
        }
        let mut body = request.clone();
        body["model"] = json!(upstream);
        Ok(PreparedRequest {
            headers: Default::default(),
            url: format!("{}/{}/chat", self.base, ctx.account_id)
                .parse()
                .unwrap(),
            body,
            key: ctx.secrets.get(ctx.account_id, "api_key").unwrap(),
        })
    }
}
fn account(id: &str, _model: &str, provider: Arc<dyn IInferenceProvider>) -> RouteAccount {
    RouteAccount {
        id: id.into(),
        config: ProviderConfig {
            enabled: true,
            routing: AccountRouting { enabled: true },
            ..Default::default()
        },
        provider,
        serial: Arc::new(Mutex::new(())),
    }
}
fn pool(name: &str, entries: &[(&str, &str)]) -> ModelPool {
    ModelPool {
        name: name.into(),
        members: entries
            .iter()
            .map(|(account, model)| PoolMember {
                account_id: (*account).into(),
                model: (*model).into(),
            })
            .collect(),
    }
}
fn request(model: &str) -> Value {
    json!({"model":model,"messages":[{"role":"user","content":"fictional test prompt"}],"stream":false})
}
async fn body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}

#[tokio::test]
async fn ordered_accounts_fail_over_on_429_and_return_to_preferred_after_reset() {
    let upstream = Upstream::default();
    upstream.limit_first.store(true, Ordering::SeqCst);
    let server = serve(
        Router::new()
            .route("/{account}/chat", post(completion))
            .with_state(upstream.clone()),
    )
    .await;
    let clock = Arc::new(AtomicI64::new(1000));
    let clock_fn = clock.clone();
    let store = Arc::new(MemoryStore::default());
    store
        .set("first", "api_key", "fictional-first-key")
        .unwrap();
    store
        .set("second", "api_key", "fictional-second-key")
        .unwrap();
    let engine = Arc::new(
        RouterEngine::new(store).with_clock(Arc::new(move || clock_fn.load(Ordering::SeqCst))),
    );
    let adapter = Arc::new(Adapter {
        base: server.base.clone(),
        blocked_until: Arc::new(AtomicI64::new(0)),
        clock: clock.clone(),
    });
    engine
        .configure(
            RouterSettings {
                enabled: true,
                pools: vec![pool(
                    "server-x",
                    &[("first", "server-x"), ("second", "server-x")],
                )],
                ..Default::default()
            },
            vec![
                account("second", "x", adapter.clone()),
                account("first", "x", adapter),
            ],
        )
        .await;
    let response = engine.route(request("server-x")).await;
    assert_eq!(response.headers()["x-ai-usage-account"], "second");
    assert_eq!(body(response).await["model"], "server-x");
    let response = engine.route(request("server-x")).await;
    assert_eq!(response.headers()["x-ai-usage-account"], "second");
    drop(response);
    clock.store(1061, Ordering::SeqCst);
    let response = engine.route(request("server-x")).await;
    assert_eq!(response.headers()["x-ai-usage-account"], "first");
    drop(response);
    let calls = upstream.calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .map(|(id, _, _)| id.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second", "second", "first"]
    );
    assert!(calls
        .iter()
        .all(|(id, _, key)| key == &format!("Bearer fictional-{id}-key")));
}

#[tokio::test]
async fn pool_order_can_choose_a_different_model_before_an_exact_model_and_reset_to_first() {
    let upstream = Upstream::default();
    let server = serve(
        Router::new()
            .route("/{account}/chat", post(completion))
            .with_state(upstream.clone()),
    )
    .await;
    let clock = Arc::new(AtomicI64::new(1000));
    let clock_fn = clock.clone();
    let gate = Arc::new(AtomicI64::new(1100));
    let engine = Arc::new(
        RouterEngine::new(Arc::new(MemoryStore::default()))
            .with_clock(Arc::new(move || clock_fn.load(Ordering::SeqCst))),
    );
    let adapter = Arc::new(Adapter {
        base: server.base.clone(),
        blocked_until: gate,
        clock: clock.clone(),
    });
    let settings = RouterSettings {
        enabled: true,
        pools: vec![pool(
            "server-x",
            &[
                ("first", "server-x"),
                ("backup", "server-y"),
                ("exact", "server-x"),
            ],
        )],
        ..Default::default()
    };
    engine
        .configure(
            settings.clone(),
            vec![
                account("first", "x", adapter.clone()),
                account("backup", "y", adapter.clone()),
                account("exact", "x", adapter.clone()),
            ],
        )
        .await;
    let response = engine.route(request("server-x")).await;
    assert_eq!(response.headers()["x-ai-usage-account"], "backup");
    assert_eq!(response.headers()["x-ai-usage-upstream-model"], "server-y");
    drop(response);
    engine
        .configure(
            settings,
            vec![
                account("first", "x", adapter.clone()),
                account("backup", "y", adapter),
            ],
        )
        .await;
    let response = engine.route(request("server-x")).await;
    assert_eq!(response.headers()["x-ai-usage-model"], "server-x");
    assert_eq!(body(response).await["model"], "server-x");
    let response = engine.route(request("unmapped")).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body(response).await["error"]["type"], "model_not_found");
    clock.store(1101, Ordering::SeqCst);
    let response = engine.route(request("server-x")).await;
    assert_eq!(response.headers()["x-ai-usage-account"], "first");
    assert_eq!(upstream.calls.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn streaming_preserves_events_and_terminal_errors_are_never_replayed() {
    let upstream = Upstream::default();
    let server = serve(
        Router::new()
            .route("/{account}/chat", post(completion))
            .with_state(upstream.clone()),
    )
    .await;
    let engine = Arc::new(RouterEngine::new(Arc::new(MemoryStore::default())));
    let adapter = Arc::new(Adapter {
        base: server.base.clone(),
        blocked_until: Arc::new(AtomicI64::new(0)),
        clock: Arc::new(AtomicI64::new(1000)),
    });
    engine
        .configure(
            RouterSettings {
                enabled: true,
                ..Default::default()
            },
            vec![
                account("first", "x", adapter.clone()),
                account("second", "x", adapter),
            ],
        )
        .await;
    let mut req = request("server-x");
    req["stream"] = json!(true);
    let response = engine.route(req).await;
    let bytes = to_bytes(response.into_body(), 10000).await.unwrap();
    assert!(std::str::from_utf8(&bytes)
        .unwrap()
        .ends_with("data: [DONE]\n\n"));
    upstream.server_error.store(true, Ordering::SeqCst);
    let response = engine.route(request("server-x")).await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(upstream.calls.lock().unwrap().len(), 2);
    assert!(!body(response)
        .await
        .to_string()
        .contains("fictional private"));
}

#[tokio::test]
async fn local_api_requires_key_and_loopback_host_rejects_browser_origins_and_paid_overrides() {
    let store = Arc::new(MemoryStore::default());
    store.set("router", "client_token", CLIENT_KEY).unwrap();
    let runtime = RouterRuntime::new(store.clone());
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = socket.local_addr().unwrap().port();
    drop(socket);
    runtime
        .apply(
            RouterSettings {
                enabled: true,
                port,
                ..Default::default()
            },
            vec![],
        )
        .await;
    assert!(runtime.status().await.running);
    let url = format!("http://127.0.0.1:{port}/v1/models");
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    assert_eq!(
        client.get(&url).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(&url)
            .bearer_auth(CLIENT_KEY)
            .header("origin", "https://example.com")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(&url)
            .bearer_auth(CLIENT_KEY)
            .header("host", format!("example.com:{port}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(&url)
            .bearer_auth(CLIENT_KEY)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let mut req = request("server-x");
    req["plugins"] = json!([{"id":"web"}]);
    let response = client
        .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .bearer_auth(CLIENT_KEY)
        .json(&req)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    store
        .set(
            "router",
            "client_token",
            "fictional_rotated_client_key_for_tests_only",
        )
        .unwrap();
    assert_eq!(
        client
            .get(&url)
            .bearer_auth(CLIENT_KEY)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    runtime
        .apply(
            RouterSettings {
                enabled: false,
                port,
                ..Default::default()
            },
            vec![],
        )
        .await;
    assert!(!runtime.status().await.running);
    assert!(
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn real_local_vllm_adapter_discovers_and_sends_mapped_model() {
    let upstream = Upstream::default();
    let fixture = upstream.clone();
    let server = serve(
        Router::new()
            .route(
                "/v1/models",
                get(|| async { Json(json!({"data":[{"id":"server-x"}]})) }),
            )
            .route(
                "/v1/chat/completions",
                post(move |headers: HeaderMap, Json(body): Json<Value>| {
                    let fixture = fixture.clone();
                    async move {
                        completion(State(fixture), Path("local".into()), headers, Json(body)).await
                    }
                }),
            ),
    )
    .await;
    let engine = Arc::new(RouterEngine::new(Arc::new(MemoryStore::default())));
    let mut local = account("local", "x", Arc::new(HttpProvider::VllmLocal));
    local
        .config
        .fields
        .insert("base_url".into(), server.base.clone());
    engine
        .configure(
            RouterSettings {
                enabled: true,
                ..Default::default()
            },
            vec![local],
        )
        .await;
    let response = engine.route(request("server-x")).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await["model"], "server-x");
    assert_eq!(upstream.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn changing_settings_cancels_a_waiting_request() {
    let engine = Arc::new(RouterEngine::new(Arc::new(MemoryStore::default())));
    let adapter = Arc::new(Adapter {
        base: "http://127.0.0.1:1".into(),
        blocked_until: Arc::new(AtomicI64::new(0)),
        clock: Arc::new(AtomicI64::new(1000)),
    });
    let first = account("first", "x", adapter);
    let lock = first.serial.clone();
    let held = lock.lock().await;
    engine
        .configure(
            RouterSettings {
                enabled: true,
                ..Default::default()
            },
            vec![first],
        )
        .await;
    let cloned = engine.clone();
    let pending = tokio::spawn(async move { cloned.route(request("server-x")).await });
    tokio::task::yield_now().await;
    engine.configure(RouterSettings::default(), vec![]).await;
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), pending)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.status(), StatusCode::SERVICE_UNAVAILABLE);
    drop(held);
}

#[tokio::test]
async fn paused_stream_is_cancelled_without_waiting_for_the_client_to_read() {
    let server = serve(Router::new().route(
        "/{account}/chat",
        post(|| async {
            let chunks = futures_util::stream::repeat_with(|| {
                Ok::<_, std::io::Error>(axum::body::Bytes::from_static(
                    b"data: {\"sample\":true}\n\n",
                ))
            });
            Response::builder()
                .header("content-type", "text/event-stream")
                .body(axum::body::Body::from_stream(chunks))
                .unwrap()
        }),
    ))
    .await;
    let engine = Arc::new(RouterEngine::new(Arc::new(MemoryStore::default())));
    let adapter = Arc::new(Adapter {
        base: server.base.clone(),
        blocked_until: Arc::new(AtomicI64::new(0)),
        clock: Arc::new(AtomicI64::new(1000)),
    });
    let first = account("first", "x", adapter);
    let serial = first.serial.clone();
    engine
        .configure(
            RouterSettings {
                enabled: true,
                ..Default::default()
            },
            vec![first],
        )
        .await;
    let mut req = request("server-x");
    req["stream"] = json!(true);
    let response = engine.route(req).await;
    assert_eq!(response.status(), StatusCode::OK);
    // Keep the response unconsumed while disabling routing.
    engine.configure(RouterSettings::default(), vec![]).await;
    let lock = tokio::time::timeout(std::time::Duration::from_secs(2), serial.lock())
        .await
        .expect("Upstream permit was not released");
    drop(lock);
    drop(response);
}

#[tokio::test]
async fn local_ollama_metadata_blocks_cloud_aliases_before_generation() {
    let remote = Arc::new(AtomicBool::new(false));
    let remote_copy = remote.clone();
    let upstream = Upstream::default();
    let fixture = upstream.clone();
    let server = serve(Router::new()
        .route("/api/tags", get(|| async { Json(json!({"models":[{"name":"server-x","details":{"parameter_size":"1B","quantization_level":"Q4"}}]})) }))
        .route("/api/show", post(move || { let remote = remote_copy.clone(); async move {
            if remote.load(Ordering::SeqCst) { Json(json!({"remote_host":"https://ollama.com","remote_model":"cloud-model","details":{"parameter_size":"1B","quantization_level":"Q4"}})) }
            else { Json(json!({"details":{"parameter_size":"1B","quantization_level":"Q4"}})) }
        }}))
        .route("/v1/chat/completions", post(move |headers: HeaderMap, Json(body): Json<Value>| { let fixture = fixture.clone(); async move { completion(State(fixture), Path("local".into()), headers, Json(body)).await } }))).await;
    let engine = Arc::new(RouterEngine::new(Arc::new(MemoryStore::default())));
    let mut local = account("local", "x", Arc::new(HttpProvider::OllamaLocal));
    local
        .config
        .fields
        .insert("base_url".into(), server.base.clone());
    engine
        .configure(
            RouterSettings {
                enabled: true,
                ..Default::default()
            },
            vec![local],
        )
        .await;
    let response = engine.route(request("server-x")).await;
    assert_eq!(response.status(), StatusCode::OK);
    drop(response);
    remote.store(true, Ordering::SeqCst);
    let response = engine.route(request("server-x")).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(upstream.calls.lock().unwrap().len(), 1);
}
