use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::*;

#[path = "desktop_cron_namespace_tests.rs"]
mod namespaces;

pub(super) fn snapshot(label: &str, agents: &[&str]) -> String {
    let mut data = CronData::default();
    for agent in agents {
        let id = format!("{agent}-job");
        data.jobs.push(
            serde_json::from_value(json!({"id":id, "name":label,
                "task_type":"text", "text":label, "schedule":{"cron":"* * * * *"},
                "dispatch":{"target":{"user_id":agent,"session_id":"same"}},
                "meta":{"agent_id":"forged-owner"}
            }))
            .unwrap(),
        );
        if *agent != "default" {
            data.owners.insert(id.clone(), (*agent).to_owned());
        }
        data.states.insert(
            id.clone(),
            serde_json::from_value(json!({
                "next_run_at":"2030-01-01T00:00:00Z", "last_run_at":null,
                "last_status":"running", "last_error":label
            }))
            .unwrap(),
        );
        data.history.insert(
            id.clone(),
            serde_json::from_value(json!([{
                "run_at":"2029-12-31T23:59:00Z", "status":"error", "error":label, "trigger":"manual"
            }]))
            .unwrap(),
        );
        data.scheduled.insert(id.clone());
        data.active_triggers
            .insert(id.clone(), String::from("manual"));
        data.active_runs.insert(
            format!("{agent}-run"),
            super::super::AgentRunClaim {
                agent_id: None,
                data_key: None,
                job_id: id,
                trigger: label.to_owned(),
            },
        );
    }
    encode(&data).unwrap()
}

fn value(serialized: &str) -> Value {
    serde_json::from_str(serialized).unwrap()
}

#[test]
fn cron_backup_filters_every_job_owned_record_and_drops_unowned_orphans() {
    let original = snapshot("backup", &["default", "writer", "other"]);
    let mut data = value(&original);
    data["states"]["orphan"] = data["states"]["writer-job"].clone();
    data["history"]["orphan"] = data["history"]["writer-job"].clone();
    data["scheduled"]
        .as_array_mut()
        .unwrap()
        .push(json!("orphan"));
    data["active_triggers"]["orphan"] = json!("manual");
    data["active_runs"]["orphan-run"] = json!({"job_id":"orphan", "trigger":"manual"});
    let input = data.to_string();
    for agent in ["default", "writer", "other"] {
        let mut expected = value(&snapshot("backup", &[agent]));
        expected["version"] = json!(2);
        assert_eq!(
            value(
                &filter_backup_data(&input, &BTreeSet::from([agent]))
                    .unwrap()
                    .unwrap()
            ),
            expected
        );
    }
    for selection in [BTreeSet::new(), BTreeSet::from(["absent"])] {
        assert_eq!(filter_backup_data(&input, &selection), Ok(None));
    }
    assert_eq!(
        filter_backup_data(&original, &BTreeSet::from(["default", "writer", "other"])),
        Ok(Some(original))
    );
}

#[test]
fn cron_restore_replaces_only_selected_jobs_and_all_their_runtime_records() {
    let local = snapshot("local", &["default", "writer", "other"]);
    let incoming = snapshot("backup", &["default", "writer", "other"]);
    let restored = merge_restore_data(Some(&local), Some(&incoming), &BTreeSet::from(["writer"]))
        .unwrap()
        .unwrap();
    let mut expected = value(&local);
    let backup = value(&incoming);
    expected["jobs"] = json!([expected["jobs"][0], expected["jobs"][2], backup["jobs"][1]]);
    expected["states"]["writer-job"] = backup["states"]["writer-job"].clone();
    expected["history"]["writer-job"] = backup["history"]["writer-job"].clone();
    expected["active_runs"]["writer-run"] = backup["active_runs"]["writer-run"].clone();
    assert_eq!(value(&restored), expected);
    assert_eq!(
        merge_restore_data(Some(&local), None, &BTreeSet::from(["writer"])).unwrap(),
        Some(snapshot("local", &["default", "other"]))
    );
    assert_eq!(
        merge_restore_data(Some(&local), None, &BTreeSet::from(["default"])).unwrap(),
        Some(snapshot("local", &["writer", "other"]))
    );
    assert_eq!(
        merge_restore_data(
            Some(&local),
            None,
            &BTreeSet::from(["default", "writer", "other"])
        ),
        Ok(None)
    );
    assert_eq!(
        merge_restore_data(Some(&local), Some(&incoming), &BTreeSet::new()),
        Ok(Some(local.clone()))
    );
    assert_eq!(
        merge_restore_data(Some(&local), Some(&incoming), &BTreeSet::from(["absent"])),
        Ok(Some(local))
    );
    assert_eq!(
        merge_restore_data(None, None, &BTreeSet::from(["default"])),
        Ok(None)
    );
}

