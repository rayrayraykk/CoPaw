use pretty_assertions::assert_eq;
use qwenpaw_protocol::Item;
use qwenpaw_protocol::ThreadStatus;
use qwenpaw_protocol::TurnStatus;

use super::*;

#[test]
fn persists_and_reopens_complete_thread_snapshots() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let path = directory.path().join("threads.sqlite3");
    let snapshot = StoredThread {
        thread: Thread {
            id: String::from("thread-1"),
            model: String::from("qwen-test"),
            workspace_root: Some(String::from("/workspace")),
            status: ThreadStatus::Idle,
            archived: false,
            created_at: 10,
            updated_at: 20,
        },
        turns: vec![Turn {
            id: String::from("turn-1"),
            thread_id: String::from("thread-1"),
            status: TurnStatus::Completed,
            items: vec![Item::AgentMessage {
                id: String::from("item-1"),
                text: String::from("hello"),
            }],
            error: None,
        }],
        messages: vec![StoredMessage::text("assistant", "hello")],
    };
    ThreadStore::open(&path)
        .expect("store should open")
        .upsert(&snapshot)
        .expect("snapshot should persist");

    let reopened = ThreadStore::open(&path).expect("store should reopen");

    assert_eq!(
        reopened.load_all().expect("snapshot should load"),
        vec![snapshot]
    );
}

#[test]
fn reads_snapshots_written_before_the_archived_field_existed() {
    let legacy = serde_json::json!({
        "thread": {
            "id": "thread-legacy",
            "model": "qwen-test",
            "workspaceRoot": "/workspace",
            "status": "idle",
            "createdAt": 10,
            "updatedAt": 20
        },
        "turns": [],
        "messages": []
    });

    let snapshot: StoredThread =
        serde_json::from_value(legacy).expect("legacy snapshot should deserialize");

    assert!(!snapshot.thread.archived);
}

#[test]
fn persists_non_secret_core_settings_atomically() {
    let store = ThreadStore::in_memory().expect("store should open");

    store
        .write_settings(&[
            ("base_url", "https://example.test/v1"),
            ("default_model", "qwen-test"),
        ])
        .expect("settings should persist");
    store
        .write_settings(&[("default_model", "qwen-next")])
        .expect("setting should update");

    assert_eq!(
        store.read_setting("base_url").expect("setting should read"),
        Some(String::from("https://example.test/v1"))
    );
    assert_eq!(
        store
            .read_setting("default_model")
            .expect("setting should read"),
        Some(String::from("qwen-next"))
    );
    assert_eq!(
        store.read_setting("api_key").expect("setting should read"),
        None
    );
}

#[test]
fn deletes_only_the_requested_thread_snapshot() {
    let store = ThreadStore::in_memory().expect("store should open");
    for id in ["thread-1", "thread-2"] {
        store
            .upsert(&StoredThread {
                thread: Thread {
                    id: String::from(id),
                    model: String::from("qwen-test"),
                    workspace_root: Some(String::from("/workspace")),
                    status: ThreadStatus::Idle,
                    archived: false,
                    created_at: 10,
                    updated_at: 20,
                },
                turns: Vec::new(),
                messages: Vec::new(),
            })
            .expect("snapshot should persist");
    }

    assert!(store.delete("thread-1").expect("snapshot should delete"));
    assert!(!store.delete("missing").expect("missing delete should work"));
    assert_eq!(
        store
            .load_all()
            .expect("remaining snapshots should load")
            .into_iter()
            .map(|snapshot| snapshot.thread.id)
            .collect::<Vec<_>>(),
        vec![String::from("thread-2")]
    );
}
