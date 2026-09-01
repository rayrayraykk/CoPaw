use pretty_assertions::assert_eq;
use qwenpaw_storage::StoredFunctionCall;
use qwenpaw_storage::StoredToolCall;

use super::*;

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
