use super::*;
use crate::{StoredModelCall, StoredThread, ThreadStore};
use pretty_assertions::assert_eq;
use qwenpaw_protocol::{Thread, ThreadStatus};
use std::collections::BTreeMap;

fn record() -> StoredUsageRecord {
    StoredUsageRecord {
        id: String::from("usage-fixture"),
        thread_id: String::from("thread-fixture"),
        turn_id: String::from("turn-fixture"),
        agent_id: String::from("writer"),
        data_key: Some(WorkspaceDataKey::Workspace(Uuid::from_u128(1))),
        recorded_at: 1,
        call: StoredModelCall {
            provider_id: String::from("fixture"),
            model: String::from("model"),
            prompt_tokens: 12,
            completion_tokens: 3,
            cache_read_tokens: 4,
            cache_write_tokens: 0,
            cache_eligible_input_tokens: 12,
            cache_observed: true,
            usage_observed: true,
        },
    }
}

fn snapshot(record: StoredUsageRecord) -> StoreBackup {
    StoreBackup {
        version: 2,
        settings: BTreeMap::from([(String::from("label"), String::from("original"))]),
        usage: vec![record],
        threads: vec![StoredThread {
            thread: Thread {
                id: String::from("thread-fixture"),
                model: String::from("model"),
                workspace_root: None,
                status: ThreadStatus::Idle,
                archived: false,
                created_at: 1,
                updated_at: 1,
            },
            turns: Vec::new(),
            messages: Vec::new(),
            turn_metadata: Vec::new(),
        }],
    }
}

#[test]
fn versioned_usage_roundtrips_and_old_rows_cannot_smuggle_workspace_keys() {
    let current = record();
    let encoded = encode(&current).unwrap();
    assert_eq!(decode(&encoded).unwrap(), current);
    assert!(serde_json::from_str::<StoredUsageRecord>(&encoded).is_err());
    assert!(decode(&serde_json::to_string(&current).unwrap()).is_err());
    let mut legacy = current.clone();
    legacy.data_key = None;
    assert_eq!(
        decode(&serde_json::to_string(&legacy).unwrap()).unwrap(),
        legacy
    );
    let mut invalid = serde_json::from_str::<serde_json::Value>(&encoded).unwrap();
    invalid["version"] = serde_json::json!(3);
    assert!(decode(&invalid.to_string()).is_err());
    let store = ThreadStore::in_memory().unwrap();
    let expected = snapshot(current);
    store.replace_from_backup(&expected).unwrap();
    assert_eq!(store.backup_snapshot(1024 * 1024).unwrap(), expected);
    let mut old = snapshot(legacy);
    old.version = 1;
    store.replace_from_backup(&old).unwrap();
    old.version = 2;
    assert_eq!(store.backup_snapshot(1024 * 1024).unwrap(), old);
}

#[test]
fn invalid_usage_versions_and_bindings_never_replace_existing_tables() {
    let store = ThreadStore::in_memory().unwrap();
    let expected = snapshot(record());
    store.replace_from_backup(&expected).unwrap();
    let mut invalid = Vec::new();
    let mut mixed = expected.clone();
    mixed.version = 1;
    invalid.push(mixed);
    let mut unsupported = expected.clone();
    unsupported.version = 3;
    invalid.push(unsupported);
    for key in [
        WorkspaceDataKey::Workspace(Uuid::nil()),
        WorkspaceDataKey::LegacyAgent(String::from("../writer")),
    ] {
        let mut bad = expected.clone();
        bad.usage[0].data_key = Some(key);
        invalid.push(bad);
    }
    let mut duplicate = expected.clone();
    duplicate.settings.clear();
    duplicate.usage.push(duplicate.usage[0].clone());
    invalid.push(duplicate);
    for bad in invalid {
        assert!(store.replace_from_backup(&bad).is_err());
        assert_eq!(store.backup_snapshot(1024 * 1024).unwrap(), expected);
    }
}

#[test]
fn duplicate_usage_rolls_back_the_thread_and_survives_thread_deletion() {
    let store = ThreadStore::in_memory().unwrap();
    let expected = snapshot(record());
    store.replace_from_backup(&expected).unwrap();
    let mut thread = expected.threads[0].clone();
    thread.thread.model = String::from("must-rollback");
    assert!(
        store
            .upsert_with_usage(&thread, &expected.usage[0])
            .is_err()
    );
    assert_eq!(store.backup_snapshot(1024 * 1024).unwrap(), expected);
    assert!(store.delete(&thread.thread.id).unwrap());
    assert_eq!(store.load_usage().unwrap(), expected.usage);
    assert_eq!(store.load_all().unwrap(), Vec::<StoredThread>::new());
}
