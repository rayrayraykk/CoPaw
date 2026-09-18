use super::super::{Thread, ThreadStatus, default_groups_for, metadata_for_key};
use super::*;
use pretty_assertions::assert_eq;

fn key() -> WorkspaceDataKey {
    WorkspaceDataKey::Workspace(uuid::Uuid::now_v7())
}

fn catalog(owner: &str, key: &WorkspaceDataKey, id: &str) -> ChatCatalog {
    let thread = Thread {
        id: id.to_owned(),
        model: String::from("fixture"),
        workspace_root: None,
        status: ThreadStatus::Idle,
        archived: false,
        created_at: 1,
        updated_at: 1,
    };
    ChatCatalog {
        version: 2,
        chats: BTreeMap::from([(
            id.to_owned(),
            metadata_for_key(&thread, "same-session", owner, key),
        )]),
        groups: default_groups_for(owner)
            .into_iter()
            .map(|mut group| {
                group.data_key = Some(key.clone());
                group
            })
            .collect(),
    }
}

#[test]
fn namespaces_require_complete_valid_keys_and_allow_reused_origin_labels() {
    let first = key();
    let second = key();
    let mut combined = catalog("writer", &first, "first");
    let other = catalog("writer", &second, "second");
    combined.chats.extend(other.chats);
    combined.groups.extend(other.groups);
    assert_eq!(validate_catalog(&combined), Ok(()));
    let mut missing = combined.clone();
    missing.chats.get_mut("first").unwrap().data_key = None;
    assert_eq!(
        validate_catalog(&missing),
        Err("chat catalog has an invalid Workspace binding")
    );
    let mut nil = combined.clone();
    nil.groups[0].data_key = Some(WorkspaceDataKey::Workspace(uuid::Uuid::nil()));
    assert_eq!(
        validate_catalog(&nil),
        Err("chat catalog has an invalid Workspace binding")
    );
    let mut duplicated = combined.clone();
    duplicated.groups[3].data_key = Some(first);
    duplicated.groups[3].agent_id = String::from("editor");
    assert_eq!(
        validate_catalog(&duplicated),
        Err("chat catalog contains duplicate group IDs")
    );
    combined.version = 1;
    assert_eq!(
        validate_catalog(&combined),
        Err("chat catalog has an invalid Workspace binding")
    );
}

#[test]
fn legacy_nondefault_records_need_historical_binding_not_same_public_name() {
    let mut old = catalog("writer", &key(), "writer-thread");
    old.version = 1;
    old.groups.extend(default_groups_for("default"));
    for chat in old.chats.values_mut() {
        chat.data_key = None;
    }
    for group in &mut old.groups {
        group.data_key = None;
    }
    let old = encode(&old).unwrap();
    let selected = BTreeSet::from(["writer"]);
    let ids = BTreeSet::from(["writer-thread"]);
    let new = BTreeMap::from([(String::from("writer"), key())]);
    assert_eq!(
        serde_json::from_str::<Value>(&filter_backup_data(&old, &selected, &ids, &new).unwrap())
            .unwrap(),
        json!({"version":2,"chats":{},"groups":[]})
    );
    let historic = legacy_bindings(&selected);
    let recovered = decode(
        Some(&filter_backup_data(&old, &selected, &ids, &historic).unwrap()),
        &historic,
    )
    .unwrap();
    assert_eq!(
        recovered.chats["writer-thread"].data_key.as_ref(),
        historic.get("writer")
    );
    assert_eq!(recovered.groups.len(), 3);
}

#[test]
fn restore_maps_retained_workspace_with_new_actor_and_preserves_unselected_catalog() {
    let source_key = key();
    let target_key = key();
    let other_key = key();
    let source = encode(&catalog("writer", &source_key, "restored-thread")).unwrap();
    let mut current = catalog("writer", &other_key, "keep-thread");
    let old_target = catalog("editor", &target_key, "replace-thread");
    current.chats.extend(old_target.chats);
    current.groups.extend(old_target.groups);
    let selected = BTreeSet::from(["editor"]);
    let before = encode(&current).unwrap();
    let local = BTreeMap::from([
        (String::from("editor"), target_key.clone()),
        (String::from("writer"), other_key.clone()),
    ]);
    let archived = BTreeMap::from([(String::from("editor"), source_key.clone())]);
    let result = merge_restore_data(
        Some(&before),
        Some(&source),
        &selected,
        &BTreeSet::from(["restored-thread"]),
        &local,
        &archived,
        &local,
    )
    .unwrap();
    let mut expected = catalog("writer", &other_key, "keep-thread");
    let restored = catalog("writer", &target_key, "restored-thread");
    expected.chats.extend(restored.chats);
    expected.groups.extend(restored.groups);
    assert_eq!(
        serde_json::from_str::<Value>(&result).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    let conflict = encode(&catalog("writer", &source_key, "keep-thread")).unwrap();
    assert_eq!(
        merge_restore_data(
            Some(&before),
            Some(&conflict),
            &selected,
            &BTreeSet::from(["keep-thread"]),
            &local,
            &archived,
            &local
        ),
        Err("Restored chat ID conflicts with an unselected Agent")
    );
    assert_eq!(
        merge_restore_data(
            Some(&before),
            Some(&source),
            &BTreeSet::new(),
            &BTreeSet::new(),
            &local,
            &archived,
            &local
        )
        .unwrap(),
        before
    );
}
