use std::collections::BTreeMap;
use std::io::Cursor;

use serde_json::Value;
use serde_json::json;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use desktop_agents::AgentBackupSnapshot;
use desktop_agents::BackupAgent;

use super::*;

struct Fixture {
    directory: tempfile::TempDir,
    desktop: DesktopWorkspace,
}

impl Fixture {
    fn new(control: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap().join("default");
        let data_dir = root.join(control);
        fs::create_dir_all(data_dir.join("agents")).unwrap();
        fs::write(data_dir.join("database"), "live Core data").unwrap();
        let mut agents = serde_json::Map::new();
        let mut workspace_keys = serde_json::Map::new();
        for id in ["default", "writer", "other"] {
            let workspace = if id == "default" {
                root.clone()
            } else {
                data_dir.join("workspaces").join(id)
            };
            fs::create_dir_all(&workspace).unwrap();
            fs::write(workspace.join("notes"), format!("{id} local")).unwrap();
            let key = json!({"kind":"legacy_agent","id":id});
            fs::write(
                workspace.join(MARKER_NAME),
                serde_json::to_vec(&key).unwrap(),
            )
            .unwrap();
            workspace_keys.insert(workspace.to_string_lossy().into_owned(), key.clone());
            agents.insert(
                id.to_owned(),
                json!({"workspace_dir": workspace, "data_key":key,
                "enabled": true, "pinned": id == "default",
                "config": {"id": id, "workspace_dir": workspace, "name": format!("local {id}")}}),
            );
        }
        let desktop = DesktopWorkspace {
            data_dir,
            initial: root.clone(),
            selected: tokio::sync::RwLock::new(root),
        };
        fs::write(desktop_agents::catalog_path(&desktop), serde_json::to_vec(&json!({
            "schema_version": 3, "revision": 1, "order": ["default", "writer", "other"], "agents": agents, "workspace_keys":workspace_keys
        })).unwrap()).unwrap();
        Self { directory, desktop }
    }

    fn plan(&self, ids: &[&str], fallback: Option<&str>) -> AgentRestorePlan {
        let archived = AgentBackupSnapshot {
            version: 1,
            agents: ids
                .iter()
                .map(|id| {
                    let source = format!("C:\\source\\{id}");
                    BackupAgent {
                        data_key: None,
                        id: (*id).to_owned(),
                        workspace_dir: source.clone(),
                        enabled: true,
                        pinned: *id == "default",
                        config: json!({"id": id, "workspace_dir": source,
                    "name": format!("archived {id}")}),
                    }
                })
                .collect(),
        };
        desktop_agents::restore::plan_restore(
            &self.desktop,
            &archived,
            None,
            &ids.iter().map(|id| (*id).to_owned()).collect(),
            fallback,
        )
        .unwrap()
    }

    fn tree(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        tree(self.directory.path())
    }
}

fn archive(files: &[(&str, &[u8])]) -> (ZipArchive<Cursor<Vec<u8>>>, Manifest) {
    archive_with_prefix(super::super::WORKSPACE_PREFIX, files)
}

fn archive_with_prefix(
    prefix: &str,
    files: &[(&str, &[u8])],
) -> (ZipArchive<Cursor<Vec<u8>>>, Manifest) {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let mut manifest = Manifest {
        format: super::super::FORMAT_VERSION.to_owned(),
        entries: BTreeMap::new(),
    };
    for (name, bytes) in files {
        super::super::write_bytes(
            &mut writer,
            &format!("{prefix}{name}"),
            bytes,
            SimpleFileOptions::default(),
            &mut manifest,
        )
        .unwrap();
    }
    (ZipArchive::new(writer.finish().unwrap()).unwrap(), manifest)
}

