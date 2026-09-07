use super::*;

#[tokio::test]
async fn long_tool_history_passes_through_http_and_local_adapter_without_truncation() {
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
    let store = Arc::new(MemoryStore::default());
    store.set("router", "client_token", CLIENT_KEY).unwrap();
    let engine = Arc::new(RouterEngine::new(store));
    let mut local = account("local", "x", Arc::new(HttpProvider::VllmLocal));
    local
        .config
        .fields
        .insert("base_url".into(), server.base.clone());
    engine
        .configure(
            RouterSettings {
                enabled: true,
                pools: vec![pool("long-context-pool", &[("local", "server-x")])],
                ..Default::default()
            },
            vec![local],
        )
        .await;
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = tcp.local_addr().unwrap().port();
    let app = application(engine, port);
    let _router = TestServer {
        base: format!("http://127.0.0.1:{port}"),
        task: tokio::spawn(async move {
            axum::serve(tcp, app).await.unwrap();
        }),
    };
    let mut messages = vec![
        json!({"role":"system","content":"Fictional long tool conversation."}),
        json!({"role":"user","content":[{"type":"text","text":"Keep every tool result: café 世界."}]}),
    ];
    for index in 0..600 {
        let id = format!("call_{index}");
        messages.push(json!({"role":"assistant","content":null,"tool_calls":[{
            "id":id,"type":"function","function":{"name":"fixture","arguments":format!("{{\"index\":{index}}}")}
        }]}));
        messages.push(
            json!({"role":"tool","tool_call_id":id,"content":format!("Fictional result {index}")}),
        );
    }
    messages.push(json!({"role":"user","content":"Use all the preceding fictional results."}));
    assert_eq!(messages.len(), 1203);
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    for stream in [false, true] {
        let request = json!({"model":"long-context-pool","messages":messages,"stream":stream,
            "tools":[{"type":"function","function":{"name":"fixture","parameters":{"type":"object","properties":{"index":{"type":"integer"}}}}}]});
        let response = client
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .bearer_auth(CLIENT_KEY)
            .json(&request)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-ai-usage-upstream-model"], "server-x");
        if stream {
            assert!(response.text().await.unwrap().ends_with("data: [DONE]\n\n"));
        } else {
            assert_eq!(
                response.json::<Value>().await.unwrap()["model"],
                "long-context-pool"
            );
        }
        let mut expected = request;
        expected["model"] = json!("server-x");
        assert_eq!(upstream.calls.lock().unwrap().last().unwrap().1, expected);
    }
    assert_eq!(upstream.calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn malformed_chat_messages_report_shape_errors_before_routing() {
    let store = Arc::new(MemoryStore::default());
    store.set("router", "client_token", CLIENT_KEY).unwrap();
    let engine = Arc::new(RouterEngine::new(store));
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = tcp.local_addr().unwrap().port();
    let app = application(engine, port);
    let _router = TestServer {
        base: format!("http://127.0.0.1:{port}"),
        task: tokio::spawn(async move {
            axum::serve(tcp, app).await.unwrap();
        }),
    };
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let mut late_invalid = vec![json!({"role":"user","content":"Fictional message."}); 1200];
    late_invalid.push(json!({"content":"Fictional invalid tail."}));
    let array_error = "messages must be a non-empty array of chat messages.";
    let role_error = "Each chat message must be an object with a non-empty string role.";
    for (messages, expected) in [
        (None, array_error),
        (Some(Value::Null), array_error),
        (Some(json!({"role":"user"})), array_error),
        (Some(json!([])), array_error),
        (Some(json!(["invalid"])), role_error),
        (Some(json!([null])), role_error),
        (Some(json!([{"content":"Fictional content."}])), role_error),
        (Some(json!([{"role":1}])), role_error),
        (Some(json!([{"role":""}])), role_error),
        (Some(json!(late_invalid)), role_error),
    ] {
        let mut request = json!({"model":"server-x"});
        if let Some(messages) = messages {
            request["messages"] = messages;
        }
        let response = client
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .bearer_auth(CLIENT_KEY)
            .json(&request)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let error = response.json::<Value>().await.unwrap();
        assert_eq!(error["error"]["type"], "invalid_request");
        assert_eq!(error["error"]["message"], expected);
    }
}
