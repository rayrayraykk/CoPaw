use pretty_assertions::assert_eq;
use qwenpaw_storage::StoredFunctionCall;
use qwenpaw_storage::StoredToolCall;

use super::*;

#[test]
fn media_has_an_independent_budget_and_is_never_truncated() {
    let mut message = StoredMessage::text("user", "before and after");
    message.user_input = Some(qwenpaw_storage::StoredUserInput {
        item_id: String::from("item"),
        parts: vec![
            qwenpaw_storage::StoredUserPart::Text {
                text: String::from("before"),
            },
            qwenpaw_storage::StoredUserPart::Image {
                path: String::from("red.png"),
                mime_type: String::from("image/png"),
                size: 2 * 1_048_576,
                data: Some("A".repeat(3 * 1_048_576)),
            },
            qwenpaw_storage::StoredUserPart::Text {
                text: String::from("after"),
            },
        ],
    });
    let limits = ContextLimits {
        max_messages: 32,
        max_bytes: 4096,
    };
    assert_eq!(
        build_context(&[message.clone()], limits).unwrap(),
        vec![message.clone()]
    );
    assert!(
        build_context(
            &[message.clone()],
            ContextLimits {
                max_messages: 32,
                max_bytes: 1
            }
        )
        .is_err()
    );
    let mut huge = message.clone();
    for _ in 0..11 {
        huge.user_input
            .as_mut()
            .unwrap()
            .parts
            .extend(message.user_input.as_ref().unwrap().parts.clone());
    }
    assert_eq!(
        build_context(&[huge.clone()], limits).unwrap_err(),
        ContextError::MediaTooLarge
    );
    assert_eq!(
        build_context(&[huge, message.clone()], limits).unwrap(),
        vec![message]
    );
}

#[test]
fn signed_native_blocks_are_preserved_or_rejected_never_truncated() {
    let mut assistant = StoredMessage::text("assistant", "answer");
    assistant.provider_content.insert(
        String::from("anthropic-messages"),
        vec![serde_json::json!({
            "type": "thinking", "thinking": "opaque".repeat(2000), "signature": "original-signature"
        })],
    );
    let messages = vec![StoredMessage::text("user", "question"), assistant];
    assert_eq!(
        build_context(
            &messages,
            ContextLimits {
                max_messages: 32,
                max_bytes: 20_000
            }
        )
        .unwrap(),
        messages
    );
    assert!(matches!(
        build_context(
            &messages,
            ContextLimits {
                max_messages: 32,
                max_bytes: 1000
            }
        ),
        Err(ContextError::TooLarge { .. })
    ));
    assert_eq!(
        messages[1].provider_content["anthropic-messages"][0]["signature"],
        "original-signature"
    );
}

#[test]
fn retains_system_and_newest_complete_user_turns() {
    let messages = vec![
        StoredMessage::text("system", "system"),
        StoredMessage::text("user", "old question"),
        StoredMessage::text("assistant", "old answer"),
        StoredMessage::text("user", "new question"),
        StoredMessage::text("assistant", "new answer"),
    ];

    let context = build_context(
        &messages,
        ContextLimits {
            max_messages: 3,
            max_bytes: 10_000,
        },
    )
    .expect("context should fit");

    assert_eq!(
        context,
        vec![
            StoredMessage::text("system", "system"),
            StoredMessage::text("user", "new question"),
            StoredMessage::text("assistant", "new answer"),
        ]
    );
}

#[test]
fn preserves_tool_call_and_result_adjacency() {
    let messages = vec![
        StoredMessage::text("system", "system"),
        StoredMessage::text("user", "inspect"),
        StoredMessage::assistant_tool_calls(
            String::new(),
            vec![StoredToolCall {
                id: String::from("call-1"),
                kind: String::from("function"),
                function: StoredFunctionCall {
                    name: String::from("read_file"),
                    arguments: String::from("{\"path\":\"src/lib.rs\"}"),
                },
            }],
        ),
        StoredMessage::tool_result(String::from("call-1"), String::from("contents")),
        StoredMessage::text("assistant", "done"),
    ];

    assert_eq!(
        build_context(
            &messages,
            ContextLimits {
                max_messages: 32,
                max_bytes: 10_000,
            },
        )
        .expect("context should fit"),
        messages
    );
}

#[test]
fn truncates_large_content_in_the_latest_turn_within_the_byte_budget() {
    let messages = vec![
        StoredMessage::text("system", "system"),
        StoredMessage::text("user", "x".repeat(2_000)),
        StoredMessage::text("assistant", "y".repeat(2_000)),
    ];
    let limits = ContextLimits {
        max_messages: 32,
        max_bytes: 1_000,
    };

    let context = build_context(&messages, limits).expect("truncated context should fit");

    assert_eq!(context.len(), 3);
    assert!(context[1].content.ends_with(TRUNCATION_MARKER));
    assert!(context[2].content.ends_with(TRUNCATION_MARKER));
    assert!(context.iter().map(serialized_size).sum::<usize>() <= limits.max_bytes);
}

#[test]
fn rejects_irreducible_tool_metadata_over_the_byte_limit() {
    let messages = vec![
        StoredMessage::text("system", "system"),
        StoredMessage::text("user", "use tool"),
        StoredMessage::assistant_tool_calls(
            String::new(),
            vec![StoredToolCall {
                id: String::from("call-1"),
                kind: String::from("function"),
                function: StoredFunctionCall {
                    name: String::from("read_file"),
                    arguments: "x".repeat(2_000),
                },
            }],
        ),
    ];

    let error = build_context(
        &messages,
        ContextLimits {
            max_messages: 32,
            max_bytes: 500,
        },
    )
    .expect_err("irreducible metadata should fail closed");

    assert!(matches!(
        error,
        ContextError::TooLarge {
            actual_bytes,
            max_bytes: 500
        } if actual_bytes > 500
    ));
}
