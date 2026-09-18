use qwenpaw_protocol::Thread;
use qwenpaw_protocol::ThreadStatus;
use qwenpaw_storage::StoredModelCall;
use qwenpaw_storage::StoredThread;
use qwenpaw_storage::StoredUsageRecord;
use serde_json::Value;
use serde_json::json;

use super::*;

#[test]
fn global_only_restore_preserves_channel_workspaces_and_does_not_import_them() {
    let local = BTreeMap::from([
        (
            CHANNEL_DATA.to_owned(),
            json!({"version":2,"workspaces":[]}).to_string(),
        ),
        (String::from("ui_language"), String::from("en")),
    ]);
    let archived = BTreeMap::from([
        (
            CHANNEL_DATA.to_owned(),
            json!({"version":1,"console":{"bot_prefix":"not selected"}}).to_string(),
        ),
        (String::from("ui_language"), String::from("zh")),
    ]);
    let empty = BTreeMap::new();
    let bindings = desktop_chats::RestoreBindings {
        current: &empty,
        source: &empty,
        target: &empty,
    };
    let (settings, preserved) = merge_settings(
        &local,
        &archived,
        &BTreeSet::new(),
        &BTreeSet::new(),
        true,
        false,
        &bindings,
    )
    .unwrap();
    let mut expected = local.clone();
    expected.insert(String::from("ui_language"), String::from("zh"));
    assert_eq!(settings, expected);
    assert!(preserved.is_empty());
    let (settings, _) = merge_settings(
        &BTreeMap::new(),
        &archived,
        &BTreeSet::new(),
        &BTreeSet::new(),
        true,
        false,
        &bindings,
    )
    .unwrap();
    assert_eq!(
        settings,
        BTreeMap::from([(String::from("ui_language"), String::from("zh"))])
    );
}

fn snapshot(label: &str) -> StoreBackup {
    let mut chats = serde_json::Map::new();
    let mut groups = Vec::new();
    let mut events = Vec::new();
    let mut traces = serde_json::Map::new();
    let mut mail = serde_json::Map::new();
    let mut threads = Vec::new();
    let mut usage = Vec::new();
    for id in ["default", "writer", "other"] {
        for (order, kind) in ["default", "cron", "subagents"].into_iter().enumerate() {
            let mut group = json!({"id": kind, "name": format!("{label} {id} {kind}"),
                "data_key": {"kind": "legacy_agent", "id": id},
                "order": order, "kind": kind, "source": null, "pinned": true});
            if id != "default" {
                group["agent_id"] = json!(id);
            }
            groups.push(group);
        }
        chats.insert(
            format!("{id}-thread"),
            json!({"agent_id": id, "data_key": {"kind": "legacy_agent", "id": id},
            "name": format!("{label} {id}"), "session_id": format!("{id}-session"),
            "user_id": "desktop", "channel": "console", "meta": {"note": label},
            "pinned": true, "source": "chat", "group_id": "default",
            "parent_session_id": null, "root_session_id": null,
            "updated_at": 1, "last_finished_at": null}),
        );
        events.push(json!({"id": format!("{id}-event"), "agent_id": id,
            "source_type": "cron", "source_id": "job", "event_type": "result",
            "status": "success", "severity": "info", "title": label,
            "body": label, "payload": {"run_id": format!("{id}-run")},
            "read": true, "created_at": 1.0}));
        traces.insert(
            format!("{id}-run"),
            json!({"run_id": format!("{id}-run"),
            "created_at": 1.0, "completed_at": 2.0, "status": "success",
            "meta": {"agent_id": id}, "events": [{"at": 1.0, "event": {"text": label}}]}),
        );
        mail.insert(
            id.to_owned(),
            json!({"whitelist": {"user@example.com": {
            "remark": label, "display_name": id}}, "blacklist": {},
            "pending": [], "approved_replay": []}),
        );
        threads.push(thread(&format!("{id}-thread"), label));
        usage.push(StoredUsageRecord {
            id: format!("{id}-usage"),
            thread_id: format!("{id}-thread"),
            turn_id: format!("{id}-turn"),
            agent_id: id.to_owned(),
            data_key: Some(qwenpaw_storage::WorkspaceDataKey::LegacyAgent(
                id.to_owned(),
            )),
            recorded_at: 1,
            call: StoredModelCall {
                provider_id: label.to_owned(),
                model: label.to_owned(),
                prompt_tokens: 1,
                completion_tokens: 2,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                cache_eligible_input_tokens: 0,
                cache_observed: false,
                usage_observed: true,
            },
        });
    }
    // Uncatalogued App Protocol sessions are owned by the default runtime.
    threads.push(thread("sdk-thread", label));
    let settings = BTreeMap::from([
        (
            CHAT_DATA.to_owned(),
            json!({"version": 2, "chats": chats, "groups": groups}).to_string(),
        ),
        (
            INBOX_DATA.to_owned(),
            json!({"version": 1, "events": events, "traces": traces}).to_string(),
        ),
        (MAIL_DATA.to_owned(), mail_setting(mail)),
        (CRON_DATA.to_owned(), default_cron_snapshot(label)),
        (HEARTBEAT_DATA.to_owned(), label.to_owned()),
        (
            "desktop_security_data".to_owned(),
            format!("{label} security"),
        ),
        ("desktop_mcp_data".to_owned(), format!("{label} mcp")),
        (
            "ui_language".to_owned(),
            if label == "local" { "en" } else { "zh" }.to_owned(),
        ),
    ]);
    StoreBackup {
        version: 2,
        settings,
        threads,
        usage,
    }
}

