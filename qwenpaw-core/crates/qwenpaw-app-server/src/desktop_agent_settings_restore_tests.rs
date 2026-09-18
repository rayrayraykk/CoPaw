use super::super::desktop_agents::AgentBackupSnapshot;
use super::super::desktop_agents::BackupAgent;
use super::*;

fn core() -> Core {
    Core::new(qwenpaw_core::ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1/v1"),
        default_model: String::from("fixture-model"),
    })
}

fn agents(root: &Path) -> AgentBackupSnapshot {
    AgentBackupSnapshot {
        version: 1,
        agents: ["default", "writer"].into_iter().map(|id| {
            let workspace = root.join(id).to_string_lossy().into_owned();
            BackupAgent {
                data_key: None,
                id: id.to_owned(), workspace_dir: workspace.clone(),
                enabled: id == "default", pinned: id == "default",
                config: json!({"id": id, "workspace_dir": workspace, "running": default_running_config()}),
            }
        }).collect(),
    }
}

#[tokio::test]
async fn planned_default_runtime_overrides_global_settings_without_creating_workspaces_or_templates()
 {
    let directory = tempfile::tempdir().unwrap();
    let live = core();
    let mut global = DesktopAgentSettings::default();
    global.running_config["shell_command_timeout"] = json!(31);
    live.write_agent_settings_data(&serde_json::to_string(&global).unwrap())
        .unwrap();
    let original = live.backup_snapshot(1024 * 1024).unwrap();
    let mut planned = agents(directory.path());
    let running = &mut planned.agents[0].config["running"];
    running["shell_command_timeout"] = json!(97);
    running["shell_command_executable"] = json!("fixture-shell");
    running["approval_level"] = json!("STRICT");
    running["loop"]["iteration"]["enabled"] = json!(true);
    running["loop"]["iteration"]["max_iterations"] = json!(27);
    let expected = AgentRuntimeConfig {
        max_agent_steps: 27,
        shell_timeout_ms: 97_000,
        shell_executable: String::from("fixture-shell"),
        approval_level: ToolApprovalLevel::Strict,
    };
    let mut lease = live.begin_restore(Duration::from_secs(2)).await.unwrap();
    let rollback = lease.capture_rollback(1024 * 1024).unwrap();
    let candidate = lease.prepare_restore(&original).unwrap();
    hydrate_restore(&candidate, &planned).unwrap();
    assert_eq!(candidate.agent_runtime_config().unwrap(), expected);
    assert_eq!(candidate.backup_snapshot(1024 * 1024).unwrap(), original);
    assert_eq!(
        live.agent_runtime_config().unwrap(),
        rollback.agent_runtime_config().unwrap()
    );
    lease.apply(&candidate, 1024 * 1024).await.unwrap();
    assert_eq!(live.agent_runtime_config().unwrap(), expected);
    lease.apply(&rollback, 1024 * 1024).await.unwrap();
    assert_eq!(
        live.agent_runtime_config().unwrap(),
        rollback.agent_runtime_config().unwrap()
    );
    assert_eq!(live.backup_snapshot(1024 * 1024).unwrap(), original);
    assert!(
        std::fs::read_dir(directory.path())
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn invalid_disabled_agent_cannot_partially_replace_the_candidates_default_runtime() {
    let directory = tempfile::tempdir().unwrap();
    let candidate = core();
    let before = candidate.agent_runtime_config().unwrap();
    let original = candidate.backup_snapshot(1024 * 1024).unwrap();
    for invalid in [Value::Null, json!({}), json!({"shell_command_timeout": -1})] {
        let mut planned = agents(directory.path());
        planned.agents[0].config["running"]["shell_command_timeout"] = json!(97);
        planned.agents[1].config["running"] = invalid;
        assert_eq!(
            hydrate_restore(&candidate, &planned),
            Err("Restored Agent runtime is invalid")
        );
        assert_eq!(candidate.agent_runtime_config().unwrap(), before);
        assert_eq!(candidate.backup_snapshot(1024 * 1024).unwrap(), original);
    }
    assert!(
        std::fs::read_dir(directory.path())
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn absent_profile_running_config_uses_global_settings_but_missing_default_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let candidate = core();
    let mut global = DesktopAgentSettings::default();
    global.running_config["shell_command_timeout"] = json!(43);
    candidate
        .write_agent_settings_data(&serde_json::to_string(&global).unwrap())
        .unwrap();
    let mut planned = agents(directory.path());
    planned.agents[0]
        .config
        .as_object_mut()
        .unwrap()
        .remove("running");
    hydrate_restore(&candidate, &planned).unwrap();
    assert_eq!(
        candidate.agent_runtime_config().unwrap(),
        runtime_config(&global.running_config).unwrap()
    );
    let before = candidate.agent_runtime_config().unwrap();
    planned.agents.remove(0);
    assert_eq!(
        hydrate_restore(&candidate, &planned),
        Err("Restored default Agent is missing")
    );
    assert_eq!(candidate.agent_runtime_config().unwrap(), before);
}

#[test]
fn corrupt_global_settings_are_not_hidden_by_an_otherwise_valid_default_profile() {
    let directory = tempfile::tempdir().unwrap();
    let candidate = core();
    let before = candidate.agent_runtime_config().unwrap();
    candidate
        .write_agent_settings_data("{\"version\":999}")
        .unwrap();
    assert_eq!(
        hydrate_restore(&candidate, &agents(directory.path())),
        Err("Restored Agent settings are invalid")
    );
    assert_eq!(candidate.agent_runtime_config().unwrap(), before);
}
