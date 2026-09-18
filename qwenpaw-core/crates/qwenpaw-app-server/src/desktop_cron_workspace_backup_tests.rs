use super::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use uuid::Uuid;

fn key(id: u128) -> WorkspaceDataKey {
    WorkspaceDataKey::Workspace(Uuid::from_u128(id))
}

fn native(agents: &[(&str, u128)]) -> String {
    let labels = agents.iter().map(|(name, _)| *name).collect::<Vec<_>>();
    let mut data = parse_data(&super::super::tests::snapshot("native", &labels)).unwrap();
    let bindings = agents
        .iter()
        .map(|(name, id)| ((*name).to_owned(), key(*id)))
        .collect();
    materialize(&mut data, &bindings);
    // Represent server-assigned ownership, not a legacy label inference.
    for (agent, namespace) in agents {
        data.workspace_owners
            .insert(format!("{agent}-job"), key(*namespace));
        data.public_ids
            .insert(format!("{agent}-job"), String::from("shared"));
        data.active_runs
            .get_mut(&format!("{agent}-run"))
            .unwrap()
            .data_key = Some(key(*namespace));
    }
    encode(&data).unwrap()
}

#[test]
fn native_namespaces_override_reused_labels_and_validate_all_job_and_claim_bindings() {
    let mut data = parse_data(&native(&[("writer", 1), ("other", 2)])).unwrap();
    data.owners
        .insert(String::from("other-job"), String::from("writer"));
    let valid: Value = serde_json::from_str(&encode(&data).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(parse_data(&valid.to_string()).unwrap()).unwrap(),
        valid
    );
    for case in [
        "same-namespace",
        "missing",
        "unknown-job",
        "nil",
        "claim-key",
        "claim-actor",
        "old-version",
    ] {
        let mut invalid = valid.clone();
        match case {
            "same-namespace" => invalid["workspace_owners"]["other-job"] = json!(key(1)),
            "missing" => {
                invalid["workspace_owners"]
                    .as_object_mut()
                    .unwrap()
                    .remove("writer-job");
            }
            "unknown-job" => invalid["workspace_owners"]["missing-job"] = json!(key(3)),
            "nil" => invalid["workspace_owners"]["writer-job"] = json!(key(0)),
            "claim-key" => invalid["active_runs"]["writer-run"]["data_key"] = json!(key(2)),
            "claim-actor" => invalid["active_runs"]["writer-run"]["agent_id"] = json!("../forged"),
            _ => invalid["version"] = json!(3),
        }
        assert!(parse_data(&invalid.to_string()).is_err(), "{case}");
    }
}

#[test]
fn backup_uses_current_binding_and_restore_remaps_jobs_claims_and_colliding_internal_keys() {
    let original = native(&[("writer", 1), ("other", 2)]);
    let source_binding = Bindings::from([(String::from("editor"), key(1))]);
    let selected = filter_for_bindings(&original, &source_binding)
        .unwrap()
        .unwrap();
    let expected = native(&[("writer", 1)]);
    assert_eq!(
        serde_json::from_str::<Value>(&selected).unwrap(),
        serde_json::from_str::<Value>(&expected).unwrap()
    );
    let mut local = parse_data(&native(&[("writer", 9)])).unwrap();
    let claim = local.active_runs.remove("writer-run").unwrap();
    local.active_runs.insert(String::from("local-run"), claim);
    let local = encode(&local).unwrap();
    let current = Bindings::from([
        (String::from("unselected"), key(9)),
        (String::from("editor"), key(3)),
    ]);
    let restored = merge_for_bindings(
        Some(&local),
        Some(&selected),
        &BTreeSet::from(["editor"]),
        &current,
        &source_binding,
        &current,
        &BTreeSet::new(),
    )
    .unwrap()
    .unwrap();
    let retained = filter_for_bindings(
        &restored,
        &Bindings::from([(String::from("unselected"), key(9))]),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&retained).unwrap(),
        serde_json::from_str::<Value>(&local).unwrap()
    );
    let data = parse_data(&restored).unwrap();
    let incoming = data
        .jobs
        .iter()
        .find(|job| owner_key(&data, job.id.as_deref().unwrap()) == key(3))
        .unwrap();
    let incoming_id = incoming.id.as_deref().unwrap();
    assert_ne!(incoming_id, "writer-job");
    assert_eq!(incoming.public_id(), Some("shared"));
    assert_eq!(
        serde_json::to_value(&data.active_runs["writer-run"]).unwrap(),
        json!({"job_id":incoming_id,"trigger":"native","agent_id":"writer","data_key":key(3)})
    );
    let archived = parse_data(&selected).unwrap();
    assert_eq!(
        serde_json::to_value(&data.states[incoming_id]).unwrap(),
        serde_json::to_value(&archived.states["writer-job"]).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&data.history[incoming_id]).unwrap(),
        serde_json::to_value(&archived.history["writer-job"]).unwrap()
    );
    assert!(data.scheduled.contains(incoming_id));
    assert_eq!(data.active_triggers[incoming_id], "manual");
}

#[test]
fn old_native_nondefault_jobs_need_historical_binding_not_a_reused_public_name() {
    let original = super::super::tests::snapshot("legacy", &["writer"]);
    assert_eq!(
        filter_for_bindings(
            &original,
            &Bindings::from([(String::from("writer"), key(1))])
        )
        .unwrap(),
        None
    );
    let bound = Bindings::from([(
        String::from("editor"),
        WorkspaceDataKey::LegacyAgent(String::from("writer")),
    )]);
    assert_eq!(
        parse_data(&filter_for_bindings(&original, &bound).unwrap().unwrap())
            .unwrap()
            .jobs
            .len(),
        1
    );
    let default = super::super::tests::snapshot("legacy", &["default"]);
    let selected = filter_for_bindings(
        &default,
        &Bindings::from([(String::from("default"), key(1))]),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        owner_key(&parse_data(&selected).unwrap(), "default-job"),
        key(1)
    );
    assert_eq!(
        merge_for_bindings(
            Some(&selected),
            None,
            &BTreeSet::new(),
            &Bindings::new(),
            &Bindings::new(),
            &Bindings::new(),
            &BTreeSet::new()
        )
        .unwrap(),
        Some(selected)
    );
}

#[test]
fn restored_claim_without_an_archived_trace_cannot_target_a_retained_inbox_run() {
    let current_inbox = json!({"version":1,"events":[],"traces":{"writer-run":{
        "run_id":"writer-run","created_at":1.0,"completed_at":null,"status":"running",
        "meta":{"source":"cron","agent_id":"default"},"events":[]
    }}})
    .to_string();
    let selected = BTreeSet::from(["writer"]);
    let protected =
        crate::desktop_inbox::retained_backup_run_ids(Some(&current_inbox), &selected).unwrap();
    assert_eq!(protected, BTreeSet::from([String::from("writer-run")]));
    let mut archived = parse_data(&native(&[("writer", 1)])).unwrap();
    archived.active_runs.get_mut("writer-run").unwrap().agent_id = Some(String::from("default"));
    let archived = encode(&archived).unwrap();
    assert_eq!(
        merge_for_bindings(
            None,
            Some(&archived),
            &selected,
            &Bindings::new(),
            &Bindings::from([(String::from("writer"), key(1))]),
            &Bindings::from([(String::from("writer"), key(2))]),
            &protected
        ),
        Err("Restored Cron run ID conflicts with an unselected Inbox record")
    );
    assert_eq!(
        crate::desktop_inbox::retained_backup_run_ids(
            Some(&current_inbox),
            &BTreeSet::from(["default"])
        )
        .unwrap(),
        BTreeSet::new()
    );
}
