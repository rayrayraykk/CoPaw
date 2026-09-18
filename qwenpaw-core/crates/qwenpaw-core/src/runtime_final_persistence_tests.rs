//! Final-write failures remain observable without discarding visible replies.

use super::*;
use pretty_assertions::assert_eq;

async fn staged() -> (Core, tempfile::TempDir, StoredThread, StoredThread) {
    let directory = tempfile::tempdir().unwrap();
    let core = Core::persistent(
        ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1/v1"),
            default_model: String::from("final-write-fixture"),
        },
        &directory.path().join("core.sqlite"),
    )
    .unwrap();
    let thread = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    let mut state = core.inner.state.lock().await;
    let record = state.threads.get_mut(&thread.id).unwrap();
    let checkpoint = record.snapshot();
    record.checkpoint = Some(checkpoint.clone());
    record.thread.status = ThreadStatus::Active;
    record.turns.push(Turn {
        id: String::from("final-turn"),
        thread_id: thread.id,
        status: TurnStatus::InProgress,
        items: vec![Item::AgentMessage {
            id: String::from("visible-reply"),
            text: String::from("Do not discard this reply"),
        }],
        error: None,
    });
    let journal = record.snapshot();
    core.inner.store.upsert(&journal).unwrap();
    drop(state);
    (core, directory, checkpoint, journal)
}

#[tokio::test]
async fn final_persistence_failure_marks_all_outcomes_without_rewriting_the_journal() {
    for (outcome, original) in [
        (TurnOutcome::Completed, None),
        (TurnOutcome::Interrupted, None),
        (
            TurnOutcome::Failed(String::from("Original model failure")),
            Some("Original model failure"),
        ),
    ] {
        let (core, directory, checkpoint, journal) = staged().await;
        let database = rusqlite::Connection::open(directory.path().join("core.sqlite")).unwrap();
        database
            .execute_batch(
                "CREATE TRIGGER reject_final BEFORE INSERT ON threads
             WHEN json_extract(NEW.snapshot, '$.turns[#-1].status') != 'inProgress'
             BEGIN SELECT RAISE(FAIL, 'private-sqlite-detail'); END;",
            )
            .unwrap();
        let (sender, mut events) = mpsc::channel(1);
        core.finish_turn(&journal.thread.id, "final-turn", outcome, &sender)
            .await;
        let Some(CoreEvent::TurnCompleted(notification)) = events.recv().await else {
            panic!("expected the terminal event");
        };
        let failure = "Failed to persist the final turn; the latest state may not survive restart.";
        let mut expected_turn = journal.turns[0].clone();
        expected_turn.status = TurnStatus::Failed;
        expected_turn.error = Some(ErrorInfo {
            message: original.map_or_else(
                || failure.to_owned(),
                |message| format!("{message}\n{failure}"),
            ),
        });
        assert_eq!(notification.turn, expected_turn);
        let current = core.read_thread(&journal.thread.id).await.unwrap();
        let mut expected_thread = journal.thread.clone();
        assert!(current.thread.updated_at >= expected_thread.updated_at);
        expected_thread.updated_at = current.thread.updated_at;
        expected_thread.status = ThreadStatus::Error;
        assert_eq!(
            current,
            ThreadReadResponse {
                thread: expected_thread,
                turns: vec![expected_turn],
            }
        );
        assert_eq!(core.inner.store.load_all().unwrap(), vec![journal.clone()]);
        assert_eq!(
            core.export_thread_checkpoint(&journal.thread.id)
                .await
                .unwrap(),
            checkpoint
        );
        assert!(
            !core
                .turn_was_persisted(&journal.thread.id, "final-turn")
                .await
        );
        assert_eq!(
            core.check_final_persistence().unwrap_err(),
            CoreError::Storage(String::from(
                "one or more final turn writes failed in this Core instance"
            ))
        );
        database
            .execute_batch("DROP TRIGGER reject_final;")
            .unwrap();
        core.restore_thread_checkpoint(&journal.thread.id, checkpoint)
            .await
            .unwrap();
        assert!(core.check_final_persistence().is_err());
    }
}