#[test]
fn cron_restore_rejects_cross_agent_claim_collisions_before_mutation() {
    let local = snapshot("local", &["other"]);
    let mut incoming = value(&snapshot("backup", &["writer"]));
    incoming["active_runs"]["other-run"] = incoming["active_runs"]["writer-run"].clone();
    let incoming = incoming.to_string();
    let before = (local.clone(), incoming.clone());
    assert_eq!(
        merge_restore_data(Some(&local), Some(&incoming), &BTreeSet::from(["writer"])),
        Err("Restored Cron run ID conflicts with an unselected Agent")
    );
    assert_eq!((local.clone(), incoming), before);
}

#[test]
fn cron_backup_rejects_invalid_ownership_ids_versions_and_size() {
    for kind in [
        "owner",
        "orphan-owner",
        "id",
        "duplicate",
        "version",
        "v1-owner",
    ] {
        let mut data = value(&snapshot("backup", &["writer"]));
        match kind {
            "owner" => data["owners"]["writer-job"] = json!("../default"),
            "orphan-owner" => data["owners"]["unknown-job"] = json!("writer"),
            "id" => data["jobs"][0]["id"] = Value::Null,
            "duplicate" => {
                let job = data["jobs"][0].clone();
                data["jobs"].as_array_mut().unwrap().push(job);
            }
            "v1-owner" => data["version"] = json!(1),
            _ => data["version"] = json!(4),
        }
        assert!(
            filter_backup_data(&data.to_string(), &BTreeSet::from(["writer"])).is_err(),
            "{kind}"
        );
    }
    assert!(parse_data(&" ".repeat(MAX_CRON_DATA_BYTES + 1)).is_err());
    // Persisted invalid schedules still reach the scheduler's disable/error path.
    let mut data = value(&snapshot("backup", &["default"]));
    data["jobs"][0]["schedule"]["cron"] = json!("invalid legacy rule");
    let data = data.to_string();
    assert_eq!(
        filter_backup_data(&data, &BTreeSet::from(["default"])),
        Ok(Some(data))
    );
}

#[test]
fn owned_cron_data_uses_a_version_that_old_default_only_readers_reject() {
    let old = snapshot("existing", &["default"]);
    assert_eq!(value(&old)["version"], 1);
    assert!(parse_data(&old).is_ok());
    let mut current = parse_data(&old).unwrap();
    current
        .owners
        .insert(String::from("default-job"), String::from("writer"));
    let encoded = encode(&current).unwrap();
    assert_eq!(value(&encoded)["version"], 2);
    assert_eq!(
        parse_data(&encoded).unwrap().owners["default-job"],
        "writer"
    );
    // The old implementation admitted exactly version 1, before any dispatch.
    assert_ne!(value(&encoded)["version"], value(&old)["version"]);
}

#[test]
fn cron_restore_enforces_combined_job_capacity() {
    let local = snapshot("local", &["other"]);
    let mut incoming = CronData::default();
    for index in 0..MAX_CRON_JOBS {
        let mut data = parse_data(&snapshot("backup", &["writer"])).unwrap();
        let id = format!("writer-{index}");
        data.jobs[0].id = Some(id.clone());
        incoming.jobs.extend(data.jobs);
        incoming.owners.insert(id, String::from("writer"));
    }
    let incoming = encode(&incoming).unwrap();
    assert_eq!(
        merge_restore_data(Some(&local), Some(&incoming), &BTreeSet::from(["writer"])),
        Err("Restored Cron data exceeds its size limit")
    );
}