fn mail_setting(agents: serde_json::Map<String, Value>) -> String {
    let workspaces = agents.into_iter().map(|(id, acl)| {
        json!({"data_key": {"kind": "legacy_agent", "id": id}, "agents": {id: acl}})
    }).collect::<Vec<_>>();
    json!({"version": 2, "workspaces": workspaces}).to_string()
}

fn default_cron_snapshot(label: &str) -> String {
    json!({"version":1,"jobs":[{"id":"default-job","name":label,
        "task_type":"text","text":label,"schedule":{"cron":"* * * * *"},
        "dispatch":{"target":{"user_id":"admin","session_id":"same"}}}],
        "states":{},"history":{}})
    .to_string()
}

fn thread(id: &str, label: &str) -> StoredThread {
    StoredThread {
        thread: Thread {
            id: id.to_owned(),
            model: label.to_owned(),
            workspace_root: Some(String::from("/same/project")),
            status: ThreadStatus::Idle,
            archived: false,
            created_at: 1,
            updated_at: 1,
        },
        turns: Vec::new(),
        messages: Vec::new(),
        turn_metadata: Vec::new(),
    }
}

fn data(state: &StoreBackup, key: &str) -> Value {
    serde_json::from_str(&state.settings[key]).unwrap()
}

fn view(state: &StoreBackup) -> Value {
    let mut result = serde_json::to_value(state).unwrap();
    for key in [CHAT_DATA, INBOX_DATA, MAIL_DATA] {
        if let Some(value) = state.settings.get(key) {
            result["settings"][key] = serde_json::from_str(value).unwrap();
        }
    }
    result
}

