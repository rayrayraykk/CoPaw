use super::*;
use pretty_assertions::assert_eq;

fn key(value: u128) -> WorkspaceDataKey {
    WorkspaceDataKey::Workspace(uuid::Uuid::from_u128(value))
}

fn record(id: &str, actor: &str, key: Option<WorkspaceDataKey>) -> StoredUsageRecord {
    StoredUsageRecord {
        id: id.to_owned(),
        agent_id: actor.to_owned(),
        data_key: key,
        thread_id: String::from("deleted-thread"),
        turn_id: String::from("turn"),
        recorded_at: 1,
        call: qwenpaw_storage::StoredModelCall {
            provider_id: String::from("fixture"),
            model: String::from("model"),
            prompt_tokens: 10,
            completion_tokens: 3,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cache_eligible_input_tokens: 0,
            cache_observed: false,
            usage_observed: true,
        },
    }
}

fn snapshot(usage: Vec<StoredUsageRecord>) -> StoreBackup {
    StoreBackup {
        version: 2,
        usage,
        settings: BTreeMap::new(),
        threads: Vec::new(),
    }
}

#[test]
fn usage_restore_maps_keys_not_event_labels_and_protects_unselected_records() {
    let source = snapshot(vec![record("incoming", "writer", Some(key(1)))]);
    let current = snapshot(vec![
        record("replace", "editor", Some(key(3))),
        record("keep", "writer", Some(key(2))),
    ]);
    let local = Bindings::from([
        (String::from("editor"), key(3)),
        (String::from("writer"), key(2)),
    ]);
    let archived = Bindings::from([(String::from("editor"), key(1))]);
    let bindings = RestoreBindings {
        current: &local,
        source: &archived,
        target: &local,
    };
    let original = (source.clone(), current.clone());
    let selected = BTreeSet::from(["editor"]);
    let mut exported = source.clone();
    filter_backup(&mut exported, &selected, &archived).unwrap();
    assert_eq!(exported, source);
    assert_eq!(
        merge(&current, &source, &selected, &bindings).unwrap(),
        vec![
            current.usage[1].clone(),
            record("incoming", "writer", Some(key(3)))
        ]
    );
    assert_eq!(
        merge(&current, &source, &BTreeSet::new(), &bindings).unwrap(),
        current.usage
    );
    assert_eq!((source.clone(), current.clone()), original);
    let mut collision = source;
    collision.usage[0].id = String::from("keep");
    assert_eq!(
        merge(&current, &collision, &selected, &bindings).unwrap_err(),
        "Restored usage ID conflicts with existing data"
    );
}

#[test]
fn legacy_usage_needs_historical_binding_and_default_restore_gets_a_target_key() {
    let mut source = snapshot(vec![record("legacy", "writer", None)]);
    source.version = 1;
    let modern = Bindings::from([(String::from("writer"), key(1))]);
    let mut exported = source.clone();
    filter_backup(&mut exported, &BTreeSet::from(["writer"]), &modern).unwrap();
    assert_eq!(
        exported,
        StoreBackup {
            usage: Vec::new(),
            ..source.clone()
        }
    );
    let legacy = Bindings::from([(
        String::from("writer"),
        WorkspaceDataKey::LegacyAgent(String::from("writer")),
    )]);
    filter_backup(&mut source, &BTreeSet::from(["writer"]), &legacy).unwrap();
    assert_eq!(source.usage, vec![record("legacy", "writer", None)]);
    let current = snapshot(Vec::new());
    assert_eq!(
        merge(
            &current,
            &source,
            &BTreeSet::from(["writer"]),
            &RestoreBindings {
                current: &modern,
                source: &legacy,
                target: &modern
            }
        )
        .unwrap(),
        vec![record("legacy", "writer", Some(key(1)))]
    );
    source.usage[0].agent_id = String::from("default");
    let origin = Bindings::from([(String::from("default"), key(2))]);
    let target = Bindings::from([(String::from("default"), key(3))]);
    assert_eq!(
        merge(
            &current,
            &source,
            &BTreeSet::from(["default"]),
            &RestoreBindings {
                current: &target,
                source: &origin,
                target: &target
            }
        )
        .unwrap(),
        vec![record("legacy", "default", Some(key(3)))]
    );
}
