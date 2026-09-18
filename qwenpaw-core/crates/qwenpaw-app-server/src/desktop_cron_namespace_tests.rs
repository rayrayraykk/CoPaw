use super::*;
use pretty_assertions::assert_eq;

fn namespaced() -> Value {
    let mut data = value(&snapshot("namespace", &["default", "writer"]));
    data["version"] = json!(3);
    data["public_ids"] = json!({"default-job":"shared", "writer-job":"shared"});
    data
}

#[test]
fn scoped_public_ids_preserve_both_namespaces_and_older_native_formats() {
    let current = namespaced();
    let parsed = parse_data(&current.to_string()).expect("different Agents may share a public ID");
    assert_eq!(value(&encode(&parsed).unwrap()), current);
    for agents in [vec!["default"], vec!["default", "writer"]] {
        let old = snapshot("old", &agents);
        assert_eq!(
            value(&encode(&parse_data(&old).unwrap()).unwrap()),
            value(&old)
        );
    }
}

#[test]
fn scoped_public_ids_validate_version_reference_and_per_agent_uniqueness() {
    for invalid in ["old-version", "orphan", "empty", "control", "duplicate"] {
        let mut data = namespaced();
        match invalid {
            "old-version" => data["version"] = json!(2),
            "orphan" => data["public_ids"]["missing"] = json!("shared"),
            "empty" => data["public_ids"]["writer-job"] = json!(""),
            "control" => data["public_ids"]["writer-job"] = json!("bad\0id"),
            _ => data["owners"]["default-job"] = json!("writer"),
        }
        assert!(parse_data(&data.to_string()).is_err(), "{invalid}");
    }
}

#[test]
fn scoped_backup_retains_only_selected_public_ids_and_their_records() {
    let original = namespaced();
    for agent in ["default", "writer"] {
        let mut expected = value(&snapshot("namespace", &[agent]));
        expected["version"] = json!(3);
        expected["public_ids"] = json!({format!("{agent}-job"):"shared"});
        assert_eq!(
            value(
                &filter_backup_data(&original.to_string(), &BTreeSet::from([agent]))
                    .unwrap()
                    .unwrap()
            ),
            expected
        );
    }
}

#[test]
fn restore_rekeys_colliding_storage_keys_without_changing_public_identity_or_unselected_data() {
    let local = snapshot("local", &["other"]);
    let mut incoming = value(&snapshot("incoming", &["writer"]));
    incoming["jobs"][0]["id"] = json!("other-job");
    incoming["owners"] = json!({"other-job":"writer"});
    for field in ["states", "history", "active_triggers"] {
        incoming[field]["other-job"] = incoming[field]
            .as_object_mut()
            .unwrap()
            .remove("writer-job")
            .unwrap();
    }
    incoming["scheduled"] = json!(["other-job"]);
    incoming["active_runs"]["writer-run"]["job_id"] = json!("other-job");
    let before = (local.clone(), incoming.clone());
    let merged = value(
        &merge_restore_data(
            Some(&local),
            Some(&incoming.to_string()),
            &BTreeSet::from(["writer"]),
        )
        .unwrap()
        .unwrap(),
    );
    let new_key = merged["owners"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, agent)| **agent == "writer")
        .unwrap()
        .0;
    assert_ne!(new_key, "other-job");
    let mut expected = value(&local);
    expected["version"] = json!(3);
    expected["public_ids"] = json!({new_key:"other-job"});
    let mut job = incoming["jobs"][0].clone();
    job["id"] = json!(new_key);
    expected["jobs"].as_array_mut().unwrap().push(job);
    expected["owners"][new_key] = json!("writer");
    for field in ["states", "history", "active_triggers"] {
        expected[field][new_key] = incoming[field]["other-job"].clone();
    }
    let mut scheduled = vec!["other-job", new_key.as_str()];
    scheduled.sort_unstable();
    expected["scheduled"] = json!(scheduled);
    expected["active_runs"]["writer-run"] = json!({"job_id":new_key,"trigger":"incoming"});
    assert_eq!(merged, expected);
    assert_eq!((local, incoming), before);
}
