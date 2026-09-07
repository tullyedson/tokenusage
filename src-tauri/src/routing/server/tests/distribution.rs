use super::*;
use crate::routing::config::RouteMode;

async fn fixture(upstream: Upstream) -> (Arc<RouterEngine>, TestServer) {
    let server = serve(
        Router::new()
            .route("/{account}/chat", post(completion))
            .with_state(upstream),
    )
    .await;
    let adapter = Arc::new(Adapter {
        base: server.base.clone(),
        blocked_until: Arc::new(AtomicI64::new(0)),
        clock: Arc::new(AtomicI64::new(1000)),
    });
    let store = Arc::new(MemoryStore::default());
    store.set("router", "client_token", CLIENT_KEY).unwrap();
    let engine = Arc::new(RouterEngine::new(store));
    let mut balanced = pool(
        "balanced",
        &[("first", "small"), ("second", "small"), ("third", "small")],
    );
    balanced.mode = RouteMode::LoadDistribution;
    engine
        .configure(
            RouterSettings {
                enabled: true,
                pools: vec![balanced],
                ..Default::default()
            },
            ["first", "second", "third"]
                .into_iter()
                .map(|id| account(id, "small", adapter.clone()))
                .collect(),
        )
        .await;
    (engine, server)
}
#[tokio::test]
async fn http_instances_override_sessions_and_report_distribution_without_false_fallbacks() {
    let upstream = Upstream::default();
    let (engine, _upstream_server) = fixture(upstream.clone()).await;
    let tcp = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let port = tcp.local_addr().unwrap().port();
    let app = application(engine.clone(), port);
    let server = TestServer {
        base: format!("http://127.0.0.1:{port}"),
        task: tokio::spawn(async move { axum::serve(tcp, app).await.unwrap() }),
    };
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    for (instance, session, account) in [
        ("caller_a", "conversation_a", "first"),
        ("caller_b", "conversation_a", "second"),
        ("caller_c", "conversation_a", "third"),
        ("caller_a", "conversation_b", "first"),
    ] {
        let response = client
            .post(format!("{}/v1/chat/completions", server.base))
            .bearer_auth(CLIENT_KEY)
            .header("x-ai-usage-instance", instance)
            .header("x-ai-usage-session", session)
            .json(&request("balanced"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["x-ai-usage-account"], account);
        assert_eq!(
            response.headers()["x-ai-usage-route-mode"],
            "loadDistribution"
        );
        assert_eq!(
            response.headers()["x-ai-usage-selection"],
            if session == "conversation_b" {
                "sticky"
            } else {
                "distributed"
            }
        );
        assert_eq!(response.json::<Value>().await.unwrap()["model"], "balanced");
    }
    let report = engine.routing_report();
    assert_eq!(report.recent.len(), 4);
    assert!(report
        .recent
        .iter()
        .all(|row| row.mode == RouteMode::LoadDistribution && row.fallback_count == 0));
    let serialized = serde_json::to_string(&report).unwrap();
    for private in [
        "caller_a",
        "conversation_a",
        CLIENT_KEY,
        "fictional test prompt",
    ] {
        assert!(!serialized.contains(private));
    }
    let invalid = client
        .post(format!("{}/v1/chat/completions", server.base))
        .bearer_auth(CLIENT_KEY)
        .header("x-ai-usage-instance", "not an opaque id")
        .json(&request("balanced"))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), 400);
    assert_eq!(upstream.calls.lock().unwrap().len(), 4);
    let models = engine.model_list().await.unwrap();
    assert_eq!(
        models["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == "balanced")
            .unwrap()["routing_mode"],
        "loadDistribution"
    );
}
#[tokio::test]
async fn quota_failure_reassigns_the_caller_and_subsequent_requests_stay_there() {
    let upstream = Upstream::default();
    upstream.limit_first.store(true, Ordering::SeqCst);
    let (engine, _server) = fixture(upstream.clone()).await;
    let response = engine
        .route_with_identity(request("balanced"), None, Some("caller"))
        .await;
    assert_eq!(response.headers()["x-ai-usage-account"], "second");
    drop(response);
    assert_eq!(engine.routing_report().recent[0].fallback_count, 1);
    let response = engine
        .route_with_identity(request("balanced"), None, Some("caller"))
        .await;
    assert_eq!(response.headers()["x-ai-usage-account"], "second");
    assert_eq!(response.headers()["x-ai-usage-selection"], "sticky");
    drop(response);
    assert_eq!(engine.routing_report().recent[0].fallback_count, 0);
    assert_eq!(
        upstream
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|(id, _, _)| id.as_str())
            .collect::<Vec<_>>(),
        ["first", "second", "second"]
    );
}
#[tokio::test]
async fn sessions_are_sticky_and_unidentified_requests_are_independently_distributed() {
    let (engine, _server) = fixture(Upstream::default()).await;
    for expected in ["first", "second", "third", "first"] {
        let response = engine.route(request("balanced")).await;
        assert_eq!(response.headers()["x-ai-usage-account"], expected);
    }
    let first = engine
        .route_with_session(request("balanced"), Some("session_only"))
        .await;
    let selected = first.headers()["x-ai-usage-account"].clone();
    drop(first);
    let second = engine
        .route_with_session(request("balanced"), Some("session_only"))
        .await;
    assert_eq!(second.headers()["x-ai-usage-account"], selected);
    assert_eq!(second.headers()["x-ai-usage-selection"], "sticky");
}
#[tokio::test]
async fn ambiguous_submission_failure_never_replays_to_another_distribution_member() {
    let upstream = Upstream::default();
    upstream.server_error.store(true, Ordering::SeqCst);
    let (engine, _server) = fixture(upstream.clone()).await;
    let response = engine
        .route_with_identity(request("balanced"), None, Some("caller"))
        .await;
    assert_eq!(response.status(), 502);
    assert_eq!(upstream.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn simultaneous_streams_spread_and_keep_load_until_disconnect_or_configuration_change() {
    use crate::routing::reports::RequestStatus;
    use axum::body::{Body, Bytes};
    use futures_util::StreamExt;
    async fn until(mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !condition() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Routing did not settle");
    }
    let server = serve(Router::new().route("/{account}/chat", post(|| async {
        let stream = futures_util::stream::once(async { Ok::<_, std::io::Error>(Bytes::from_static(b"data: {\"model\":\"small\",\"choices\":[{\"delta\":{\"content\":\"fixture\"}}]}\n\n")) }).chain(futures_util::stream::pending());
        Response::builder().header("content-type", "text/event-stream").body(Body::from_stream(stream)).unwrap()
    }))).await;
    let adapter = Arc::new(Adapter {
        base: server.base.clone(),
        blocked_until: Arc::new(AtomicI64::new(0)),
        clock: Arc::new(AtomicI64::new(1000)),
    });
    let accounts = ["first", "second", "third"]
        .into_iter()
        .map(|id| account(id, "small", adapter.clone()))
        .collect::<Vec<_>>();
    let mut balanced = pool(
        "balanced",
        &[("first", "small"), ("second", "small"), ("third", "small")],
    );
    balanced.mode = RouteMode::LoadDistribution;
    let settings = RouterSettings {
        enabled: true,
        pools: vec![balanced],
        ..Default::default()
    };
    let engine = Arc::new(RouterEngine::new(Arc::new(MemoryStore::default())));
    engine.configure(settings.clone(), accounts.clone()).await;
    let mut req = request("balanced");
    req["stream"] = json!(true);
    let responses = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        futures_util::future::join_all(
            ["a", "b", "c"]
                .into_iter()
                .map(|caller| engine.route_with_identity(req.clone(), None, Some(caller))),
        ),
    )
    .await
    .expect("Independent callers serialized on one server");
    let destinations = responses
        .iter()
        .map(|response| response.headers()["x-ai-usage-account"].to_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(destinations, ["first", "second", "third"].into());
    assert_eq!(engine.routing_report().active.len(), 3);

    let a_account = responses[0].headers()["x-ai-usage-account"]
        .to_str()
        .unwrap()
        .to_owned();
    let worker = engine.clone();
    let queued_request = req.clone();
    let queued = tokio::spawn(async move {
        worker
            .route_with_identity(queued_request, None, Some("a"))
            .await
    });
    until(|| {
        engine
            .routing_report()
            .active
            .iter()
            .any(|row| row.status == RequestStatus::Waiting)
    })
    .await;
    let waiting = engine
        .routing_report()
        .active
        .into_iter()
        .find(|row| row.status == RequestStatus::Waiting)
        .unwrap();
    assert_eq!(waiting.target.unwrap().account_id, a_account);
    queued.abort();
    assert!(queued.await.unwrap_err().is_cancelled());

    let mut responses = responses.into_iter();
    drop(responses.next().unwrap());
    until(|| engine.routing_report().active.len() == 2).await;
    let replacement = engine
        .route_with_identity(req.clone(), None, Some("new"))
        .await;
    assert_eq!(replacement.headers()["x-ai-usage-account"], a_account);
    // Existing stream guards belong to the cancelled configuration only.
    engine.configure(settings, accounts).await;
    until(|| engine.routing_report().active.is_empty()).await;
    let fresh = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        engine.route_with_identity(req, None, Some("a")),
    )
    .await
    .unwrap();
    assert_eq!(fresh.headers()["x-ai-usage-account"], "first");
    assert_eq!(fresh.headers()["x-ai-usage-selection"], "distributed");
    drop((fresh, replacement, responses));
    until(|| engine.routing_report().active.is_empty()).await;
}

#[tokio::test]
async fn local_metadata_failure_can_try_another_server_before_generation() {
    let upstream = Upstream::default();
    let (engine, backup) = fixture(upstream.clone()).await;
    let broken = serve(Router::new()
        .route("/api/tags", get(|| async { Json(json!({"models":[{"name":"small","details":{"parameter_size":"1B","quantization_level":"Q4"}}]})) }))
        .route("/api/show", post(|| async { StatusCode::SERVICE_UNAVAILABLE }))).await;
    let mut first = account("first", "small", Arc::new(HttpProvider::OllamaLocal));
    first
        .config
        .fields
        .insert("base_url".into(), broken.base.clone());
    let adapter = Arc::new(Adapter {
        base: backup.base.clone(),
        blocked_until: Arc::new(AtomicI64::new(0)),
        clock: Arc::new(AtomicI64::new(1000)),
    });
    let mut balanced = pool("balanced", &[("first", "small"), ("second", "small")]);
    balanced.mode = RouteMode::LoadDistribution;
    engine
        .configure(
            RouterSettings {
                enabled: true,
                pools: vec![balanced],
                ..Default::default()
            },
            vec![first, account("second", "small", adapter)],
        )
        .await;
    let response = engine
        .route_with_identity(request("balanced"), None, Some("caller"))
        .await;
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["x-ai-usage-account"], "second");
    assert_eq!(upstream.calls.lock().unwrap().len(), 1);
    assert_eq!(engine.routing_report().recent[0].fallback_count, 1);
}