#[test]
fn global_and_skill_staging_roll_back_together_without_touching_protected_data() {
    let fixture = Fixture::new(".core");
    let data = &fixture.desktop.data_dir;
    fs::create_dir_all(data.join("settings")).unwrap();
    fs::write(data.join("settings/ui.json"), b"old UI").unwrap();
    fs::write(
        data.join("settings/local.json"),
        b"retain omitted global file",
    )
    .unwrap();
    let recovery = data
        .join("skill_pool")
        .join(format!("{RECOVERY_PREFIX}retained"));
    fs::create_dir_all(&recovery).unwrap();
    fs::write(recovery.join("original"), b"retained recovery").unwrap();
    fs::write(data.join("skill_pool/old.md"), b"old skill").unwrap();
    let before = fixture.tree();
    let (mut archive, manifest) = archive_with_prefix(
        "data/",
        &[
            ("config/settings/ui.json", b"new UI"),
            ("skill_pool/new/SKILL.md", b"new skill"),
        ],
    );
    let mut transaction = RestoreFiles::default();
    stage_globals(
        &fixture.desktop,
        &mut archive,
        &manifest,
        None,
        &mut transaction,
    )
    .unwrap();
    stage_skill_pool(&fixture.desktop, &mut archive, &manifest, &mut transaction).unwrap();
    assert_eq!(fs::read(data.join("settings/ui.json")).unwrap(), b"old UI");
    transaction.apply().unwrap();
    assert_eq!(fs::read(data.join("settings/ui.json")).unwrap(), b"new UI");
    assert_eq!(
        fs::read(data.join("settings/local.json")).unwrap(),
        b"retain omitted global file"
    );
    assert_eq!(
        fs::read(data.join("skill_pool/new/SKILL.md")).unwrap(),
        b"new skill"
    );
    assert!(!data.join("skill_pool/old.md").exists());
    assert_eq!(
        fs::read(recovery.join("original")).unwrap(),
        b"retained recovery"
    );
    transaction.rollback().unwrap();
    drop(transaction);
    assert_eq!(fixture.tree(), before);
}

#[test]
fn global_staging_rejects_control_and_recovery_targets_before_mutation() {
    for name in [
        "backups/archive.json",
        "agents/catalog.json",
        "workspaces/agent.json",
        "skill_pool/skill.json",
        "local-models/runtime.json",
        "checkpoints/graph.json",
        "threads.sqlite3",
        ".qwenpaw-restore-recovery-fixture/config.json",
    ] {
        let fixture = Fixture::new(".core");
        let before = fixture.tree();
        let (mut archive, manifest) = archive_with_prefix("data/config/", &[(name, b"untrusted")]);
        let mut transaction = RestoreFiles::default();
        assert!(
            stage_globals(
                &fixture.desktop,
                &mut archive,
                &manifest,
                None,
                &mut transaction
            )
            .is_err(),
            "{name}"
        );
        drop(transaction);
        assert_eq!(fixture.tree(), before, "{name}");
    }
}

#[test]
fn omitted_global_and_skill_payloads_do_not_clear_existing_files() {
    let fixture = Fixture::new(".core");
    fs::create_dir_all(fixture.desktop.data_dir.join("skill_pool")).unwrap();
    fs::write(
        fixture.desktop.data_dir.join("skill_pool/keep"),
        b"keep skill",
    )
    .unwrap();
    let before = fixture.tree();
    let (mut archive, manifest) = archive_with_prefix("data/", &[]);
    let mut transaction = RestoreFiles::default();
    stage_globals(
        &fixture.desktop,
        &mut archive,
        &manifest,
        None,
        &mut transaction,
    )
    .unwrap();
    stage_skill_pool(&fixture.desktop, &mut archive, &manifest, &mut transaction).unwrap();
    transaction.apply().unwrap();
    transaction.commit();
    drop(transaction);
    assert_eq!(fixture.tree(), before);
}

fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                result.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    result
}