#[test]
fn selected_agent_merge_preserves_complete_unselected_data_with_or_without_globals() {
    let current = snapshot("local");
    let archived = snapshot("archived");
    let before = current.clone();
    let backup_before = archived.clone();
    for globals in [false, true] {
        let restored = merge(
            &current,
            &archived,
            &BTreeSet::from(["writer"]),
            globals,
            true,
        )
        .unwrap();
        let mut expected = current.clone();
        let mut chats = data(&current, CHAT_DATA);
        let archived_chats = data(&archived, CHAT_DATA);
        chats["chats"]["writer-thread"] = archived_chats["chats"]["writer-thread"].clone();
        chats["groups"]
            .as_array_mut()
            .unwrap()
            .retain(|group| group["agent_id"] != "writer");
        chats["groups"].as_array_mut().unwrap().extend(
            archived_chats["groups"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|group| group["agent_id"] == "writer")
                .cloned(),
        );
        expected
            .settings
            .insert(CHAT_DATA.to_owned(), chats.to_string());
        let mut inbox = data(&current, INBOX_DATA);
        let archived_inbox = data(&archived, INBOX_DATA);
        inbox["events"]
            .as_array_mut()
            .unwrap()
            .retain(|event| event["agent_id"] != "writer");
        inbox["events"]
            .as_array_mut()
            .unwrap()
            .push(archived_inbox["events"][1].clone());
        inbox["traces"]["writer-run"] = archived_inbox["traces"]["writer-run"].clone();
        expected
            .settings
            .insert(INBOX_DATA.to_owned(), inbox.to_string());
        let mut mail = data(&current, MAIL_DATA);
        let archived_mail = data(&archived, MAIL_DATA);
        for workspace in mail["workspaces"].as_array_mut().unwrap() {
            if workspace["data_key"]["id"] == "writer" {
                *workspace = archived_mail["workspaces"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|entry| entry["data_key"]["id"] == "writer")
                    .unwrap()
                    .clone();
            }
        }
        expected
            .settings
            .insert(MAIL_DATA.to_owned(), mail.to_string());
        expected.threads.remove(1);
        expected.threads.push(archived.threads[1].clone());
        expected.usage.remove(1);
        expected.usage.push(archived.usage[1].clone());
        if globals {
            expected
                .settings
                .insert("ui_language".to_owned(), String::from("zh"));
        }
        assert_eq!(view(&restored.snapshot), view(&expected));
        assert_eq!(
            restored.preserved_local_keys,
            if globals {
                vec!["security", "mcp"]
            } else {
                Vec::<&str>::new()
            }
        );
    }
    assert_eq!(current, before);
    assert_eq!(archived, backup_before);
}

#[test]
fn global_only_never_replaces_agents_and_reports_only_actual_protected_overlays() {
    let mut current = snapshot("local");
    current.settings.remove("desktop_mcp_data");
    let archived = snapshot("archived");
    for preserve in [false, true] {
        let restored = merge(&current, &archived, &BTreeSet::new(), true, preserve).unwrap();
        let mut expected = current.clone();
        for (key, value) in &archived.settings {
            if !(AGENT_KEYS.contains(&key.as_str()) || preserve && key == "desktop_security_data") {
                expected.settings.insert(key.clone(), value.clone());
            }
        }
        assert_eq!(restored.snapshot, expected);
        assert_eq!(
            restored.preserved_local_keys,
            if preserve {
                vec!["security"]
            } else {
                Vec::<&str>::new()
            }
        );
    }
}

#[test]
fn restoring_default_includes_sdk_threads_and_resets_only_default_schedules() {
    let current = snapshot("local");
    let mut archived = snapshot("archived");
    archived.settings.remove(CRON_DATA);
    let restored = merge(
        &current,
        &archived,
        &BTreeSet::from(["default"]),
        false,
        false,
    )
    .unwrap();
    assert_eq!(
        restored.snapshot.threads,
        vec![
            current.threads[1].clone(),
            current.threads[2].clone(),
            archived.threads[0].clone(),
            archived.threads[3].clone()
        ]
    );
    assert!(!restored.snapshot.settings.contains_key(CRON_DATA));
    assert_eq!(restored.snapshot.settings[HEARTBEAT_DATA], "archived");
    assert_eq!(
        restored.snapshot.settings["desktop_security_data"],
        "local security"
    );
}

