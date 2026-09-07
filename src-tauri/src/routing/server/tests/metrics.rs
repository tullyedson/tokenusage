use super::*;
use crate::routing::reports::RequestStatus;
use axum::body::{Body, Bytes};

#[tokio::test]
async fn http_bodies_and_stream_usage_are_measured_without_retaining_content_or_gating_on_metadata()
{
    const RESPONSE: &str = "  {\"model\":\"server-x\",\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"private response 世界\"}}],\"usage\":{\"prompt_tokens\":200,\"completion_tokens\":50,\"total_tokens\":250}}\n";
    const STREAM: &str = "data: {\"model\":\"server-x\",\"choices\":[{\"delta\":{\"content\":\"private response 世界\"}}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":200,\"completion_tokens\":50,\"total_tokens\":250}}\n\ndata: [DONE]\n\n";
    let upstream = serve(Router::new()
        .route("/v1/models", get(|| async { Json(json!({"data":[{"id":"server-x","context_length":1000},{"id":"server-y","context_length":2000}]})) }))
        .route("/v1/chat/completions", post(|Json(body):Json<Value>| async move {
            if body["stream"] == true {
                assert_eq!(body["stream_options"]["include_usage"],true);
                let chunks = STREAM.as_bytes().chunks(3).map(|chunk| Ok::<_,std::io::Error>(Bytes::copy_from_slice(chunk))).collect::<Vec<_>>();
                Response::builder().header("content-type","text/event-stream").body(Body::from_stream(futures_util::stream::iter(chunks))).unwrap()
            } else { ([("content-type","application/json")],RESPONSE).into_response() }
        }))).await;
    let store = Arc::new(MemoryStore::default());
    store.set("router", "client_token", CLIENT_KEY).unwrap();
    let clock = Arc::new(AtomicI64::new(1000));
    let clock_fn = clock.clone();
    let engine = Arc::new(
        RouterEngine::new(store).with_clock(Arc::new(move || clock_fn.load(Ordering::SeqCst))),
    );
    let mut local = account("local", "server-x", Arc::new(HttpProvider::VllmLocal));
    local
        .config
        .fields
        .insert("base_url".into(), upstream.base.clone());
    engine
        .configure(
            RouterSettings {
                enabled: true,
                pools: vec![pool(
                    "measured-pool",
                    &[("local", "server-y"), ("local", "server-x")],
                )],
                ..Default::default()
            },
            vec![local],
        )
        .await;
    engine.model_list().await.unwrap();
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = tcp.local_addr().unwrap().port();
    let app = application(engine.clone(), port);
    let router = TestServer {
        base: format!("http://127.0.0.1:{port}"),
        task: tokio::spawn(async move {
            axum::serve(tcp, app).await.unwrap();
        }),
    };
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    for (stream, chunked) in [(false, false), (true, true), (false, true)] {
        let request = format!("  {{\"model\":\"measured-pool\",\"stream\":{stream},\"messages\":[{{\"role\":\"user\",\"content\":\"private request café 世界\"}}]}}\n  ");
        let size = request.len() as u64;
        let send = client
            .post(format!("{}/v1/chat/completions", router.base))
            .bearer_auth(CLIENT_KEY)
            .header("content-type", "application/json");
        let send = if chunked {
            let chunks = request
                .into_bytes()
                .chunks(5)
                .map(|chunk| Ok::<_, std::io::Error>(Bytes::copy_from_slice(chunk)))
                .collect::<Vec<_>>();
            send.body(reqwest::Body::wrap_stream(futures_util::stream::iter(
                chunks,
            )))
        } else {
            send.body(request)
        };
        let response = send.send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let id = response.headers()["x-ai-usage-request-id"]
            .to_str()
            .unwrap()
            .to_owned();
        let output = response.text().await.unwrap();
        assert!(output.contains("private response 世界"));
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !engine.routing_report().active.is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let report = engine.routing_report();
        let row = report.recent.iter().find(|row| row.id == id).unwrap();
        assert_eq!(row.status, RequestStatus::Completed);
        assert_eq!(row.metrics.request_bytes, Some(size));
        assert_eq!(
            row.metrics.response_bytes,
            Some(if stream { STREAM.len() } else { RESPONSE.len() } as u64)
        );
        assert_eq!(row.metrics.tokens.unwrap().total, Some(250));
        assert_eq!(row.metrics.context_limit, Some(1000));
        assert_eq!(row.metrics.context_used_percent, Some(25.0));
        let serialized = serde_json::to_string(&report).unwrap();
        for private in [
            "private request",
            "private response",
            CLIENT_KEY,
            &upstream.base,
        ] {
            assert!(!serialized.contains(private));
        }
    }
    clock.store(1301, Ordering::SeqCst);
    let response = engine.route(request("measured-pool")).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        engine.routing_report().recent[0]
            .metrics
            .context_used_percent,
        None
    );
    assert_eq!(
        engine.routing_report().recent[0]
            .metrics
            .tokens
            .unwrap()
            .total,
        Some(250)
    );
}
