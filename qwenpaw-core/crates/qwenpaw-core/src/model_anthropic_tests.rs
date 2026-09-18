use pretty_assertions::assert_eq;
use qwenpaw_storage::StoredFunctionCall;
use qwenpaw_storage::StoredToolCall;

use super::*;

#[allow(clippy::needless_pass_by_value)]
fn feed(decoder: &mut Decoder, value: Value) -> (Vec<ModelEvent>, bool) {
    decoder.parse(&value.to_string()).unwrap()
}

#[test]
fn encodes_system_tools_failed_results_and_signed_content_without_loss() {
    let mut assistant = StoredMessage::assistant_tool_calls(
        String::from("reading"),
        vec![StoredToolCall {
            id: String::from("call-1"),
            kind: String::from("function"),
            function: StoredFunctionCall {
                name: String::from("read_file"),
                arguments: String::from("{\"path\":\"test.txt\"}"),
            },
        }],
    );
    let mut result =
        StoredMessage::tool_result(String::from("call-1"), String::from("permission denied"));
    result.tool_error = Some(true);
    let tools = vec![
        json!({"type": "function", "function": {"name": "read_file", "description": "Read a file",
        "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}}),
    ];
    let mut messages = vec![
        StoredMessage::text("system", "system-a"),
        StoredMessage::text("system", "system-b"),
        StoredMessage::text("user", "read"),
        assistant.clone(),
        result,
        StoredMessage::text("user", "try again"),
    ];
    let body = request_body("fixture-model", &messages, &tools, Map::new()).unwrap();
    assert_eq!(
        body,
        json!({"model": "fixture-model", "max_tokens": 16384, "stream": true,
        "system": [{"type": "text", "text": "system-a"}, {"type": "text", "text": "system-b"}],
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "read"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "reading"}, {"type": "tool_use", "id": "call-1", "name": "read_file", "input": {"path": "test.txt"}}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "call-1", "content": "permission denied", "is_error": true}, {"type": "text", "text": "try again"}]}
        ], "tools": [{"name": "read_file", "description": "Read a file", "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}], "tool_choice": {"type": "auto"}})
    );
    let blocks = vec![
        json!({"type": "thinking", "thinking": "opaque", "signature": "signed"}),
        json!({"type": "redacted_thinking", "data": "opaque-data"}),
        json!({"type": "tool_use", "id": "call-1", "name": "read_file", "input": {"path": "test.txt"}}),
    ];
    assistant
        .provider_content
        .insert(PROTOCOL.to_owned(), blocks.clone());
    messages[3] = serde_json::from_value(serde_json::to_value(assistant).unwrap()).unwrap();
    let body = request_body("fixture-model", &messages, &tools, Map::new()).unwrap();
    assert_eq!(body["messages"][1]["content"], json!(blocks));
    for suffix in ["", "/", "/v1", "/v1/"] {
        assert_eq!(
            endpoint(&format!("http://localhost/proxy{suffix}")),
            "http://localhost/proxy/v1/messages"
        );
    }
}

#[test]
fn maps_thinking_controls_and_explicit_disable_wins() {
    for (parameters, expected) in [
        (
            json!({"max_tokens": 2048, "thinking_enable": true, "thinking_budget": 4096}),
            json!({"max_tokens": 5120, "thinking": {"type": "enabled", "budget_tokens": 4096}}),
        ),
        (
            json!({"thinking_enable": true}),
            json!({"max_tokens": 16384, "thinking": {"type": "enabled", "budget_tokens": 8192}}),
        ),
        (
            json!({"disable_thinking": true, "thinking_enable": true}),
            json!({"max_tokens": 16384, "thinking": {"type": "disabled"}}),
        ),
        (json!({"max_tokens": null}), json!({"max_tokens": 8192})),
    ] {
        let mut body =
            request_body("fixture", &[], &[], parameters.as_object().unwrap().clone()).unwrap();
        for key in ["model", "messages", "stream"] {
            body.as_object_mut().unwrap().remove(key);
        }
        assert_eq!(body, expected);
    }
}

#[test]
fn parses_fragmented_tool_input_signed_thinking_and_cumulative_usage() {
    let mut decoder = Decoder::default();
    feed(&mut decoder, json!({"type": "ping"}));
    feed(
        &mut decoder,
        json!({"type": "message_start", "message": {"usage": {"input_tokens": 10, "cache_creation_input_tokens": 5, "cache_read_input_tokens": 20, "output_tokens": 1}}}),
    );
    feed(
        &mut decoder,
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": "", "signature": ""}}),
    );
    feed(
        &mut decoder,
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "thought"}}),
    );
    feed(
        &mut decoder,
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "signature_delta", "signature": "signed"}}),
    );
    feed(
        &mut decoder,
        json!({"type": "content_block_stop", "index": 0}),
    );
    let (events, _) = feed(
        &mut decoder,
        json!({"type": "content_block_start", "index": 1, "content_block": {"type": "tool_use", "id": "call", "name": "read_file", "input": {}}}),
    );
    assert_eq!(
        events,
        vec![ModelEvent::ToolCallDelta {
            index: 1,
            id: Some(String::from("call")),
            name: Some(String::from("read_file")),
            arguments: None
        }]
    );
    for partial in ["{\"path\":", "\"test.txt\"}"] {
        let (events, _) = feed(
            &mut decoder,
            json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": partial}}),
        );
        assert_eq!(
            events,
            vec![ModelEvent::ToolCallDelta {
                index: 1,
                id: None,
                name: None,
                arguments: Some(partial.to_owned())
            }]
        );
    }
    feed(
        &mut decoder,
        json!({"type": "content_block_stop", "index": 1}),
    );
    feed(
        &mut decoder,
        json!({"type": "message_delta", "usage": {"output_tokens": 3}}),
    );
    feed(&mut decoder, json!({"type": "future_event"}));
    feed(
        &mut decoder,
        json!({"type": "message_delta", "usage": {"output_tokens": 7}}),
    );
    let (events, done) = feed(&mut decoder, json!({"type": "message_stop"}));
    assert!(done);
    assert_eq!(
        events,
        vec![
            ModelEvent::Usage(ModelUsage {
                prompt_tokens: 35,
                completion_tokens: 7,
                cache_read_tokens: 20,
                cache_write_tokens: 5,
                cache_eligible_input_tokens: 35,
                cache_observed: true
            }),
            ModelEvent::ProviderContent {
                protocol: PROTOCOL,
                content: vec![
                    json!({"type": "thinking", "thinking": "thought", "signature": "signed"}),
                    json!({"type": "tool_use", "id": "call", "name": "read_file", "input": {"path": "test.txt"}}),
                ]
            }
        ]
    );
}

