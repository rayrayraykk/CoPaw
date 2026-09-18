use futures_util::StreamExt;
use pretty_assertions::assert_eq;
use qwenpaw_storage::{StoredFunctionCall, StoredToolCall};

use super::*;

fn call(id: &str, name: &str) -> StoredToolCall {
    StoredToolCall {
        id: id.to_owned(),
        kind: String::from("function"),
        function: StoredFunctionCall {
            name: name.to_owned(),
            arguments: String::from("{\"path\":\"fixture.txt\"}"),
        },
    }
}

#[test]
fn native_parallel_history_preserves_signatures_order_and_optional_wire_ids() {
    let parts = vec![
        json!({"text": "private thought", "thought": true}),
        json!({"functionCall": {"name": "read_file", "args": {"path": "fixture.txt"}}, "thoughtSignature": "signed-first"}),
        json!({"functionCall": {"id": "native-2", "name": "read_file", "args": {"path": "fixture.txt"}}}),
    ];
    let mut assistant = StoredMessage::assistant_tool_calls(
        String::new(),
        vec![call("local-1", "read_file"), call("native-2", "read_file")],
    );
    assistant
        .provider_content
        .insert(PROTOCOL.to_owned(), parts.clone());
    let assistant: StoredMessage =
        serde_json::from_value(serde_json::to_value(assistant).unwrap()).unwrap();
    let mut failed = StoredMessage::tool_result(String::from("native-2"), String::from("denied"));
    failed.tool_error = Some(true);
    let messages = vec![
        StoredMessage::text("system", "system"),
        StoredMessage::text("developer", "developer"),
        StoredMessage::text("user", "read"),
        assistant,
        StoredMessage::tool_result(String::from("local-1"), String::from("file contents")),
        failed,
    ];
    assert_eq!(
        request_body(&messages, &[], Map::new()).unwrap(),
        json!({
            "systemInstruction": {"parts": [{"text": "system"}, {"text": "developer"}]},
            "contents": [{"role": "user", "parts": [{"text": "read"}]}, {"role": "model", "parts": parts},
                {"role": "user", "parts": [
                    {"functionResponse": {"name": "read_file", "response": {"output": "file contents"}}},
                    {"functionResponse": {"id": "native-2", "name": "read_file", "response": {"error": "denied"}}}]}],
            "generationConfig": {"thinkingConfig": {"includeThoughts": false, "thinkingBudget": 0}}
        })
    );
    assert!(request_body(&messages[4..], &[], Map::new()).is_err());
}

