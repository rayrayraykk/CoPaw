//! Final persistence must precede publishing an idle admission slot.

use super::*;
use pretty_assertions::assert_eq;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn persistence_final_write_holds_admission_until_sqlite_finishes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("core.sqlite");
    let core = Core::persistent(
        ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1"),
            default_model: String::from("fixture"),
        },
        &path,
    )
    .unwrap();
    let id = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread
        .id;
    let turn_id = String::from("turn-final-write-fixture");
    // Stage only the finalization boundary; no model producer competes with
    // finish_turn. The independent SQLite writer delays this exact upsert.
    {
        let mut state = core.inner.state.lock().await;
        let record = state.threads.get_mut(&id).unwrap();
        record.checkpoint = Some(record.snapshot());
        record.thread.status = ThreadStatus::Active;
        record.turns.push(Turn {
            id: turn_id.clone(),
            thread_id: id.clone(),
            status: TurnStatus::InProgress,
            items: Vec::new(),
            error: None,
        });
        record.active_turn = Some(ActiveTurn {
            id: turn_id.clone(),
            cancellation: CancellationToken::new(),
            usage_owner: None,
        });
        core.inner.store.upsert(&record.snapshot()).unwrap();
    }
    let database = rusqlite::Connection::open(path).unwrap();
    database.execute_batch("BEGIN IMMEDIATE;").unwrap();
    let (sender, mut events) = mpsc::channel(1);
    let writer_core = core.clone();
    let writer_id = id.clone();
    let writer_turn = turn_id.clone();
    let writer = tokio::spawn(async move {
        writer_core
            .finish_turn(&writer_id, &writer_turn, TurnOutcome::Completed, &sender)
            .await;
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while core.inner.state.try_lock().is_ok() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("final writer must hold state while SQLite is blocked");
    assert!(
        tokio::time::timeout(Duration::from_millis(50), core.read_thread(&id))
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(
            Duration::from_millis(50),
            core.start_turn(TurnStartParams {
                thread_id: id.clone(),
                input: vec![UserInput::Text {
                    text: String::from("next")
                }],
            })
        )
        .await
        .is_err()
    );
    assert!(events.try_recv().is_err());
    database.execute_batch("COMMIT;").unwrap();
    tokio::time::timeout(Duration::from_secs(2), writer)
        .await
        .unwrap()
        .unwrap();
    let Some(CoreEvent::TurnCompleted(notification)) = events.recv().await else {
        panic!("expected terminal event after saving");
    };
    assert_eq!(notification.turn.status, TurnStatus::Completed);
    assert!(core.turn_was_persisted(&id, &turn_id).await);
    let saved = core.export_thread_checkpoint(&id).await.unwrap();
    assert_eq!(saved.turns, vec![notification.turn]);
    assert_eq!(
        core.backup_snapshot(1024 * 1024).unwrap().threads,
        vec![saved]
    );
}
