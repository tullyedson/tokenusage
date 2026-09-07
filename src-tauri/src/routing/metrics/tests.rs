use super::*;
use serde_json::json;

#[test]
fn counters_keep_cache_and_reasoning_as_subsets_and_missing_is_not_zero() {
    let tokens = TokenUsage::from_response(&json!({"usage":{"prompt_tokens":200,"completion_tokens":50,
        "prompt_tokens_details":{"cached_tokens":100},"completion_tokens_details":{"reasoning_tokens":20}}})).unwrap();
    assert_eq!(
        (
            tokens.input,
            tokens.output,
            tokens.total,
            tokens.cached_input,
            tokens.reasoning
        ),
        (Some(200), Some(50), Some(250), Some(100), Some(20))
    );
    assert!(TokenUsage::from_response(&json!({"usage":null})).is_none());
    assert!(TokenUsage::from_response(&json!({"usage":{}})).is_none());
    let zero =
        TokenUsage::from_response(&json!({"usage":{"prompt_tokens":0,"completion_tokens":0}}))
            .unwrap();
    assert_eq!(zero.total, Some(0));
    let partial = TokenUsage::from_response(&json!({"usage":{"completion_tokens":20}})).unwrap();
    assert_eq!((partial.input, partial.total), (None, None));
    let mut metrics = CallMetrics {
        tokens: Some(tokens),
        context_limit: Some(1000),
        ..Default::default()
    };
    metrics.context_percentage();
    assert_eq!(metrics.context_used_percent, Some(25.0));
    metrics.context_limit = None;
    metrics.context_percentage();
    assert_eq!(metrics.context_used_percent, None);
}

#[test]
fn malformed_or_inconsistent_counts_do_not_become_usage_percentages() {
    for usage in [
        json!({"prompt_tokens":-1}),
        json!({"completion_tokens":1.2}),
        json!({"total_tokens":"20"}),
        json!({"total_tokens":MAX_COUNT+1}),
        json!({"prompt_tokens":200,"completion_tokens":50,"total_tokens":251}),
        json!({"prompt_tokens":200,"total_tokens":50}),
    ] {
        assert!(TokenUsage::from_response(&json!({"usage":usage})).is_none());
    }
}

fn snapshot(checked_at: i64, resets_at: i64, used_percent: f64) -> AllowanceSnapshot {
    AllowanceSnapshot {
        checked_at,
        windows: vec![AllowanceWindow {
            id: "weekly",
            label: "Weekly",
            used_percent,
            resets_at,
        }],
    }
}
#[test]
fn allowance_differences_are_percentage_points_and_resets_remain_unknown() {
    let before = snapshot(100, 200, 10.0);
    let observation = AllowanceObservation::compare(&before, &snapshot(110, 201, 10.25));
    assert!(observation.status == AllowanceStatus::Observed);
    assert_eq!(observation.changes[0].percentage_points, 0.25);
    for after in [
        snapshot(99, 200, 10.5),
        snapshot(110, 400, 10.5),
        snapshot(200, 200, 10.5),
        snapshot(110, 200, 9.0),
        snapshot(110, 200, f64::NAN),
    ] {
        assert!(
            AllowanceObservation::compare(&before, &after).status == AllowanceStatus::Unavailable
        );
    }
    assert_eq!(
        AllowanceObservation::compare(&before, &snapshot(110, 200, 10.0)).changes[0]
            .percentage_points,
        0.0
    );
}

#[test]
fn fragmented_stream_usage_is_replaced_never_summed_or_taken_from_message_content() {
    let source = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"private fixture\",\"usage\":{\"total_tokens\":999}}}],\"usage\":{\"prompt_tokens\":200,\"completion_tokens\":0}}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":200,\"completion_tokens\":50,\"total_tokens\":250}}\r\n\r\n",
        "data: {\"usage\":null}\n\ndata: [DONE]\n\n");
    let mut stream = crate::routing::stream::AliasStream::default();
    let mut bytes = Vec::new();
    for chunk in source.as_bytes().chunks(3) {
        bytes.extend(stream.push(chunk, "pool").unwrap());
    }
    bytes.extend(stream.finish("pool"));
    assert_eq!(bytes, source.as_bytes());
    assert_eq!(stream.tokens().unwrap().total, Some(250));
    assert!(stream.completed());
    stream
        .push(b"data: {\"usage\":{\"total_tokens\":-1}}\n\n", "pool")
        .unwrap();
    assert!(stream.tokens().is_none());
}

#[test]
fn stream_usage_defaults_preserve_explicit_opt_out_and_other_options() {
    let mut body = json!({"stream":true,"stream_options":{"include_obfuscation":false}});
    request_stream_usage(&mut body);
    assert_eq!(
        body["stream_options"],
        json!({"include_usage":true,"include_obfuscation":false})
    );
    for options in [
        json!({"include_usage":false}),
        Value::Null,
        json!("invalid"),
    ] {
        let mut body = json!({"stream":true,"stream_options":options});
        let original = body.clone();
        request_stream_usage(&mut body);
        assert_eq!(body, original);
    }
    let mut body = json!({"stream":false});
    request_stream_usage(&mut body);
    assert!(body.get("stream_options").is_none());
}
