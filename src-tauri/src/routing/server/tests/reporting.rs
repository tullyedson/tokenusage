use super::*;
use crate::routing::reports::RequestStatus;
use axum::body::{Body, Bytes};
use futures_util::StreamExt;

async fn until(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !predicate() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Report lifecycle did not settle");
}
async fn configured(base: &str) -> Arc<RouterEngine> {
    let engine = Arc::new(RouterEngine::new(Arc::new(MemoryStore::default())));
    let provider = Arc::new(Adapter {
        base: base.into(),
        blocked_until: Arc::new(AtomicI64::new(0)),
        clock: Arc::new(AtomicI64::new(1000)),
    });
    let mut connection = account("first", "server-x", provider);
    connection.config.label = "Desk account".into();
    connection.config.provider_type = "fixture-provider".into();
    engine
        .configure(
            RouterSettings {
                enabled: true,
                pools: vec![pool("flash-models", &[("first", "server-x")])],
                ..Default::default()
            },
            vec![connection],
        )
        .await;
    engine
}

#[tokio::test]
async fn live_stream_reports_the_real_destination_and_client_disconnect_clears_active_state() {
    let server = serve(Router::new().route(
        "/first/chat",
        post(|| async {
            let stream = futures_util::stream::once(async {
                Ok::<_, std::io::Error>(Bytes::from_static(
                    b"data: {\"choices\":[{\"delta\":{\"content\":\"private output\"}}]}\n\n",
                ))
            })
            .chain(futures_util::stream::pending());
            Response::builder()
                .header("content-type", "text/event-stream")
                .body(Body::from_stream(stream))
                .unwrap()
        }),
    ))
    .await;
    let engine = configured(&server.base).await;
    let mut req = request("flash-models");
    req["stream"] = json!(true);
    let response = engine
        .route_with_session(req, Some("private_session_id"))
        .await;
    let report = engine.routing_report();
    assert_eq!(report.active.len(), 1);
    assert_eq!(report.active[0].status, RequestStatus::Streaming);
    assert_eq!(report.active[0].pool, "flash-models");
    assert_eq!(
        report.active[0].id,
        response.headers()["x-ai-usage-request-id"]
    );
    let destination = report.active[0].target.as_ref().unwrap();
    assert_eq!(destination.provider_id, "fixture-provider");
    assert_eq!(destination.account_label, "Desk account");
    assert_eq!(destination.model, "server-x");
    engine.clear_routing_history();
    assert_eq!(engine.routing_report().active.len(), 1);
    drop(response);
    until(|| engine.routing_report().active.is_empty()).await;
    let report = engine.routing_report();
    assert_eq!(report.recent[0].status, RequestStatus::Cancelled);
    let metadata = serde_json::to_string(&report).unwrap();
    for private in [
        "private output",
        "fictional test prompt",
        "private_session_id",
        &server.base,
    ] {
        assert!(!metadata.contains(private));
    }
}

#[tokio::test]
async fn reports_distinguish_complete_error_and_truncated_streams_without_changing_their_bytes() {
    for (source, status) in [
        (
            "data: {\"choices\":[{\"delta\":{\"content\":\"private\"}}]}\n\ndata: [DONE]\n\n",
            RequestStatus::Completed,
        ),
        (
            "data: {\"error\":{\"message\":\"private error\"}}\n\ndata: [DONE]\n\n",
            RequestStatus::Failed,
        ),
        (
            "data: {\"choices\":[{\"delta\":{\"content\":\"private\"}}]}\n\n",
            RequestStatus::Failed,
        ),
    ] {
        let server = serve(Router::new().route(
            "/first/chat",
            post(move || async move { ([("content-type", "text/event-stream")], source) }),
        ))
        .await;
        let engine = configured(&server.base).await;
        let mut req = request("flash-models");
        req["stream"] = json!(true);
        let response = engine.route(req).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(response.into_body(), 10000)
                .await
                .unwrap()
                .as_ref(),
            source.as_bytes()
        );
        until(|| engine.routing_report().active.is_empty()).await;
        let report = engine.routing_report();
        assert_eq!(report.recent[0].status, status);
        assert_eq!(report.recent[0].http_status, Some(200));
        assert_eq!(
            report.recent[0].metrics.response_bytes,
            Some(source.len() as u64)
        );
        assert!(!serde_json::to_string(&report).unwrap().contains("private"));
    }
}

#[tokio::test]
async fn aborting_a_queued_http_future_finalizes_its_report() {
    let engine = Arc::new(RouterEngine::new(Arc::new(MemoryStore::default())));
    let provider = Arc::new(Adapter {
        base: "http://127.0.0.1:1".into(),
        blocked_until: Arc::new(AtomicI64::new(0)),
        clock: Arc::new(AtomicI64::new(1000)),
    });
    let connection = account("first", "server-x", provider);
    let serial = connection.serial.clone();
    let held = serial.lock().await;
    engine
        .configure(
            RouterSettings {
                enabled: true,
                pools: vec![pool("flash-models", &[("first", "server-x")])],
                ..Default::default()
            },
            vec![connection],
        )
        .await;
    let worker = engine.clone();
    let pending = tokio::spawn(async move { worker.route(request("flash-models")).await });
    until(|| {
        engine
            .routing_report()
            .active
            .first()
            .is_some_and(|row| row.status == RequestStatus::Waiting)
    })
    .await;
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    assert!(engine.routing_report().active.is_empty());
    assert_eq!(
        engine.routing_report().recent[0].status,
        RequestStatus::Cancelled
    );
    drop(held);
}