#[test]
fn identity_collisions_fail_before_mutating_either_snapshot() {
    let current = snapshot("local");
    let before = current.clone();
    for kind in ["thread", "event", "trace", "usage"] {
        let mut archived = snapshot("archived");
        let expected = match kind {
            "thread" => {
                let mut chats = data(&archived, CHAT_DATA);
                let chat = chats["chats"]
                    .as_object_mut()
                    .unwrap()
                    .remove("writer-thread")
                    .unwrap();
                chats["chats"]["other-thread"] = chat;
                archived
                    .settings
                    .insert(CHAT_DATA.to_owned(), chats.to_string());
                archived
                    .threads
                    .retain(|stored| stored.thread.id != "other-thread");
                archived.threads[1].thread.id = String::from("other-thread");
                "Restored Thread ID conflicts with existing data"
            }
            "event" => {
                let mut inbox = data(&archived, INBOX_DATA);
                inbox["events"][1]["id"] = json!("other-event");
                archived
                    .settings
                    .insert(INBOX_DATA.to_owned(), inbox.to_string());
                "Restored Inbox event ID conflicts with existing data"
            }
            "trace" => {
                let mut inbox = data(&archived, INBOX_DATA);
                inbox["events"][1]["payload"]["run_id"] = json!("other-run");
                archived
                    .settings
                    .insert(INBOX_DATA.to_owned(), inbox.to_string());
                "Restored Inbox run ID conflicts with an unselected Agent"
            }
            _ => {
                archived.usage[1].id = String::from("other-usage");
                "Restored usage ID conflicts with existing data"
            }
        };
        let archived_before = archived.clone();
        assert_eq!(
            merge(&current, &archived, &BTreeSet::from(["writer"]), true, true).unwrap_err(),
            expected
        );
        assert_eq!(archived, archived_before);
        assert_eq!(current, before);
    }
}

#[test]
fn absent_selected_records_clear_only_that_agents_data() {
    let current = snapshot("local");
    let archived = StoreBackup {
        version: 1,
        settings: BTreeMap::new(),
        threads: Vec::new(),
        usage: Vec::new(),
    };
    let restored = merge(
        &current,
        &archived,
        &BTreeSet::from(["writer"]),
        false,
        false,
    )
    .unwrap();
    let mut expected = current.clone();
    let mut chats = data(&current, CHAT_DATA);
    chats["chats"]
        .as_object_mut()
        .unwrap()
        .remove("writer-thread");
    chats["groups"]
        .as_array_mut()
        .unwrap()
        .retain(|group| group["agent_id"] != "writer");
    expected
        .settings
        .insert(CHAT_DATA.to_owned(), chats.to_string());
    let mut inbox = data(&current, INBOX_DATA);
    inbox["events"].as_array_mut().unwrap().remove(1);
    inbox["traces"]
        .as_object_mut()
        .unwrap()
        .remove("writer-run");
    expected
        .settings
        .insert(INBOX_DATA.to_owned(), inbox.to_string());
    let mut mail = data(&current, MAIL_DATA);
    mail["workspaces"]
        .as_array_mut()
        .unwrap()
        .retain(|workspace| workspace["data_key"]["id"] != "writer");
    expected
        .settings
        .insert(MAIL_DATA.to_owned(), mail.to_string());
    expected.threads.remove(1);
    expected.usage.remove(1);
    assert_eq!(view(&restored.snapshot), view(&expected));
}

#[test]
fn malformed_scoped_data_and_versions_fail_instead_of_dropping_them() {
    let current = snapshot("local");
    for key in [CHAT_DATA, INBOX_DATA, MAIL_DATA] {
        let mut archived = snapshot("archived");
        archived
            .settings
            .insert(key.to_owned(), String::from("{bad json"));
        assert!(
            merge(
                &current,
                &archived,
                &BTreeSet::from(["writer"]),
                false,
                false
            )
            .is_err()
        );
    }
    let mut archived = snapshot("archived");
    archived.version = 3;
    assert_eq!(
        merge(&current, &archived, &BTreeSet::new(), false, false).unwrap_err(),
        "Unsupported Core backup version"
    );
}