#[test]
fn stages_multiple_agents_and_catalog_then_rolls_back_without_touching_live_core_files() {
    for control in [".core", "state/runtime"] {
        let fixture = Fixture::new(control);
        let before = fixture.tree();
        let plan = fixture.plan(&["default", "writer"], None);
        let (mut archive, manifest) = archive(&[
            ("default/new/file", b"restored default"),
            ("writer/new", b"restored writer"),
        ]);
        let mut transaction = RestoreFiles::default();
        stage_agents(
            &fixture.desktop,
            &plan,
            &mut archive,
            &manifest,
            &mut transaction,
        )
        .unwrap();
        assert_eq!(
            fs::read(fixture.desktop.initial.join("notes")).unwrap(),
            b"default local"
        );
        transaction.apply().unwrap();
        assert!(!fixture.desktop.initial.join("notes").exists());
        assert_eq!(
            fs::read(fixture.desktop.initial.join("new/file")).unwrap(),
            b"restored default"
        );
        assert_eq!(
            fs::read(fixture.desktop.data_dir.join("workspaces/writer/new")).unwrap(),
            b"restored writer"
        );
        assert_eq!(
            fs::read(fixture.desktop.data_dir.join("workspaces/other/notes")).unwrap(),
            b"other local"
        );
        assert_eq!(
            fs::read(fixture.desktop.data_dir.join("database")).unwrap(),
            b"live Core data"
        );
        assert_eq!(
            fs::read(desktop_agents::catalog_path(&fixture.desktop)).unwrap(),
            plan.catalog
        );
        transaction.rollback().unwrap();
        drop(transaction);
        assert_eq!(fixture.tree(), before);
    }
}

#[test]
fn committed_restore_keeps_only_archived_workspace_files_and_preserves_nested_recovery_data() {
    let fixture = Fixture::new("state/runtime");
    let root = &fixture.desktop.initial;
    let recovery = root.join("nested/.qwenpaw-restore-retained");
    fs::create_dir_all(&recovery).unwrap();
    fs::write(recovery.join("original"), "must survive").unwrap();
    fs::write(root.join("nested/obsolete"), "replace this").unwrap();
    fs::write(root.join("state/obsolete"), "not Core data").unwrap();
    let mut expected = fixture.tree();
    for name in [
        "default/notes",
        "default/nested/obsolete",
        "default/state/obsolete",
    ] {
        expected.remove(Path::new(name));
    }
    let plan = fixture.plan(&["default"], None);
    expected.insert(
        PathBuf::from("default/nested/current"),
        b"new content".to_vec(),
    );
    expected.insert(
        desktop_agents::catalog_path(&fixture.desktop)
            .strip_prefix(fixture.directory.path().canonicalize().unwrap())
            .unwrap()
            .to_path_buf(),
        plan.catalog.clone(),
    );
    let (mut archive, manifest) = archive(&[("default/nested/current", b"new content")]);
    let mut transaction = RestoreFiles::default();
    stage_agents(
        &fixture.desktop,
        &plan,
        &mut archive,
        &manifest,
        &mut transaction,
    )
    .unwrap();
    transaction.apply().unwrap();
    transaction.commit();
    drop(transaction);
    assert_eq!(fixture.tree(), expected);
}

#[test]
fn explicit_empty_workspace_removes_old_contents_but_not_other_agents() {
    let fixture = Fixture::new(".core");
    let before = fixture.tree();
    let plan = fixture.plan(&["writer"], None);
    let (mut archive, manifest) = archive(&[]);
    let mut transaction = RestoreFiles::default();
    stage_agents(
        &fixture.desktop,
        &plan,
        &mut archive,
        &manifest,
        &mut transaction,
    )
    .unwrap();
    transaction.apply().unwrap();
    let writer = fixture.desktop.data_dir.join("workspaces/writer");
    assert!(writer.is_dir());
    assert_eq!(
        fs::read_dir(&writer)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>(),
        vec![std::ffi::OsString::from(MARKER_NAME)]
    );
    assert_eq!(
        fs::read(writer.join(MARKER_NAME)).unwrap(),
        plan.identity_markers[&writer]
    );
    drop(transaction);
    assert_eq!(fixture.tree(), before);
}

#[test]
fn archive_identity_cannot_override_the_local_restore_binding() {
    let fixture = Fixture::new(".core");
    let plan = fixture.plan(&["writer"], None);
    let before = fixture.tree();
    for name in [MARKER_NAME.to_owned(), MARKER_NAME.to_uppercase()] {
        let archive_name = format!("writer/{name}");
        let (mut archive, manifest) = archive(&[(&archive_name, b"foreign identity")]);
        let mut transaction = RestoreFiles::default();
        assert!(
            stage_agents(
                &fixture.desktop,
                &plan,
                &mut archive,
                &manifest,
                &mut transaction
            )
            .is_err()
        );
        drop(transaction);
        assert_eq!(fixture.tree(), before);
    }
}