#[test]
fn encodes_portable_history_and_normalizes_tool_schemas_and_generation_parameters() {
    let messages = vec![
        StoredMessage::text("user", "read"),
        StoredMessage::assistant_tool_calls(
            String::from("reading"),
            vec![call("portable-1", "read_file")],
        ),
        StoredMessage::tool_result(String::from("portable-1"), String::from("contents")),
    ];
    let tool = json!({"type": "function", "function": {"name": "read_file", "description": "Read",
        "parameters": {"type": "object", "$schema": "json-schema", "additionalProperties": false,
            "$defs": {"Path": {"type": "string"}}, "properties": {
                "path": {"anyOf": [{"$ref": "#/$defs/Path"}, {"type": "null"}], "description": "Path"},
                "mode": {"type": "string", "const": "read"}}, "required": ["path"]}}});
    let options = json!({"max_tokens": 10, "max_output_tokens": 20, "temperature": 0.2, "top_p": null,
        "thinking_enable": true, "thinking_budget": 1024, "tool_choice": {"type": "function", "function": {"name": "read_file"}},
        "safety_settings": [{"category": "HARM_CATEGORY_HATE_SPEECH", "threshold": "BLOCK_ONLY_HIGH"}],
        "response_json_schema": {"type": "object", "properties": {"snake_case": {"type": "string"}}}});
    assert_eq!(
        request_body(&messages, &[tool], options.as_object().unwrap().clone()).unwrap(),
        json!({
            "contents": [{"role": "user", "parts": [{"text": "read"}]},
                {"role": "model", "parts": [{"text": "reading"}, {"functionCall": {"id": "portable-1", "name": "read_file", "args": {"path": "fixture.txt"}}}]},
                {"role": "user", "parts": [{"functionResponse": {"id": "portable-1", "name": "read_file", "response": {"output": "contents"}}}]}],
            "tools": [{"functionDeclarations": [{"name": "read_file", "description": "Read", "parameters": {
                "type": "OBJECT", "properties": {"path": {"type": "STRING", "description": "Path"}, "mode": {"type": "STRING", "enum": ["read"]}}, "required": ["path"]}}]}],
            "toolConfig": {"functionCallingConfig": {"mode": "ANY", "allowedFunctionNames": ["read_file"]}},
            "safetySettings": [{"category": "HARM_CATEGORY_HATE_SPEECH", "threshold": "BLOCK_ONLY_HIGH"}],
            "generationConfig": {"maxOutputTokens": 20, "temperature": 0.2,
                "thinkingConfig": {"includeThoughts": true, "thinkingBudget": 1024},
                "responseJsonSchema": {"type": "object", "properties": {"snake_case": {"type": "string"}}}}
        })
    );
    for suffix in ["", "/", "/v1beta", "/v1beta/"] {
        assert_eq!(
            endpoint(
                &format!("http://localhost/proxy{suffix}"),
                "models/gemini-3.1-pro"
            )
            .unwrap(),
            "http://localhost/proxy/v1beta/models/gemini-3.1-pro:streamGenerateContent?alt=sse"
        );
    }
    assert_eq!(
        endpoint("http://localhost/proxy/v1", "gemini-test").unwrap(),
        "http://localhost/proxy/v1/models/gemini-test:streamGenerateContent?alt=sse"
    );
    for model in [
        "",
        "../secret",
        "test?key=x",
        "models/",
        ".",
        "test/models/x",
    ] {
        assert!(endpoint("http://localhost", model).is_err());
    }
}

#[test]
fn generation_controls_and_schema_failures_are_explicit() {
    for choice in ["auto", "none", "required", "read_file"] {
        let body = request_body(&[], &[], json!({"tool_choice": choice, "thinking_enable": true,
            "disable_thinking": true, "thinking_config": {"include_thoughts": true, "thinking_budget": 2048}}).as_object().unwrap().clone()).unwrap();
        assert_eq!(
            body["generationConfig"],
            json!({"thinkingConfig": {"includeThoughts": false, "thinkingBudget": 0}})
        );
        assert!(body["toolConfig"]["functionCallingConfig"].is_object());
    }
    let body = request_body(
        &[],
        &[],
        json!({"thinking_config": {"include_thoughts": true, "thinking_budget": 2048},
        "tool_config": {"function_calling_config": {"mode": "NONE"}}})
        .as_object()
        .unwrap()
        .clone(),
    )
    .unwrap();
    assert_eq!(
        body,
        json!({"contents": [], "generationConfig": {"thinkingConfig": {"includeThoughts": true, "thinkingBudget": 2048}},
        "toolConfig": {"functionCallingConfig": {"mode": "NONE"}}})
    );
    for key in [
        "contents",
        "systemInstruction",
        "system_instruction",
        "generationConfig",
        "generation_config",
        "tools",
    ] {
        for params in [json!({key: []}), json!({"extra_body": {key: []}})] {
            let options = crate::ModelRequestOptions {
                protocol: crate::ModelProtocol::GeminiGenerateContent,
                generate_kwargs: params.as_object().unwrap().clone(),
                ..Default::default()
            };
            assert!(options.validate().is_err(), "{key}");
        }
    }
    assert!(schema::normalize(&json!({"$ref": "#/$defs/missing"})).is_err());
    assert!(schema::normalize(&json!({"$ref": "https://invalid.example/schema"})).is_err());
    assert_eq!(schema::normalize(&json!({"$defs": {"Node": {"type": "object", "properties": {"next": {"$ref": "#/$defs/Node"}}}},
        "$ref": "#/$defs/Node"})).unwrap(), json!({"type": "OBJECT", "properties": {"next": {"type": "OBJECT", "description": "(circular: Node)"}}}));
}

