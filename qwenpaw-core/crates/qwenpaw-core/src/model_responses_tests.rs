use futures_util::StreamExt;
use pretty_assertions::assert_eq;
use qwenpaw_storage::{StoredFunctionCall, StoredToolCall, StoredUserInput, StoredUserPart};

use super::*;

fn native_call() -> Value {
    json!({"type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "read_file",
        "arguments": "{\"path\":\"fixture.txt\"}", "status": "completed"})
}

fn message(text: &str) -> Value {
    json!({"type": "message", "id": "msg_1", "role": "assistant", "status": "completed",
        "content": [{"type": "output_text", "text": text, "annotations": []}]})
}

fn completed(output: &[Value]) -> Value {
    json!({"type": "response.completed", "response": {"id": "resp_1", "status": "completed",
        "output": output, "usage": {"input_tokens": 10, "output_tokens": 8,
            "input_tokens_details": {"cached_tokens": 4}, "output_tokens_details": {"reasoning_tokens": 5}}}})
}

#[test]
fn native_history_replays_reasoning_call_ids_images_and_tool_outputs() {
    let native = vec![
        json!({"type": "reasoning", "id": "rs_1", "summary": [], "encrypted_content": "opaque"}),
        native_call(),
    ];
    let mut assistant = StoredMessage::assistant_tool_calls(
        String::new(),
        vec![StoredToolCall {
            id: "call_1".into(),
            kind: "function".into(),
            function: StoredFunctionCall {
                name: "read_file".into(),
                arguments: "{\"path\":\"fixture.txt\"}".into(),
            },
        }],
    );
    assistant
        .provider_content
        .insert(PROTOCOL.into(), native.clone());
    let assistant: StoredMessage =
        serde_json::from_value(serde_json::to_value(assistant).unwrap()).unwrap();
    let mut user = StoredMessage::text("user", "look");
    user.user_input = Some(StoredUserInput {
        item_id: "user-1".into(),
        parts: vec![
            StoredUserPart::Text {
                text: "look".into(),
            },
            StoredUserPart::Image {
                path: "image.png".into(),
                mime_type: "image/png".into(),
                size: 1,
                data: Some("YQ==".into()),
            },
        ],
    });
    let messages = vec![
        StoredMessage::text("system", "system"),
        user,
        assistant,
        StoredMessage::tool_result("call_1".into(), "contents".into()),
    ];
    let body = request_body("fixture", &messages, &[], Map::new()).unwrap();
    assert_eq!(
        body,
        json!({"model": "fixture", "stream": true, "store": false,
        "include": ["reasoning.encrypted_content"], "input": [
            {"role": "system", "content": "system"},
            {"role": "user", "content": [{"type": "input_text", "text": "look"},
                {"type": "input_image", "image_url": "data:image/png;base64,YQ=="}]},
            native[0], native[1], {"type": "function_call_output", "call_id": "call_1", "output": "contents"}]})
    );
    assert!(request_body("fixture", &messages[3..], &[], Map::new()).is_err());
    let mut corrupt = messages.clone();
    corrupt[2].tool_calls[0].id = "wrong-call".into();
    assert!(request_body("fixture", &corrupt, &[], Map::new()).is_err());
}

