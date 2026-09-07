use super::*;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::get,
    Json, Router,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[test]
fn reads_real_catalog_bounds_without_treating_output_as_context() {
    let limits = ModelLimits::from_catalog(&json!({"limit":{"context":1000000,"output":131072}}));
    assert_eq!(limits.context, Some(1_000_000));
    assert_eq!(limits.output, Some(131_072));
    let local = ModelLimits::from_catalog(&json!({"max_model_len":32768,"context_length":1000000}));
    assert_eq!(local.context, Some(32768));
    let free = ModelLimits::from_catalog(
        &json!({"context_length":1000000,"top_provider":{"context_length":262144,"max_completion_tokens":65536}}),
    );
    assert_eq!(free.context, Some(262144));
    assert_eq!(free.output, Some(65536));
    for invalid in [
        json!(0),
        json!(-1),
        json!(1.5),
        json!("1000000"),
        json!(9_007_199_254_740_992u64),
        Value::Null,
    ] {
        assert_eq!(
            ModelLimits::from_catalog(&json!({"context_length":invalid})).context,
            None
        );
    }
    assert_eq!(
        ModelLimits::from_catalog(&json!({"max_output_tokens":131072})).context,
        None
    );
}

#[test]
fn chains_take_each_lowest_bound_and_do_not_ignore_unknown_members() {
    let large = ModelLimits {
        context: Some(1_000_000),
        input: None,
        output: Some(131072),
    };
    let cloud = ModelLimits {
        context: Some(1_048_576),
        ..large
    };
    assert_eq!(ModelLimits::intersection(&[large, cloud]), large);
    let small = ModelLimits {
        context: Some(32768),
        input: Some(30000),
        output: Some(8192),
    };
    assert_eq!(ModelLimits::intersection(&[large, small]), small);
    assert_eq!(
        ModelLimits::intersection(&[large, ModelLimits::default()]),
        ModelLimits::default()
    );
    assert_eq!(ModelLimits::intersection(&[]), ModelLimits::default());
}

#[test]
fn local_ollama_uses_configured_context_instead_of_the_trained_maximum() {
    let mut value = json!({"model_info":{"general.architecture":"example","example.context_length":1048576,"vision.context_length":2048}});
    assert_eq!(ollama_limits(&value, false).context, Some(1048576));
    assert_eq!(ollama_limits(&value, true).context, None);
    value["parameters"] = json!("temperature 0.7\nnum_ctx 65536\nnum_predict 4096");
    assert_eq!(ollama_limits(&value, true).context, Some(65536));
    // max_tokens overrides num_predict on the OpenAI endpoint; not an output maximum.
    assert_eq!(ollama_limits(&value, true).output, None);
    value["parameters"] = json!("num_ctx 2000000");
    assert_eq!(ollama_limits(&value, true).context, Some(1048576));
    value["parameters"] = json!("num_ctx -1");
    assert_eq!(ollama_limits(&value, true).context, None);
}

#[derive(Clone, Default)]
struct Fixture {
    failed: Arc<AtomicBool>,
    reads: Arc<AtomicUsize>,
}
async fn published(
    State(f): State<Fixture>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    assert!(!headers.contains_key("authorization"));
    assert!(headers["user-agent"]
        .to_str()
        .unwrap()
        .starts_with("AI-Usage/"));
    f.reads.fetch_add(1, Ordering::SeqCst);
    if f.failed.load(Ordering::SeqCst) {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    Ok(Json(
        json!({"opencode-go":{"models":{"fixture":{"limit":{"context":1000000,"output":131072}}}}, "unrelated":{"models":{"fixture":{"limit":{"context":2000000}}}}}),
    ))
}
#[tokio::test]
async fn published_metadata_is_bounded_cached_provider_specific_and_never_authenticated() {
    let fixture = Fixture::default();
    let tcp = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let url = format!(
        "http://127.0.0.1:{}/models",
        tcp.local_addr().unwrap().port()
    )
    .parse()
    .unwrap();
    let app = Router::new()
        .route("/models", get(published))
        .with_state(fixture.clone());
    let task = tokio::spawn(async move {
        axum::serve(tcp, app).await.unwrap();
    });
    let cache = PublishedLimits::new(url);
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let first = cache.read(&client, 1000).await;
    assert_eq!(first["opencode-go"]["fixture"].context, Some(1_000_000));
    assert!(!first.contains_key("unrelated"));
    cache.read(&client, 1001).await;
    assert_eq!(fixture.reads.load(Ordering::SeqCst), 1);
    fixture.failed.store(true, Ordering::SeqCst);
    assert!(cache.read(&client, 1300).await.is_empty());
    fixture.failed.store(false, Ordering::SeqCst);
    assert!(cache.read(&client, 1301).await.is_empty());
    assert_eq!(
        cache.read(&client, 1330).await["opencode-go"]["fixture"].output,
        Some(131072)
    );
    task.abort();
}