#[test]
fn schema_expansion_and_native_history_are_bounded_and_validated() {
    let mut definitions = Map::new();
    definitions.insert(String::from("Node0"), json!({"type": "string"}));
    for index in 1..16 {
        let reference = format!("#/$defs/Node{}", index - 1);
        definitions.insert(
            format!("Node{index}"),
            json!({"type": "object", "properties": {
            "left": {"$ref": reference}, "right": {"$ref": reference}}}),
        );
    }
    assert!(schema::normalize(&json!({"$defs": definitions, "$ref": "#/$defs/Node15"})).is_err());
    let mut message =
        StoredMessage::assistant_tool_calls(String::new(), vec![call("local", "read_file")]);
    message.provider_content.insert(PROTOCOL.to_owned(), vec![]);
    assert!(request_body(&[message.clone()], &[], Map::new()).is_err());
    message.provider_content.insert(
        PROTOCOL.to_owned(),
        vec![json!({"functionCall": {"name": "different", "args": {}}})],
    );
    assert!(request_body(&[message], &[], Map::new()).is_err());
}

#[test]
fn decoder_preserves_native_parts_and_uses_latest_usage_snapshot() {
    let parts = vec![
        json!({"text": "hidden", "thought": true}),
        json!({"text": "你好"}),
        json!({"text": "", "thoughtSignature": "signature-at-end"}),
        json!({"functionCall": {"name": "read_file", "args": {"path": "fixture.txt"}}, "thoughtSignature": "function-signature"}),
    ];
    let mut decoder = Decoder::default();
    let events = decoder.parse(&json!({"candidates": [{"index": 0, "content": {"role": "model", "parts": parts}, "finishReason": "STOP"}],
        "usageMetadata": {"promptTokenCount": 10, "totalTokenCount": 12}}).to_string()).unwrap();
    assert_eq!(events[0], ModelEvent::TextDelta(String::from("你好")));
    let ModelEvent::ToolCallDelta {
        index,
        id,
        name,
        arguments,
    } = &events[1]
    else {
        panic!("missing call")
    };
    assert_eq!(
        (*index, name.as_deref(), arguments.as_deref()),
        (0, Some("read_file"), Some("{\"path\":\"fixture.txt\"}"))
    );
    assert!(id.as_ref().unwrap().starts_with("gemini_"));
    assert_eq!(events.len(), 2);
    assert_eq!(
        decoder
            .parse(
                &json!({"usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 3,
        "thoughtsTokenCount": 5, "totalTokenCount": 18, "cachedContentTokenCount": 4}})
                .to_string()
            )
            .unwrap(),
        vec![]
    );
    assert_eq!(
        decoder.finish().unwrap(),
        vec![
            ModelEvent::ProviderContent {
                protocol: PROTOCOL,
                content: parts
            },
            ModelEvent::Usage(ModelUsage {
                prompt_tokens: 10,
                completion_tokens: 8,
                cache_read_tokens: 4,
                cache_write_tokens: 0,
                cache_eligible_input_tokens: 10,
                cache_observed: true
            })
        ]
    );
}

