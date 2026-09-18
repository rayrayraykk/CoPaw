//! Failed SQLite writes must not publish admission or a saved checkpoint.

use super::*;
use pretty_assertions::assert_eq;

async fn durable() -> (Core, tempfile::TempDir, rusqlite::Connection) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("core.sqlite");
    let base_url = start_tool_model_server(Arc::new(Mutex::new(Vec::new())), "read_file").await;
    let core = Core::persistent(
        ModelConfig {
            api_key: None,
            base_url,
            default_model: String::from("fixture"),
        },
        &path,
    )
    .unwrap();
    std::fs::write(directory.path().join("notes.txt"), "saved fixture").unwrap();
    let database = rusqlite::Connection::open(path).unwrap();
    (core, directory, database)
}

fn reject(database: &rusqlite::Connection, status: &str) {
    assert!(matches!(status, "inProgress" | "completed"));
    database
        .execute_batch(&format!(
            "CREATE TRIGGER reject_turn BEFORE INSERT ON threads
         WHEN json_extract(NEW.snapshot, '$.turns[#-1].status') = '{status}'
         BEGIN SELECT RAISE(FAIL, 'fixture turn write failure'); END;"
        ))
        .unwrap();
}

#[tokio::test]
async fn persistence_failed_admission_preserves_the_complete_previous_state() {
    let (core, directory, database) = durable().await;
    let id = thread(&core, &directory).await;
    let before = core.read_thread(&id).await.unwrap();
    core.check_final_persistence().unwrap();
    let saved = core.backup_snapshot(1024 * 1024).unwrap();
    reject(&database, "inProgress");
    assert!(
        core.start_turn_with_model(
            input(&id),
            Some((
                ModelConfig {
                    api_key: None,
                    base_url: String::from("http://127.0.0.1:1"),
                    default_model: String::from("must-not-be-published"),
                },
                ModelRequestOptions::default()
            ))
        )
        .await
        .is_err()
    );
    assert_eq!(core.read_thread(&id).await.unwrap(), before);
    assert_eq!(
        core.export_thread_checkpoint(&id).await.unwrap(),
        saved.threads[0]
    );
    assert_eq!(core.backup_snapshot(1024 * 1024).unwrap(), saved);
    core.check_final_persistence().unwrap();
    database.execute_batch("DROP TRIGGER reject_turn;").unwrap();
    let (_, mut events) = core.start_turn(input(&id)).await.unwrap();
    assert_eq!(finish(&mut events).await.status, TurnStatus::Completed);
}

#[tokio::test]
async fn persistence_failed_completion_keeps_the_previous_checkpoint_boundary() {
    let (core, directory, database) = durable().await;
    let id = thread(&core, &directory).await;
    let before = core.export_thread_checkpoint(&id).await.unwrap();
    reject(&database, "completed");
    let (started, mut events) = core.start_turn(input(&id)).await.unwrap();
    assert!(!core.turn_was_persisted(&id, &started.turn.id).await);
    let completed = finish(&mut events).await;
    assert_eq!(completed.status, TurnStatus::Failed);
    assert_eq!(
        completed.error,
        Some(qwenpaw_protocol::ErrorInfo {
            message: String::from(
                "Failed to persist the final turn; the latest state may not survive restart."
            )
        })
    );
    let failed_id = completed.id.clone();
    let failure = core.check_final_persistence().unwrap_err().to_string();
    assert_eq!(
        failure,
        "thread storage failed: one or more final turn writes failed in this Core instance"
    );
    assert!(!core.turn_was_persisted(&id, &failed_id).await);
    assert_eq!(core.read_thread(&id).await.unwrap().turns, vec![completed]);
    assert_eq!(core.export_thread_checkpoint(&id).await.unwrap(), before);
    database.execute_batch("DROP TRIGGER reject_turn;").unwrap();
    let (_, mut events) = core.start_turn(input(&id)).await.unwrap();
    assert_eq!(core.export_thread_checkpoint(&id).await.unwrap(), before);
    let completed = finish(&mut events).await;
    assert_eq!(completed.status, TurnStatus::Completed);
    assert!(core.turn_was_persisted(&id, &completed.id).await);
    assert!(!core.turn_was_persisted(&id, &failed_id).await);
    assert_eq!(
        core.check_final_persistence().unwrap_err().to_string(),
        failure
    );
    assert!(!core.turn_was_persisted(&id, "missing").await);
    assert!(!core.turn_was_persisted("missing", &completed.id).await);
    let saved = core.export_thread_checkpoint(&id).await.unwrap();
    assert_eq!(saved.turns.len(), 2);
    assert_eq!(
        core.backup_snapshot(1024 * 1024).unwrap().threads,
        vec![saved]
    );
}

