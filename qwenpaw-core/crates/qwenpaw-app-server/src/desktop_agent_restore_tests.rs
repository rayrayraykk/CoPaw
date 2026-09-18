use super::super::BackupAgentReference;
use super::super::catalog_path;
use super::super::write_catalog;
use super::*;

struct Fixture {
    directory: tempfile::TempDir,
    desktop: DesktopWorkspace,
    catalog: AgentCatalog,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let initial = directory.path().join("workspace");
        fs::create_dir(&initial).unwrap();
        let initial = initial.canonicalize().unwrap();
        let data_dir = initial.join(".core");
        fs::create_dir(&data_dir).unwrap();
        let desktop = DesktopWorkspace {
            data_dir,
            initial: initial.clone(),
            selected: tokio::sync::RwLock::new(initial),
        };
        let mut catalog = AgentCatalog {
            schema_version: 1,
            revision: 7,
            order: Vec::new(),
            agents: BTreeMap::new(),
            workspace_keys: BTreeMap::new(),
            bootstrap_identity: false,
        };
        for id in ["default", "writer", "recent"] {
            let path = if id == "default" {
                desktop.initial.clone()
            } else {
                desktop.data_dir.join("workspaces").join(id)
            };
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("notes"), format!("{id} local files")).unwrap();
            catalog.order.push(id.to_owned());
            catalog.agents.insert(
                id.to_owned(),
                AgentReference {
                    data_key: None,
                    workspace_dir: path.to_string_lossy().into_owned(),
                    enabled: true,
                    pinned: id == "default",
                    config: default_agent_config(
                        id,
                        &format!("local {id}"),
                        "local settings",
                        &path,
                        "qwenpaw",
                        "en",
                        None,
                    ),
                },
            );
        }
        super::super::identity::hydrate_legacy(&mut catalog).unwrap();
        write_catalog(&desktop, &catalog).unwrap();
        Self {
            directory,
            desktop,
            catalog,
        }
    }

    fn archive(id: &str) -> AgentBackupSnapshot {
        let source = format!("C:\\source\\{id}");
        let mut config = default_agent_config(
            id,
            "archived name",
            &format!("keep text {source}"),
            Path::new(&source),
            "qwenpaw",
            "zh",
            None,
        );
        config["project_dir"] = json!(format!("{source}\\project"));
        AgentBackupSnapshot {
            version: 1,
            agents: vec![BackupAgent {
                data_key: None,
                id: id.to_owned(),
                workspace_dir: source,
                enabled: id == "default",
                pinned: id == "default",
                config,
            }],
        }
    }

    fn registry(archived: &AgentBackupSnapshot) -> AgentRegistryBackup {
        AgentRegistryBackup {
            version: 1,
            agents: vec![
                BackupAgentReference {
                    id: String::from("default"),
                    workspace_dir: String::from("C:\\source\\default"),
                    enabled: true,
                    pinned: true,
                },
                archived.agents[0].reference(),
            ],
        }
    }
}