#[test]
fn new_workspace_parents_are_removed_on_rollback_but_retained_on_commit() {
    let fixture = Fixture::new(".core");
    let fallback = fixture
        .directory
        .path()
        .canonicalize()
        .unwrap()
        .join("new-parent/nested");
    let plan = fixture.plan(&["new-agent"], Some(fallback.to_str().unwrap()));
    let before = fixture.tree();
    for commit in [false, true] {
        let (mut archive, manifest) = archive(&[("new-agent/notes", b"new agent")]);
        let mut transaction = RestoreFiles::default();
        stage_agents(
            &fixture.desktop,
            &plan,
            &mut archive,
            &manifest,
            &mut transaction,
        )
        .unwrap();
        assert!(!fallback.join("new-agent").exists());
        transaction.apply().unwrap();
        assert_eq!(
            fs::read(fallback.join("new-agent/notes")).unwrap(),
            b"new agent"
        );
        if commit {
            transaction.commit();
        }
        drop(transaction);
        if commit {
            let catalog: Value = serde_json::from_slice(
                &fs::read(desktop_agents::catalog_path(&fixture.desktop)).unwrap(),
            )
            .unwrap();
            assert_eq!(
                catalog["agents"]["new-agent"]["workspace_dir"],
                json!(fallback.join("new-agent"))
            );
        } else {
            assert!(!fixture.directory.path().join("new-parent").exists());
            assert_eq!(fixture.tree(), before);
        }
    }
}

#[test]
fn all_workspace_targets_are_preflighted_and_protected_archive_paths_are_rejected() {
    for (control, name) in [
        (".core", "default/.CORE/database"),
        ("state/runtime", "default/state"),
        (".core", "default/nested/.qwenpaw-restore-old/original"),
    ] {
        let fixture = Fixture::new(control);
        let before = fixture.tree();
        let plan = fixture.plan(&["writer", "default"], None);
        let (mut archive, manifest) = archive(&[("writer/new", b"safe"), (name, b"unsafe")]);
        let mut transaction = RestoreFiles::default();
        assert!(
            stage_agents(
                &fixture.desktop,
                &plan,
                &mut archive,
                &manifest,
                &mut transaction
            )
            .is_err()
        );
        assert_eq!(fixture.tree(), before);
        drop(transaction);
        assert_eq!(fixture.tree(), before);
    }
}

#[test]
fn digest_failure_after_another_agent_is_staged_cleans_only_staging() {
    let fixture = Fixture::new(".core");
    let before = fixture.tree();
    let plan = fixture.plan(&["writer", "default"], None);
    let (mut archive, mut manifest) = archive(&[
        ("writer/new", b"new writer"),
        ("default/new", b"new default"),
    ]);
    manifest
        .entries
        .get_mut("data/workspaces/default/new")
        .unwrap()
        .sha256 = "0".repeat(64);
    let mut transaction = RestoreFiles::default();
    assert!(
        stage_agents(
            &fixture.desktop,
            &plan,
            &mut archive,
            &manifest,
            &mut transaction
        )
        .is_err()
    );
    assert_eq!(
        fs::read(fixture.desktop.data_dir.join("workspaces/writer/notes")).unwrap(),
        b"writer local"
    );
    drop(transaction);
    assert_eq!(fixture.tree(), before);
}

#[test]
fn late_catalog_swap_failure_rolls_back_already_exchanged_workspace_trees() {
    let fixture = Fixture::new(".core");
    let before = fixture.tree();
    let plan = fixture.plan(&["writer", "default"], None);
    let (mut archive, manifest) = archive(&[
        ("writer/new", b"new writer"),
        ("default/new", b"new default"),
    ]);
    let mut transaction = RestoreFiles::default();
    stage_agents(
        &fixture.desktop,
        &plan,
        &mut archive,
        &manifest,
        &mut transaction,
    )
    .unwrap();
    let recovery = fs::read_dir(fixture.desktop.data_dir.join("agents"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(RECOVERY_PREFIX)
        })
        .unwrap();
    fs::remove_file(recovery.join("replacement")).unwrap();
    assert!(transaction.apply().is_err());
    drop(transaction);
    assert_eq!(fixture.tree(), before);
}

