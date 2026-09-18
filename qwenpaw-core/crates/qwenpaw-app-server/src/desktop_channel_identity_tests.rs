use super::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn key(id: &str) -> WorkspaceDataKey {
    WorkspaceDataKey::LegacyAgent(id.to_owned())
}

#[test]
fn channel_identity_null_roundtrips_through_scoped_backup_and_restore() {
    let mut data = StoredChannelData::default();
    data.set_console(key("writer"), None);
    let serialized = encode(&data).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&serialized).unwrap(),
        json!({"version":2,"workspaces":[{"data_key":key("writer"),"console":null}]})
    );
    assert!(
        decode(Some(&serialized), None)
            .unwrap()
            .console(&key("writer"))
            .is_none()
    );
    assert!(
        decode(Some(&serialized), None)
            .unwrap()
            .console(&key("other"))
            .is_some()
    );
    let source = BTreeMap::from([(String::from("writer"), key("writer"))]);
    let current = BTreeMap::from([(String::from("writer"), key("old"))]);
    let target = BTreeMap::from([(String::from("writer"), key("restored"))]);
    let selected = BTreeSet::from(["writer"]);
    let filtered = filter_backup_data(&serialized, &selected, &source).unwrap();
    let local = encoded(&[("old", "replace"), ("other", "keep")]);
    let restored = merge_restore_data(
        Some(&local),
        Some(&filtered),
        &selected,
        &RestoreBindings {
            source: &source,
            current: &current,
            target: &target,
        },
    )
    .unwrap();
    let mut expected = decode(Some(&encoded(&[("other", "keep")])), None).unwrap();
    expected.set_console(key("restored"), None);
    assert_eq!(restored, encode(&expected).unwrap());
    for invalid in [
        json!({"version":2,"workspaces":[{"data_key":key("writer")}]}),
        json!({"version":2,"workspaces":[{"data_key":key("writer"),"console":[]}]}),
        json!({"version":1,"console":[]}),
    ] {
        assert_eq!(
            decode(Some(&invalid.to_string()), None).unwrap_err(),
            INVALID
        );
    }
}

fn encoded(entries: &[(&str, &str)]) -> String {
    let mut data = StoredChannelData::default();
    for (id, prefix) in entries {
        data.set_console(
            key(id),
            Some(ConsoleChannelConfig {
                bot_prefix: (*prefix).to_owned(),
                ..ConsoleChannelConfig::default()
            }),
        );
    }
    encode(&data).unwrap()
}

#[test]
fn channel_identity_rejects_unsupported_duplicate_invalid_and_oversized_data() {
    let valid: Value = serde_json::from_str(&encoded(&[("writer", "valid")])).unwrap();
    let mut duplicate = valid.clone();
    duplicate["workspaces"]
        .as_array_mut()
        .unwrap()
        .push(valid["workspaces"][0].clone());
    let mut invalid_key = valid.clone();
    invalid_key["workspaces"][0]["data_key"]["id"] = json!("../invalid");
    let mut invalid_config = valid.clone();
    invalid_config["workspaces"][0]["console"]["bot_prefix"] = json!("x".repeat(4097));
    for value in [
        duplicate,
        invalid_key,
        invalid_config,
        json!({"version":3,"workspaces":[]}),
        json!({"version":2,"workspaces":[],"extra":true}),
    ] {
        assert_eq!(decode(Some(&value.to_string()), None).unwrap_err(), INVALID);
    }
    assert_eq!(
        decode(Some(&" ".repeat(MAX_DATA_BYTES + 1)), None).unwrap_err(),
        "Stored channel configuration is too large"
    );
    let mut excess = StoredChannelData::default();
    for i in 0..=MAX_WORKSPACES {
        excess.set_console(
            key(&format!("agent-{i}")),
            Some(ConsoleChannelConfig::default()),
        );
    }
    assert_eq!(encode(&excess).unwrap_err(), INVALID);
}

#[test]
fn channel_identity_backup_filters_and_restore_remaps_without_touching_unselected_data() {
    let source = BTreeMap::from([
        ("writer".into(), key("remote")),
        ("default".into(), key("remote-default")),
    ]);
    let current = BTreeMap::from([
        ("writer".into(), key("local")),
        ("default".into(), key("local-default")),
    ]);
    let target = BTreeMap::from([("writer".into(), key("restored"))]);
    let selected = BTreeSet::from(["writer"]);
    let archived = encoded(&[("remote", "source"), ("remote-default", "not selected")]);
    let filtered = filter_backup_data(&archived, &selected, &source).unwrap();
    assert_eq!(filtered, encoded(&[("remote", "source")]));
    let local = encoded(&[
        ("local", "old"),
        ("local-default", "preserved"),
        ("orphan", "retained"),
    ]);
    let bindings = RestoreBindings {
        current: &current,
        source: &source,
        target: &target,
    };
    assert_eq!(
        merge_restore_data(Some(&local), Some(&filtered), &selected, &bindings).unwrap(),
        encoded(&[
            ("restored", "source"),
            ("local-default", "preserved"),
            ("orphan", "retained")
        ])
    );
    assert_eq!(
        merge_restore_data(Some(&local), None, &selected, &bindings).unwrap(),
        encoded(&[("local-default", "preserved"), ("orphan", "retained")])
    );
    assert_eq!(
        merge_restore_data(Some(&local), Some(&archived), &BTreeSet::new(), &bindings).unwrap(),
        local
    );
    let conflicting_target = BTreeMap::from([("writer".into(), key("orphan"))]);
    let bindings = RestoreBindings {
        target: &conflicting_target,
        ..bindings
    };
    assert_eq!(
        merge_restore_data(Some(&local), Some(&filtered), &selected, &bindings).unwrap_err(),
        "Restored channel Workspace conflicts with unselected data"
    );
}

#[test]
fn channel_identity_legacy_default_does_not_belong_to_a_selected_nondefault_agent() {
    let old = json!({"version":1,"console":{"bot_prefix":"legacy"}}).to_string();
    let bindings = BTreeMap::from([
        ("writer".into(), key("writer")),
        ("default".into(), key("bound-default")),
    ]);
    assert_eq!(
        filter_backup_data(&old, &BTreeSet::from(["writer"]), &bindings).unwrap(),
        encoded(&[])
    );
    assert_eq!(
        filter_backup_data(&old, &BTreeSet::from(["default"]), &bindings).unwrap(),
        encoded(&[("bound-default", "legacy")])
    );
    let unknown = decode(Some(&old), None).unwrap();
    assert_eq!(unknown.workspaces[0].data_key, key("default"));
}
