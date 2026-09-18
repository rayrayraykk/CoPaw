use pretty_assertions::assert_eq;
use qwenpaw_protocol::Item;
use qwenpaw_protocol::ThreadStatus;
use qwenpaw_protocol::TurnStatus;

use super::*;

#[test]
fn backup_captures_complete_durable_tables_and_enforces_size_bound() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("backup.sqlite3");
    let store = ThreadStore::open(&path).unwrap();
    let thread = StoredThread {
        thread: Thread {
            id: String::from("backup-thread"),
            model: String::from("model"),
            workspace_root: Some(String::from("/fixture")),
            status: ThreadStatus::Idle,
            archived: false,
            created_at: 1,
            updated_at: 2,
        },
        turns: Vec::new(),
        messages: vec![StoredMessage::text("user", "保留 Unicode 内容")],
        turn_metadata: Vec::new(),
    };
    let usage = StoredUsageRecord {
        id: String::from("usage-1"),
        thread_id: String::from("backup-thread"),
        turn_id: String::from("turn-1"),
        agent_id: String::from("default"),
        data_key: None,
        recorded_at: 2,
        call: StoredModelCall {
            provider_id: String::from("provider"),
            model: String::from("model"),
            prompt_tokens: 12,
            completion_tokens: 4,
            cache_read_tokens: 2,
            cache_write_tokens: 0,
            cache_eligible_input_tokens: 12,
            cache_observed: true,
            usage_observed: true,
        },
    };
    store.upsert_with_usage(&thread, &usage).unwrap();
    store
        .write_settings(&[("ui_language", "zh"), ("default_model", "model")])
        .unwrap();
    let expected = StoreBackup {
        version: 2,
        settings: BTreeMap::from([
            (String::from("ui_language"), String::from("zh")),
            (String::from("default_model"), String::from("model")),
        ]),
        threads: vec![thread],
        usage: vec![usage],
    };
    assert_eq!(store.backup_snapshot(1024 * 1024).unwrap(), expected);
    assert!(matches!(
        store.backup_snapshot(1),
        Err(StorageError::BackupTooLarge)
    ));
    drop(store);
    assert_eq!(
        ThreadStore::open(&path)
            .unwrap()
            .backup_snapshot(1024 * 1024)
            .unwrap(),
        expected
    );
    let restored_path = directory.path().join("restored.sqlite3");
    let restored = ThreadStore::open(&restored_path).unwrap();
    restored
        .write_settings(&[("obsolete", "remove on full restore")])
        .unwrap();
    restored.replace_from_backup(&expected).unwrap();
    assert_eq!(restored.backup_snapshot(1024 * 1024).unwrap(), expected);
    // Failure after settings and threads have been replaced must restore all tables.
    let mut duplicate_usage = expected.clone();
    duplicate_usage
        .settings
        .insert(String::from("ui_language"), String::from("en"));
    duplicate_usage.threads[0].messages.clear();
    duplicate_usage.usage.push(duplicate_usage.usage[0].clone());
    assert!(restored.replace_from_backup(&duplicate_usage).is_err());
    assert_eq!(restored.backup_snapshot(1024 * 1024).unwrap(), expected);
    let mut unsupported = expected.clone();
    unsupported.version = 3;
    assert!(matches!(
        restored.replace_from_backup(&unsupported),
        Err(StorageError::UnsupportedBackupVersion)
    ));
    drop(restored);
    assert_eq!(
        ThreadStore::open(&restored_path)
            .unwrap()
            .backup_snapshot(1024 * 1024)
            .unwrap(),
        expected
    );
}

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
        turn_metadata: vec![StoredTurnMetadata {
            turn_id: String::from("turn-1"),
            started_at: 10,
            completed_at: Some(20),
            model_calls: vec![StoredModelCall {
                provider_id: String::from("openai-compatible"),
                model: String::from("qwen-test"),
                prompt_tokens: 12,
                completion_tokens: 3,
                cache_read_tokens: 4,
                cache_write_tokens: 0,
                cache_eligible_input_tokens: 12,
                cache_observed: true,
                usage_observed: true,
            }],
        }],
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
    assert!(snapshot.turn_metadata.is_empty());
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
                turn_metadata: Vec::new(),
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

#[test]
fn keeps_usage_records_when_the_source_thread_is_deleted() {
    let store = ThreadStore::in_memory().expect("store should open");
    let snapshot = StoredThread {
        thread: Thread {
            id: String::from("thread-usage"),
            model: String::from("qwen-test"),
            workspace_root: Some(String::from("/workspace")),
            status: ThreadStatus::Idle,
            archived: false,
            created_at: 10,
            updated_at: 20,
        },
        turns: Vec::new(),
        messages: Vec::new(),
        turn_metadata: Vec::new(),
    };
    let usage = StoredUsageRecord {
        id: String::from("usage-1"),
        thread_id: snapshot.thread.id.clone(),
        turn_id: String::from("turn-1"),
        agent_id: String::from("default"),
        data_key: None,
        recorded_at: 20,
        call: StoredModelCall {
            provider_id: String::from("openai-compatible"),
            model: String::from("qwen-test"),
            prompt_tokens: 12,
            completion_tokens: 3,
            cache_read_tokens: 4,
            cache_write_tokens: 0,
            cache_eligible_input_tokens: 12,
            cache_observed: true,
            usage_observed: true,
        },
    };
    store
        .upsert_with_usage(&snapshot, &usage)
        .expect("snapshot and usage should persist");
    assert!(
        store
            .delete(&snapshot.thread.id)
            .expect("Thread should delete")
    );

    assert_eq!(store.load_all().expect("Threads should load"), Vec::new());
    assert_eq!(store.load_usage().expect("usage should load"), vec![usage]);
}