#[test]
fn full_and_custom_restore_differ_only_in_registry_selection_and_preserve_unselected_files() {
    let fixture = Fixture::new();
    let archived = Fixture::archive("writer");
    let registry = Fixture::registry(&archived);
    let catalog_before = fs::read(catalog_path(&fixture.desktop)).unwrap();
    for full in [false, true] {
        let plan = plan_restore(
            &fixture.desktop,
            &archived,
            full.then_some(&registry),
            &BTreeSet::from([String::from("writer")]),
            None,
        )
        .unwrap();
        let mut expected = fixture.catalog.clone();
        expected.revision += 1;
        let writer = expected.agents.get_mut("writer").unwrap();
        writer.enabled = false;
        writer.config = archived.agents[0].config.clone();
        writer.config["workspace_dir"] = json!(writer.workspace_dir);
        writer.config["project_dir"] = json!(
            Path::new(&writer.workspace_dir)
                .join("project")
                .to_string_lossy()
        );
        if full {
            expected.agents.remove("recent");
            expected.order.retain(|id| id != "recent");
        }
        assert_eq!(
            serde_json::from_slice::<Value>(&plan.catalog).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        assert_eq!(
            plan.removed_agent_ids,
            if full {
                vec!["recent"]
            } else {
                Vec::<&str>::new()
            }
        );
        assert_eq!(plan.workspaces.len(), 1);
        assert_eq!(
            plan.workspaces[0].destination,
            PathBuf::from(&fixture.catalog.agents["writer"].workspace_dir)
        );
        for agent in fixture.catalog.agents.values() {
            assert_eq!(
                fs::read_to_string(Path::new(&agent.workspace_dir).join("notes")).unwrap(),
                format!("{} local files", agent.config["id"].as_str().unwrap())
            );
        }
        assert_eq!(
            fs::read(catalog_path(&fixture.desktop)).unwrap(),
            catalog_before
        );
    }
}

#[test]
fn global_only_full_restore_changes_references_without_restoring_runtime_profiles_or_files() {
    let fixture = Fixture::new();
    let archive = Fixture::archive("writer");
    let registry = Fixture::registry(&archive);
    let plan = plan_restore(
        &fixture.desktop,
        &archive,
        Some(&registry),
        &BTreeSet::new(),
        None,
    )
    .unwrap();
    let mut expected = fixture.catalog.clone();
    expected.revision += 1;
    expected.order.retain(|id| id != "recent");
    expected.agents.remove("recent");
    expected.agents.get_mut("writer").unwrap().enabled = false;
    assert_eq!(
        serde_json::from_slice::<Value>(&plan.catalog).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert!(plan.workspaces.is_empty());
    assert_eq!(plan.removed_agent_ids, ["recent"]);
}

#[test]
fn new_and_missing_local_workspaces_use_the_explicit_base_without_creating_it() {
    let fixture = Fixture::new();
    let fallback = fixture
        .directory
        .path()
        .canonicalize()
        .unwrap()
        .join("new-base");
    fs::rename(
        &fixture.catalog.agents["writer"].workspace_dir,
        fixture.directory.path().join("retained-writer"),
    )
    .unwrap();
    for id in ["new", "writer"] {
        let archive = Fixture::archive(id);
        let plan = plan_restore(
            &fixture.desktop,
            &archive,
            None,
            &BTreeSet::from([id.to_owned()]),
            Some(fallback.to_str().unwrap()),
        )
        .unwrap();
        assert_eq!(plan.workspaces[0].destination, fallback.join(id));
        let config = &plan
            .agents
            .agents
            .iter()
            .find(|agent| agent.id == id)
            .unwrap()
            .config;
        assert_eq!(
            config["project_dir"],
            json!(fallback.join(id).join("project").to_string_lossy())
        );
        assert_eq!(
            config["description"],
            archive.agents[0].config["description"]
        );
        assert_eq!(
            plan.workspaces[0].source_root,
            archive.agents[0].workspace_dir
        );
        assert!(!fallback.exists());
    }
}

#[test]
fn full_registry_removal_does_not_authorize_overwriting_the_removed_agents_directory() {
    let mut fixture = Fixture::new();
    let base = fixture
        .directory
        .path()
        .canonicalize()
        .unwrap()
        .join("placement");
    let path = base.join("new");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("protected"), "unselected Agent").unwrap();
    let recent = fixture.catalog.agents.get_mut("recent").unwrap();
    let key = fixture
        .catalog
        .workspace_keys
        .remove(&recent.workspace_dir)
        .unwrap();
    recent.workspace_dir = path.to_string_lossy().into_owned();
    recent.config["workspace_dir"] = json!(recent.workspace_dir);
    fixture
        .catalog
        .workspace_keys
        .insert(recent.workspace_dir.clone(), key);
    write_catalog(&fixture.desktop, &fixture.catalog).unwrap();
    let archive = Fixture::archive("new");
    let registry = Fixture::registry(&archive);
    for selected in [BTreeSet::new(), BTreeSet::from([String::from("new")])] {
        assert_eq!(
            plan_restore(
                &fixture.desktop,
                &archive,
                Some(&registry),
                &selected,
                Some(base.to_str().unwrap())
            )
            .unwrap_err(),
            "Restore workspace overlaps an unselected Agent"
        );
    }
    assert_eq!(
        fs::read_to_string(path.join("protected")).unwrap(),
        "unselected Agent"
    );
}

#[test]
fn rejects_control_destinations_but_allows_workspaces_inside_the_protected_data_subtree() {
    let fixture = Fixture::new();
    let archive = Fixture::archive("new");
    let selected = BTreeSet::from([String::from("new")]);
    assert_eq!(
        plan_restore(
            &fixture.desktop,
            &archive,
            None,
            &selected,
            Some(fixture.desktop.data_dir.to_str().unwrap())
        )
        .unwrap_err(),
        "Restore workspace overlaps Core control data"
    );
    let plan = plan_restore(&fixture.desktop, &archive, None, &selected, None).unwrap();
    assert_eq!(
        plan.workspaces[0].destination,
        fixture.desktop.data_dir.join("workspaces/new")
    );
    let default = Fixture::archive("default");
    assert!(
        plan_restore(
            &fixture.desktop,
            &default,
            None,
            &BTreeSet::from([String::from("default")]),
            None
        )
        .is_ok()
    );
}

#[cfg(unix)]
#[test]
fn resolves_symlink_aliases_before_checking_agent_collisions() {
    let fixture = Fixture::new();
    let base = fixture
        .directory
        .path()
        .canonicalize()
        .unwrap()
        .join("placement");
    fs::create_dir(&base).unwrap();
    std::os::unix::fs::symlink(
        &fixture.catalog.agents["recent"].workspace_dir,
        base.join("new"),
    )
    .unwrap();
    assert_eq!(
        plan_restore(
            &fixture.desktop,
            &Fixture::archive("new"),
            None,
            &BTreeSet::from([String::from("new")]),
            Some(base.to_str().unwrap())
        )
        .unwrap_err(),
        "Restore workspace overlaps an unselected Agent"
    );
}

#[test]
fn invalid_global_references_and_inconsistent_selected_snapshots_are_rejected() {
    let fixture = Fixture::new();
    let archived = Fixture::archive("writer");
    let selected = BTreeSet::from([String::from("writer")]);
    for kind in [
        "version",
        "missing-default",
        "case-alias",
        "flags",
        "mismatch",
    ] {
        let mut registry = Fixture::registry(&archived);
        match kind {
            "version" => registry.version = 2,
            "missing-default" => {
                registry.agents.remove(0);
            }
            "case-alias" => {
                let mut alias = registry.agents[1].clone();
                alias.id = String::from("Writer");
                registry.agents.push(alias);
            }
            "flags" => registry.agents[0].enabled = false,
            _ => registry.agents[1].workspace_dir = String::from("D:\\different\\writer"),
        }
        assert!(
            plan_restore(
                &fixture.desktop,
                &archived,
                Some(&registry),
                &selected,
                None
            )
            .is_err(),
            "{kind}"
        );
    }
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(catalog_path(&fixture.desktop)).unwrap())
            .unwrap(),
        serde_json::to_value(&fixture.catalog).unwrap()
    );
}