#[test]
fn unselected_and_ownerless_traces_survive_even_when_selected_events_refer_to_them() {
    let mut current = snapshot("local");
    let mut inbox = data(&current, INBOX_DATA);
    inbox["traces"]["writer-run"]["meta"]["agent_id"] = json!("other");
    let mut orphan = inbox["traces"]["other-run"].clone();
    orphan["run_id"] = json!("orphan");
    orphan["meta"] = json!({});
    inbox["traces"]["orphan"] = orphan;
    current
        .settings
        .insert(INBOX_DATA.to_owned(), inbox.to_string());
    let archived = StoreBackup {
        version: 1,
        settings: BTreeMap::new(),
        threads: Vec::new(),
        usage: Vec::new(),
    };
    let result = merge(
        &current,
        &archived,
        &BTreeSet::from(["writer"]),
        false,
        false,
    )
    .unwrap();
    let mut expected = inbox;
    expected["events"].as_array_mut().unwrap().remove(1);
    assert_eq!(data(&result.snapshot, INBOX_DATA), expected);
    assert_eq!(
        merge(
            &current,
            &snapshot("archived"),
            &BTreeSet::from(["writer"]),
            false,
            false
        )
        .unwrap_err(),
        "Restored Inbox trace ID conflicts with existing data"
    );
}

#[test]
fn project_rebasing_uses_agent_ownership_and_does_not_rewrite_history_or_external_projects() {
    let directory = tempfile::tempdir().unwrap();
    for agent_id in ["writer", "default"] {
        let destination = directory.path().canonicalize().unwrap().join(agent_id);
        let mut original = snapshot("archived");
        for thread in &mut original.threads {
            thread.thread.workspace_root = Some(String::from("c:\\SHARED\\project"));
            thread.messages.push(qwenpaw_storage::StoredMessage::text(
                "user",
                "Keep c:\\SHARED\\project in history",
            ));
        }
        let mut chats = data(&original, CHAT_DATA);
        for chat in chats["chats"].as_object_mut().unwrap().values_mut() {
            chat["meta"]["runtime_context"] = json!({"project_dir": "c:\\SHARED\\project",
                "project_dirs": [{"path": "C:\\shared\\project", "label": "C:\\shared\\project"}, {"path": "D:\\external"}],
                "note": "C:\\shared\\project"});
        }
        original
            .settings
            .insert(CHAT_DATA.to_owned(), chats.to_string());
        let mapping = super::super::super::desktop_agents::restore::WorkspaceRestore {
            id: agent_id.to_owned(),
            source_root: String::from("C:\\shared"),
            destination: destination.clone(),
        };
        let mut result = original.clone();
        desktop_chats::remap_restore_paths(
            &mut result,
            &[mapping],
            &desktop_chats::legacy_bindings(&BTreeSet::from(["default", "writer"])),
        )
        .unwrap();
        let mut expected = original;
        for stored in &mut expected.threads {
            if stored.thread.id == format!("{agent_id}-thread")
                || agent_id == "default" && stored.thread.id == "sdk-thread"
            {
                stored.thread.workspace_root =
                    Some(destination.join("project").to_string_lossy().into_owned());
            }
        }
        let context = &mut chats["chats"][format!("{agent_id}-thread")]["meta"]["runtime_context"];
        context["project_dir"] = json!(destination.join("project"));
        context["project_dirs"][0]["path"] = json!(destination.join("project"));
        expected
            .settings
            .insert(CHAT_DATA.to_owned(), chats.to_string());
        assert_eq!(view(&result), view(&expected));
    }
}

#[test]
fn path_rebasing_is_component_scoped_and_rejects_parent_suffixes() {
    use super::super::super::desktop_agents::restore::remap_workspace_path;

    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().canonicalize().unwrap();
    for (source, value, expected) in [
        (
            "/original",
            "/original/project",
            Some(destination.join("project")),
        ),
        ("/original", "/original-extra/project", None),
        ("/original", "/original/../outside", None),
        ("/", "/", Some(destination.clone())),
        (
            "C:\\Source",
            "c:\\source\\project",
            Some(destination.join("project")),
        ),
        ("C:\\Source", "D:\\source\\project", None),
        ("C:\\Source", "C:\\source\\..\\outside", None),
        (
            "\\\\server\\share",
            "\\\\SERVER\\share\\project",
            Some(destination.join("project")),
        ),
    ] {
        assert_eq!(
            remap_workspace_path(value, source, &destination),
            expected.map(|path| path.to_string_lossy().into_owned())
        );
    }
}

