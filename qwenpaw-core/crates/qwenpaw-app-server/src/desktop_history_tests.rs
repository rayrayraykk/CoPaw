use super::*;
use pretty_assertions::assert_eq;

#[test]
fn history_preserves_turn_errors_without_replacing_or_reordering_messages() {
    let reply = json!({"id":"reply","role":"assistant","type":"message",
        "content":[{"type":"text","text":"Keep this reply"}],
        "metadata":{"timestamp":"fixture-time"}});
    for (status, error, with_reply) in [
        (TurnStatus::Completed, None, true),
        (TurnStatus::Interrupted, None, true),
        (
            TurnStatus::Failed,
            Some("Original model error\nFinal save failed"),
            true,
        ),
        (TurnStatus::Failed, Some("Final save failed"), false),
    ] {
        let turn = Turn {
            id: String::from("turn"),
            thread_id: String::from("thread"),
            status,
            error: error.map(|message| qwenpaw_protocol::ErrorInfo {
                message: message.to_owned(),
            }),
            items: if with_reply {
                vec![Item::AgentMessage {
                    id: String::from("reply"),
                    text: String::from("Keep this reply"),
                }]
            } else {
                vec![]
            },
        };
        let mut expected = if with_reply {
            vec![reply.clone()]
        } else {
            vec![]
        };
        if let Some(message) = error {
            expected.push(json!({"id":"turn_error","role":"assistant",
                "type":"error","status":"failed","content":[],"message":message,
                "metadata":{"timestamp":"fixture-time"}}));
        }
        assert_eq!(messages_from_turn(&turn, "fixture-time"), expected);
        assert_eq!(messages_from_turn(&turn, "fixture-time"), expected);
    }
}
