use super::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn key(id: u128) -> WorkspaceDataKey {
    WorkspaceDataKey::Workspace(uuid::Uuid::from_u128(id))
}

fn acl(label: &str) -> Value {
    json!({"whitelist":{"alice@example.com":{"remark":label,"display_name":"Alice"}},
        "blacklist":{},"pending":[],"approved_replay":[]})
}

#[test]
fn mail_workspace_backup_preserves_all_inner_agent_names_and_remaps_only_workspace() {
    let source = json!({"version":2,"workspaces":[{"data_key":key(1),"agents":{"writer":acl("old name"),"editor":acl("new name")}}]});
    let current = json!({"version":2,"workspaces":[{"data_key":key(2),"agents":{"writer":acl("unselected")}},
        {"data_key":key(3),"agents":{"editor":acl("replaced")}}]});
    let local = Bindings::from([
        (String::from("writer"), key(2)),
        (String::from("editor"), key(3)),
    ]);
    let origin = Bindings::from([(String::from("editor"), key(1))]);
    let selected = BTreeSet::from(["editor"]);
    assert_eq!(
        serde_json::from_str::<Value>(
            &filter_backup_data(&source.to_string(), &selected, &origin).unwrap()
        )
        .unwrap(),
        source
    );
    let restored = merge_restore_data(
        Some(&current.to_string()),
        Some(&source.to_string()),
        &selected,
        &RestoreBindings {
            current: &local,
            source: &origin,
            target: &local,
        },
    )
    .unwrap();
    let expected = json!({"version":2,"workspaces":[current["workspaces"][0],
        {"data_key":key(3),"agents":source["workspaces"][0]["agents"]}]});
    assert_eq!(serde_json::from_str::<Value>(&restored).unwrap(), expected);
    assert_eq!(
        serde_json::from_str::<Value>(
            &merge_restore_data(
                Some(&current.to_string()),
                Some(&source.to_string()),
                &BTreeSet::new(),
                &RestoreBindings {
                    current: &local,
                    source: &origin,
                    target: &local
                }
            )
            .unwrap()
        )
        .unwrap(),
        current
    );
    let collision = Bindings::from([(String::from("editor"), key(2))]);
    assert_eq!(
        merge_restore_data(
            Some(&current.to_string()),
            Some(&source.to_string()),
            &selected,
            &RestoreBindings {
                current: &local,
                source: &origin,
                target: &collision
            }
        )
        .unwrap_err(),
        "Restored mail Workspace conflicts with unselected data"
    );
}

#[test]
fn mail_legacy_namespace_requires_a_proven_binding_and_default_maps_explicitly() {
    let old = json!({"version":1,"agents":{"writer":acl("legacy"),"default":acl("default")}});
    let modern = Bindings::from([
        (String::from("writer"), key(1)),
        (String::from("default"), key(2)),
    ]);
    let empty = json!({"version":2,"workspaces":[]});
    assert_eq!(
        serde_json::from_str::<Value>(
            &filter_backup_data(&old.to_string(), &BTreeSet::from(["writer"]), &modern).unwrap()
        )
        .unwrap(),
        empty
    );
    let historical = Bindings::from([(
        String::from("writer"),
        WorkspaceDataKey::LegacyAgent(String::from("writer")),
    )]);
    let restored = merge_restore_data(
        None,
        Some(&old.to_string()),
        &BTreeSet::from(["writer"]),
        &RestoreBindings {
            current: &modern,
            source: &historical,
            target: &modern,
        },
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&restored).unwrap(),
        json!({"version":2,"workspaces":[{"data_key":key(1),"agents":{"writer":acl("legacy")}}]})
    );
    assert_eq!(
        serde_json::from_str::<Value>(
            &filter_backup_data(&old.to_string(), &BTreeSet::from(["default"]), &modern).unwrap()
        )
        .unwrap(),
        json!({"version":2,"workspaces":[{"data_key":key(2),"agents":{"default":acl("default")}}]})
    );
}

#[test]
fn mail_invalid_versions_duplicate_workspaces_and_mismatched_subjects_are_rejected() {
    let entry = json!({"data_key":key(1),"agents":{"writer":acl("valid")}});
    let valid = json!({"version":2,"workspaces":[entry]});
    let mut malformed = vec![
        json!({"version":3,"workspaces":[]}),
        json!({"version":1,"agents":{},"workspaces":[]}),
        json!({"version":2,"workspaces":[entry,entry]}),
    ];
    let mut nil = valid.clone();
    nil["workspaces"][0]["data_key"] = json!(key(0));
    malformed.push(nil);
    for field in ["pending", "approved_replay"] {
        let mut mismatch = valid.clone();
        mismatch["workspaces"][0]["agents"]["writer"][field] =
            json!([{"sender_address":"alice@example.com","agent_id":"other"}]);
        malformed.push(mismatch);
    }
    for invalid in malformed {
        assert!(
            decode(Some(&invalid.to_string()), None).is_err(),
            "{invalid}"
        );
    }
    assert_eq!(
        serde_json::from_str::<Value>(
            &encode(&decode(Some(&valid.to_string()), None).unwrap()).unwrap()
        )
        .unwrap(),
        valid
    );
}