#[test]
fn conflicting_file_directory_payloads_fail_before_staging() {
    let fixture = Fixture::new(".core");
    let before = fixture.tree();
    let plan = fixture.plan(&["writer"], None);
    let (mut archive, manifest) = archive(&[
        ("writer/conflict", b"file"),
        ("writer/conflict/child", b"child"),
    ]);
    let mut transaction = RestoreFiles::default();
    assert!(
        stage_agents(
            &fixture.desktop,
            &plan,
            &mut archive,
            &manifest,
            &mut transaction
        )
        .is_err()
    );
    assert_eq!(fixture.tree(), before);
}

#[test]
fn agent_config_file_remaps_only_known_workspace_paths_and_preserves_other_content() {
    let fixture = Fixture::new(".core");
    let before = fixture.tree();
    let plan = fixture.plan(&["writer"], None);
    let destination = fixture.desktop.data_dir.join("workspaces/writer");
    let original = json!({"id": "writer", "workspace_dir": "C:\\source\\writer",
        "project_dir": "C:\\source\\writer\\project", "description": "Keep C:\\source\\writer",
        "enabled": false, "custom": {"text": "C:\\source\\writer\\project"}});
    let bytes = serde_json::to_vec(&original).unwrap();
    let (mut archive, manifest) = archive(&[("writer/agent.json", &bytes)]);
    let mut transaction = RestoreFiles::default();
    stage_agents(
        &fixture.desktop,
        &plan,
        &mut archive,
        &manifest,
        &mut transaction,
    )
    .unwrap();
    transaction.apply().unwrap();
    let restored: Value =
        serde_json::from_slice(&fs::read(destination.join("agent.json")).unwrap()).unwrap();
    let mut expected = original;
    expected["workspace_dir"] = json!(destination);
    expected["project_dir"] = json!(destination.join("project"));
    assert_eq!(restored, expected);
    drop(transaction);
    assert_eq!(fixture.tree(), before);
}

#[test]
fn malformed_agent_config_cannot_panic_or_overwrite_the_live_workspace() {
    let fixture = Fixture::new(".core");
    let before = fixture.tree();
    let plan = fixture.plan(&["writer"], None);
    for bytes in [b"[1,2]".as_slice(), b"not JSON".as_slice()] {
        let (mut archive, manifest) = archive(&[("writer/agent.json", bytes)]);
        let mut transaction = RestoreFiles::default();
        assert!(
            stage_agents(
                &fixture.desktop,
                &plan,
                &mut archive,
                &manifest,
                &mut transaction
            )
            .is_err()
        );
        drop(transaction);
        assert_eq!(fixture.tree(), before);
    }
}

#[cfg(unix)]
#[test]
fn workspace_links_are_exchanged_without_writing_or_deleting_their_external_targets() {
    let fixture = Fixture::new(".core");
    let outside = fixture.directory.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep"), "outside data").unwrap();
    let removed = fixture.desktop.initial.join("removed-link");
    let replaced = fixture.desktop.initial.join("replaced-link");
    std::os::unix::fs::symlink(&outside, &removed).unwrap();
    std::os::unix::fs::symlink(fixture.desktop.data_dir.join("database"), &replaced).unwrap();
    let plan = fixture.plan(&["default"], None);
    for commit in [false, true] {
        let (mut archive, manifest) =
            archive(&[("default/replaced-link", b"ordinary restored file")]);
        let mut transaction = RestoreFiles::default();
        stage_agents(
            &fixture.desktop,
            &plan,
            &mut archive,
            &manifest,
            &mut transaction,
        )
        .unwrap();
        transaction.apply().unwrap();
        assert!(fs::symlink_metadata(&removed).is_err());
        assert_eq!(fs::read(&replaced).unwrap(), b"ordinary restored file");
        if commit {
            transaction.commit();
        }
        drop(transaction);
        if !commit {
            assert_eq!(fs::read_link(&removed).unwrap(), outside);
            assert_eq!(
                fs::read_link(&replaced).unwrap(),
                fixture.desktop.data_dir.join("database")
            );
        }
        assert_eq!(fs::read(outside.join("keep")).unwrap(), b"outside data");
        assert_eq!(
            fs::read(fixture.desktop.data_dir.join("database")).unwrap(),
            b"live Core data"
        );
    }
}