#[test]
fn generation_tool_schema_and_local_state_authority_match_the_native_protocol() {
    let parameters = json!({"max_tokens": 100, "max_output_tokens": 200, "temperature": null,
        "top_p": 0.5, "include": ["message.output_text.logprobs"],
        "tool_choice": {"type": "function", "function": {"name": "read_file"}}});
    let body = request_body(
        "fixture",
        &[StoredMessage::text("user", "read")],
        &[
            json!({"type": "function", "function": {"name": "read_file", "parameters": {
            "type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}}),
        ],
        parameters.as_object().unwrap().clone(),
    )
    .unwrap();
    assert_eq!(
        body,
        json!({"model": "fixture", "stream": true, "store": false,
        "input": [{"role": "user", "content": "read"}], "max_output_tokens": 200, "top_p": 0.5,
        "include": ["message.output_text.logprobs", "reasoning.encrypted_content"],
        "tool_choice": {"type": "function", "name": "read_file"}, "tools": [{"type": "function",
            "name": "read_file", "strict": false, "parameters": {"type": "object",
                "properties": {"path": {"type": "string"}}, "required": ["path"]}}]})
    );
    for key in [
        "input",
        "instructions",
        "previous_response_id",
        "conversation",
        "store",
        "background",
        "truncation",
    ] {
        for wrapped in [false, true] {
            let values = if wrapped {
                json!({"extra_body": {key: true}})
            } else {
                json!({key: true})
            };
            let options = crate::ModelRequestOptions {
                protocol: crate::ModelProtocol::OpenAIResponses,
                generate_kwargs: values.as_object().unwrap().clone(),
                ..Default::default()
            };
            assert!(options.validate().is_err(), "{key}");
        }
    }
}

#[test]
fn completed_response_emits_calls_once_preserves_native_items_and_counts_reasoning_tokens() {
    let output = vec![
        json!({"type": "reasoning", "id": "rs_1", "summary": [], "encrypted_content": "private-opaque"}),
        native_call(),
    ];
    let mut decoder = Decoder::default();
    assert_eq!(
        decoder
            .parse(
                &json!({"type": "response.output_item.done", "output_index": 1, "item": output[1]})
                    .to_string()
            )
            .unwrap(),
        (vec![], false)
    );
    assert_eq!(
        decoder.parse(&completed(&output).to_string()).unwrap(),
        (
            vec![
                ModelEvent::ToolCallDelta {
                    index: 0,
                    id: Some("call_1".into()),
                    name: Some("read_file".into()),
                    arguments: Some("{\"path\":\"fixture.txt\"}".into())
                },
                ModelEvent::ProviderContent {
                    protocol: PROTOCOL,
                    content: output
                },
                ModelEvent::Usage(ModelUsage {
                    prompt_tokens: 10,
                    completion_tokens: 8,
                    cache_read_tokens: 4,
                    cache_write_tokens: 0,
                    cache_eligible_input_tokens: 10,
                    cache_observed: true
                }),
            ],
            true
        )
    );
}

#[test]
fn failures_malformed_output_and_mixed_response_ids_never_release_tools() {
    for kind in [
        "error",
        "response.failed",
        "response.incomplete",
        "response.cancelled",
    ] {
        let mut decoder = Decoder::default();
        assert!(decoder.parse(&json!({"type": "response.output_item.done", "output_index": 0, "item": native_call()}).to_string()).unwrap().0.is_empty());
        let error = decoder
            .parse(&json!({"type": kind, "error": {"message": "private-key"}}).to_string())
            .unwrap_err();
        assert!(!error.to_string().contains("private-key"));
    }
    let mut invalid_outputs = vec![
        vec![native_call(), native_call()],
        vec![json!({"type": "web_search_call"})],
    ];
    for (key, value) in [
        ("arguments", json!("[]")),
        ("call_id", json!("")),
        ("status", json!("incomplete")),
    ] {
        let mut call = native_call();
        call[key] = value;
        invalid_outputs.push(vec![call]);
    }
    for output in invalid_outputs {
        assert!(
            Decoder::default()
                .parse(&completed(&output).to_string())
                .is_err()
        );
    }
    let mut decoder = Decoder::default();
    decoder
        .parse(&json!({"type": "response.created", "response": {"id": "other"}}).to_string())
        .unwrap();
    assert!(decoder.parse(&completed(&[]).to_string()).is_err());
    let mut decoder = Decoder::default();
    decoder
        .parse(&json!({"type": "response.output_text.delta", "delta": "different"}).to_string())
        .unwrap();
    assert!(
        decoder
            .parse(&completed(&[message("final")]).to_string())
            .is_err()
    );
    let mut event = completed(&[native_call()]);
    event["response"]["usage"]["input_tokens"] = json!(-1);
    assert!(Decoder::default().parse(&event.to_string()).is_err());
    assert!(
        Decoder::default()
            .parse(&"x".repeat(MAX_STREAM_BYTES + 1))
            .is_err()
    );
}

#[tokio::test]
async fn shared_sse_handles_split_unicode_refusal_truncation_and_idle_timeout() {
    let output = vec![message("中文")];
    let payload = format!(
        "data: {}\r\n\r\ndata: {}\n\n",
        json!({"type": "response.output_text.delta", "delta": "中"}),
        completed(&output)
    );
    let source = futures_util::stream::iter(
        payload
            .into_bytes()
            .into_iter()
            .map(|byte| Ok(bytes::Bytes::from(vec![byte]))),
    );
    let events = crate::model::model_event_stream(
        Box::pin(source),
        std::time::Duration::from_millis(100),
        crate::ModelProtocol::OpenAIResponses,
        None,
    )
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
    assert_eq!(
        events[0..3],
        [
            ModelEvent::TextDelta("中".into()),
            ModelEvent::TextDelta("文".into()),
            ModelEvent::ProviderContent {
                protocol: PROTOCOL,
                content: output
            }
        ]
    );
    for payload in [
        "data: [DONE]\n\n".to_owned(),
        format!(
            "data: {}\n\n",
            json!({"type": "response.output_item.done", "output_index": 0, "item": native_call()})
        ),
    ] {
        let source = futures_util::stream::iter([Ok(bytes::Bytes::from(payload))]);
        let events = crate::model::model_event_stream(
            Box::pin(source),
            std::time::Duration::from_millis(100),
            crate::ModelProtocol::OpenAIResponses,
            None,
        )
        .collect::<Vec<_>>()
        .await;
        assert_eq!(events.len(), 1);
        assert!(events[0].is_err());
    }
    let refusal = json!({"type": "message", "id": "msg_1", "role": "assistant", "status": "completed",
        "content": [{"type": "refusal", "refusal": "Cannot help"}]});
    let events = Decoder::default()
        .parse(&completed(&[refusal]).to_string())
        .unwrap()
        .0;
    assert_eq!(events[0], ModelEvent::TextDelta("Cannot help".into()));
    let mut stream = crate::model::model_event_stream(
        Box::pin(futures_util::stream::pending()),
        std::time::Duration::from_millis(100),
        crate::ModelProtocol::OpenAIResponses,
        None,
    );
    assert!(matches!(
        stream.next().await,
        Some(Err(ModelError::StreamIdleTimeout))
    ));
    assert!(stream.next().await.is_none());
}