#[test]
fn combined_custom_registry_is_bounded_and_unknown_requested_ids_do_not_grant_scope() {
    let fixture = Fixture::new();
    let mut archived = AgentBackupSnapshot {
        version: 1,
        agents: Vec::new(),
    };
    let mut selected = BTreeSet::new();
    for number in 0..254 {
        let id = format!("new-{number}");
        archived.agents.extend(Fixture::archive(&id).agents);
        selected.insert(id);
    }
    assert!(plan_restore(&fixture.desktop, &archived, None, &selected, None).is_err());
    let empty = plan_restore(
        &fixture.desktop,
        &Fixture::archive("writer"),
        None,
        &BTreeSet::from([String::from("not-in-archive")]),
        None,
    )
    .unwrap();
    let mut expected = fixture.catalog.clone();
    expected.revision += 1;
    assert_eq!(
        serde_json::from_slice::<Value>(&empty.catalog).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert!(empty.workspaces.is_empty());
    assert!(empty.removed_agent_ids.is_empty());
}

#[test]
fn fallback_parent_traversal_is_rejected_without_creating_directories() {
    let fixture = Fixture::new();
    let base = fixture.directory.path().join("missing/../escape");
    assert_eq!(
        plan_restore(
            &fixture.desktop,
            &Fixture::archive("new"),
            None,
            &BTreeSet::from([String::from("new")]),
            Some(base.to_str().unwrap())
        )
        .unwrap_err(),
        "Restore workspace path is invalid"
    );
    assert!(!fixture.directory.path().join("missing").exists());
    assert!(!fixture.directory.path().join("escape").exists());
}