#[test]
fn rejects_duplicate_tool_identity_and_nonobject_input_and_handles_unfragmented_input() {
    for input in [json!({}), json!({"path": "test.txt"})] {
        let mut decoder = Decoder::default();
        feed(
            &mut decoder,
            json!({"type": "message_start", "message": {}}),
        );
        feed(
            &mut decoder,
            json!({"type": "content_block_start", "index": 0,
            "content_block": {"type": "tool_use", "id": "call", "name": "read_file", "input": input}}),
        );
        assert!(
            decoder
                .parse(
                    &json!({"type": "content_block_start", "index": 1,
            "content_block": {"type": "tool_use", "id": "call", "name": "read_file", "input": {}}})
                    .to_string()
                )
                .is_err()
        );
        let (events, _) = feed(
            &mut decoder,
            json!({"type": "content_block_stop", "index": 0}),
        );
        assert_eq!(
            events,
            vec![ModelEvent::ToolCallDelta {
                index: 0,
                id: None,
                name: None,
                arguments: Some(input.to_string())
            }]
        );
    }
    for partial in ["[]", "{invalid"] {
        let mut decoder = Decoder::default();
        feed(
            &mut decoder,
            json!({"type": "message_start", "message": {}}),
        );
        feed(
            &mut decoder,
            json!({"type": "content_block_start", "index": 0,
            "content_block": {"type": "tool_use", "id": "call", "name": "read_file", "input": {}}}),
        );
        feed(
            &mut decoder,
            json!({"type": "content_block_delta", "index": 0,
            "delta": {"type": "input_json_delta", "partial_json": partial}}),
        );
        assert!(
            decoder
                .parse(&json!({"type": "content_block_stop", "index": 0}).to_string())
                .is_err()
        );
    }
}

#[test]
fn rejects_malformed_out_of_order_excessive_and_remote_error_events() {
    for event in [
        json!({"type": "message_stop"}),
        json!({"type": "error", "error": {"message": "private-secret"}}),
        json!({}),
    ] {
        let error = Decoder::default().parse(&event.to_string()).unwrap_err();
        assert!(!error.to_string().contains("private-secret"));
    }
    for event in [
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "orphan"}}),
        json!({"type": "content_block_start", "index": 128, "content_block": {"type": "text", "text": ""}}),
        json!({"type": "message_start"}),
        json!({"type": "message_delta", "usage": {"output_tokens": -1}}),
    ] {
        let mut decoder = Decoder::default();
        feed(
            &mut decoder,
            json!({"type": "message_start", "message": {}}),
        );
        assert!(decoder.parse(&event.to_string()).is_err());
    }
    let mut decoder = Decoder::default();
    feed(
        &mut decoder,
        json!({"type": "message_start", "message": {}}),
    );
    feed(
        &mut decoder,
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
    );
    assert!(decoder.parse("{\"type\":\"message_stop\"}").is_err());
    assert!(
        Decoder::default()
            .parse(&" ".repeat(MAX_STREAM_BYTES + 1))
            .is_err()
    );
}