#[tokio::test]
async fn runtime_protection_includes_unpersisted_defaults_and_bootstrap_mcp_configuration() {
    let core = qwenpaw_core::Core::new(qwenpaw_core::ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1/v1"),
        default_model: String::from("fixture"),
    });
    let client: qwenpaw_core::McpClientSettings = serde_json::from_value(json!({
        "key": "bootstrap", "enabled": false, "transport": "streamable_http", "url": "https://fixture.example/mcp",
        "headers": {"Authorization": "Bearer private-bootstrap-key"}
    })).unwrap();
    core.replace_mcp_client_settings(vec![client.clone()])
        .unwrap();
    let current = core.backup_snapshot(1024 * 1024).unwrap();
    assert!(!current.settings.contains_key("desktop_mcp_data"));
    assert!(!current.settings.contains_key("desktop_security_data"));
    let plan = super::super::super::desktop_agents::restore::AgentRestorePlan {
        catalog: Vec::new(),
        agents: super::super::super::desktop_agents::AgentBackupSnapshot {
            version: 1,
            agents: Vec::new(),
        },
        workspaces: Vec::new(),
        identity_markers: BTreeMap::new(),
        current_bindings: BTreeMap::new(),
        archived_bindings: BTreeMap::new(),
        removed_agent_ids: Vec::new(),
    };
    let mut incoming = current.clone();
    incoming.settings.insert(
        String::from("desktop_mcp_data"),
        String::from("foreign MCP metadata"),
    );
    incoming.settings.insert(
        String::from("desktop_security_data"),
        String::from("foreign security"),
    );
    let lease = core
        .begin_restore(std::time::Duration::from_secs(2))
        .await
        .unwrap();
    let mut sanitized = client.clone();
    sanitized.headers.clear();
    for globals in [false, true] {
        for protect in [false, true] {
            let restored =
                merge_for_agents(&core, &current, &incoming, &plan, globals, protect).unwrap();
            let mut expected = if globals {
                incoming.clone()
            } else {
                current.clone()
            };
            if globals && protect {
                expected.settings.insert(
                    String::from("desktop_mcp_data"),
                    json!({"version": 1, "clients": [sanitized]}).to_string(),
                );
                expected.settings.insert(
                    String::from("desktop_security_data"),
                    core.backup_security_data().unwrap(),
                );
                assert_eq!(restored.preserved_local_keys, ["security", "mcp"]);
                let actual: Value =
                    serde_json::from_str(&restored.snapshot.settings["desktop_mcp_data"]).unwrap();
                let expected_mcp: Value =
                    serde_json::from_str(&expected.settings["desktop_mcp_data"]).unwrap();
                assert_eq!(actual, expected_mcp);
                // Field ordering is not part of the persisted JSON contract.
                expected.settings.insert(
                    String::from("desktop_mcp_data"),
                    restored.snapshot.settings["desktop_mcp_data"].clone(),
                );
                assert!(
                    !serde_json::to_string(&restored.snapshot)
                        .unwrap()
                        .contains("private-bootstrap-key")
                );
                let candidate = lease.prepare_restore(&restored.snapshot).unwrap();
                assert_eq!(
                    candidate.security_settings().unwrap(),
                    core.security_settings().unwrap()
                );
            } else {
                assert_eq!(restored.preserved_local_keys, Vec::<String>::new());
            }
            assert_eq!(restored.snapshot, expected);
        }
    }
    assert_eq!(core.backup_snapshot(1024 * 1024).unwrap(), current);
    assert_eq!(core.mcp_client_settings(), vec![client]);
}