#[test]
fn rejects_incomplete_blocked_malformed_and_excessive_responses() {
    for value in [
        json!({"error": {"message": "private-key"}}),
        json!({"promptFeedback": {"blockReason": "SAFETY"}}),
        json!({"candidates": [{"finishReason": "SAFETY"}]}),
        json!({"candidates": [{"finishReason": 1}]}),
        json!({"candidates": [null]}),
        json!({"candidates": [{"index": -1}]}),
        json!({"usageMetadata": {"promptTokenCount": -1}}),
    ] {
        let error = Decoder::default().parse(&value.to_string()).unwrap_err();
        assert!(!error.to_string().contains("private-key"));
    }
    for part in [
        json!({"text": 1}),
        json!({"text": "hi", "thoughtSignature": 1}),
        json!({"thought": "true", "text": "hidden"}),
        json!({"functionCall": {"name": "read", "args": []}}),
        json!({"functionCall": {"args": {}}}),
        json!({"inlineData": {}}),
    ] {
        assert!(
            Decoder::default()
                .parse(&json!({"candidates": [{"content": {"parts": [part]}}]}).to_string())
                .is_err()
        );
    }
    let mut decoder = Decoder::default();
    assert!(decoder.finish().is_err());
    let call = json!({"candidates": [{"content": {"parts": [{"functionCall": {"id": "one", "name": "read", "args": {}}}]}}]});
    decoder.parse(&call.to_string()).unwrap();
    assert!(decoder.parse(&call.to_string()).is_err());
    assert!(
        decoder
            .parse(&json!({"candidates": [{"finishReason": "MAX_TOKENS"}]}).to_string())
            .is_err()
    );
    let mut decoder = Decoder::default();
    decoder
        .parse(&json!({"candidates": [{"finishReason": "MAX_TOKENS"}]}).to_string())
        .unwrap();
    assert!(decoder.finish().is_ok());
    assert!(
        decoder
            .parse(&json!({"candidates": [{"content": {"parts": [{"text": "late"}]}}]}).to_string())
            .is_err()
    );
    let mut decoder = Decoder {
        bytes: MAX_STREAM_BYTES,
        ..Default::default()
    };
    assert!(decoder.parse("{}").is_err());
}

#[tokio::test]
async fn sse_transport_handles_utf8_fragmentation_eof_and_idle_timeout() {
    let data = format!(
        "data: {}\r\n\r\ndata: {}\n\n",
        json!({"candidates": [{"content": {"parts": [{"text": "中文"}]}, "finishReason": "STOP"}]}),
        json!({"usageMetadata": {"promptTokenCount": 1, "totalTokenCount": 2}})
    );
    let source = futures_util::stream::iter(
        data.into_bytes()
            .into_iter()
            .map(|byte| Ok(bytes::Bytes::from(vec![byte]))),
    );
    let events = crate::model::model_event_stream(
        Box::pin(source),
        std::time::Duration::from_millis(100),
        crate::ModelProtocol::GeminiGenerateContent,
        None,
    )
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
    assert_eq!(
        events,
        vec![
            ModelEvent::TextDelta(String::from("中文")),
            ModelEvent::ProviderContent {
                protocol: PROTOCOL,
                content: vec![json!({"text": "中文"})]
            },
            ModelEvent::Usage(ModelUsage {
                prompt_tokens: 1,
                completion_tokens: 1,
                cache_eligible_input_tokens: 1,
                ..Default::default()
            })
        ]
    );
    for data in ["data: [DONE]\n\n", "data: {}\n\n"] {
        let source = futures_util::stream::iter([Ok(bytes::Bytes::from_static(data.as_bytes()))]);
        let results = crate::model::model_event_stream(
            Box::pin(source),
            std::time::Duration::from_millis(100),
            crate::ModelProtocol::GeminiGenerateContent,
            None,
        )
        .collect::<Vec<_>>()
        .await;
        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
    }
    let mut stream = crate::model::model_event_stream(
        Box::pin(futures_util::stream::pending()),
        std::time::Duration::from_millis(100),
        crate::ModelProtocol::GeminiGenerateContent,
        None,
    );
    assert!(matches!(
        stream.next().await,
        Some(Err(ModelError::StreamIdleTimeout))
    ));
    assert!(stream.next().await.is_none());
}