#[tokio::test]
async fn persistence_failed_readmission_preserves_the_unsaved_reply_and_saved_boundary() {
    let (core, directory, database) = durable().await;
    let id = thread(&core, &directory).await;
    let checkpoint = core.export_thread_checkpoint(&id).await.unwrap();
    reject(&database, "completed");
    let (_, mut events) = core.start_turn(input(&id)).await.unwrap();
    let completed = finish(&mut events).await;
    database.execute_batch("DROP TRIGGER reject_turn;").unwrap();
    let before = core.read_thread(&id).await.unwrap();
    let saved = core.backup_snapshot(1024 * 1024).unwrap();
    reject(&database, "inProgress");
    assert!(core.start_turn(input(&id)).await.is_err());
    assert_eq!(core.read_thread(&id).await.unwrap(), before);
    assert_eq!(core.backup_snapshot(1024 * 1024).unwrap(), saved);
    assert_eq!(
        core.export_thread_checkpoint(&id).await.unwrap(),
        checkpoint
    );
    assert!(!core.turn_was_persisted(&id, &completed.id).await);
    database.execute_batch("DROP TRIGGER reject_turn;").unwrap();
    let (_, mut events) = core.start_turn(input(&id)).await.unwrap();
    assert_eq!(finish(&mut events).await.status, TurnStatus::Completed);
}

#[tokio::test]
async fn persistence_restore_clears_receipts_only_after_a_successful_write() {
    let (core, directory, database) = durable().await;
    let id = thread(&core, &directory).await;
    let (_, mut events) = core.start_turn(input(&id)).await.unwrap();
    let completed = finish(&mut events).await;
    let saved = core.export_thread_checkpoint(&id).await.unwrap();
    reject(&database, "completed");
    assert!(
        core.restore_thread_checkpoint(&id, saved.clone())
            .await
            .is_err()
    );
    assert_eq!(core.export_thread_checkpoint(&id).await.unwrap(), saved);
    assert!(core.turn_was_persisted(&id, &completed.id).await);
    database.execute_batch("DROP TRIGGER reject_turn;").unwrap();
    core.restore_thread_checkpoint(&id, saved.clone())
        .await
        .unwrap();
    assert!(!core.turn_was_persisted(&id, &completed.id).await);
    assert_eq!(
        core.export_thread_checkpoint(&id).await.unwrap().messages,
        saved.messages
    );
    let (_, mut events) = core.start_turn(input(&id)).await.unwrap();
    let completed = finish(&mut events).await;
    assert!(core.turn_was_persisted(&id, &completed.id).await);
}

#[tokio::test]
async fn persistence_reopen_keeps_existing_interrupted_journal_recovery() {
    let (core, directory, database) = durable().await;
    let id = thread(&core, &directory).await;
    reject(&database, "completed");
    let (_, mut events) = core.start_turn(input(&id)).await.unwrap();
    let completed = finish(&mut events).await;
    assert!(events.recv().await.is_none());
    let mut journal = core.backup_snapshot(1024 * 1024).unwrap().threads.remove(0);
    assert_eq!(journal.turns[0].status, TurnStatus::InProgress);
    database.execute_batch("DROP TRIGGER reject_turn;").unwrap();
    drop(core);
    let reopened = Core::persistent(
        ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1"),
            default_model: String::from("fixture"),
        },
        &directory.path().join("core.sqlite"),
    )
    .unwrap();
    let recovered = reopened.export_thread_checkpoint(&id).await.unwrap();
    reopened.check_final_persistence().unwrap();
    journal.turns[0].status = TurnStatus::Interrupted;
    journal.thread.status = qwenpaw_protocol::ThreadStatus::Idle;
    journal.thread.updated_at = recovered.thread.updated_at;
    assert_eq!(recovered, journal);
    assert!(!reopened.turn_was_persisted(&id, &completed.id).await);
    assert_eq!(
        reopened.backup_snapshot(1024 * 1024).unwrap().threads,
        vec![recovered]
    );
}
