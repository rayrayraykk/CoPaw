use qwenpaw_protocol::Thread;
use qwenpaw_protocol::ThreadStatus;
use qwenpaw_protocol::Turn;
use qwenpaw_protocol::TurnStatus;
use qwenpaw_storage::StoredMessage;
use serde_json::json;
use sha2::Digest as _;
use sha2::Sha256;

use super::super::CheckpointState;
use super::*;

struct Fixture {
    _directory: tempfile::TempDir,
    workspace: WorkspaceRestore,
    control: PathBuf,
    staged: PathBuf,
}

impl Fixture {
    fn new(same_root: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let destination = root.join("workspace");
        let control = destination.join(".core");
        fs::create_dir_all(&control).unwrap();
        fs::write(control.join("database"), "live control data").unwrap();
        let staged = root.join("staging");
        fs::create_dir_all(staged.join("snapshots")).unwrap();
        let source_root = if same_root {
            destination.to_string_lossy().into_owned()
        } else {
            String::from("C:\\source\\default")
        };
        let workspace = WorkspaceRestore {
            id: String::from("default"),
            source_root,
            destination,
        };
        Self {
            _directory: directory,
            workspace,
            control,
            staged,
        }
    }

    fn thread(&self) -> ThreadCheckpoint {
        ThreadCheckpoint {
            thread: Thread {
                id: String::from("thread"),
                model: String::from("model"),
                workspace_root: Some(self.workspace.source_root.clone()),
                status: ThreadStatus::Idle,
                archived: false,
                created_at: 1,
                updated_at: 1,
            },
            turns: Vec::new(),
            messages: vec![StoredMessage::text(
                "user",
                format!("Keep {} in history", self.workspace.source_root),
            )],
            turn_metadata: Vec::new(),
        }
    }

    fn add(
        &self,
        name: &str,
        thread: &ThreadCheckpoint,
        extra: Option<(&str, &[u8])>,
        symlink: bool,
    ) -> String {
        self.add_with_identity(name, thread, extra, symlink, self.source_identity())
    }

    fn add_with_identity(
        &self,
        name: &str,
        thread: &ThreadCheckpoint,
        extra: Option<(&str, &[u8])>,
        symlink: bool,
        identity: CheckpointIdentity,
    ) -> String {
        let mut state = super::super::read_state(&self.staged, &self.source_identity()).unwrap();
        let file = tempfile::NamedTempFile::new_in(self.staged.join("snapshots")).unwrap();
        let mut writer = ZipWriter::new(file.reopen().unwrap());
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("thread.json", options).unwrap();
        serde_json::to_writer(&mut writer, thread).unwrap();
        writer.start_file("checkpoint.id", options).unwrap();
        serde_json::to_writer(
            &mut writer,
            &super::super::SnapshotIdentity {
                version: super::super::STATE_VERSION,
                id: uuid::Uuid::now_v7(),
                workspace: identity,
            },
        )
        .unwrap();
        writer.start_file("files/notes.md", options).unwrap();
        writer.write_all(name.as_bytes()).unwrap();
        if let Some((path, bytes)) = extra {
            writer.start_file(path, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        if symlink {
            writer
                .add_symlink("files/link", "/outside", options)
                .unwrap();
        }
        writer.finish().unwrap().sync_all().unwrap();
        let commit = format!("{:x}", Sha256::digest(fs::read(file.path()).unwrap()));
        file.persist_noclobber(self.staged.join("snapshots").join(format!("{commit}.zip")))
            .unwrap();
        let parent_commit = state.heads.get("session").cloned();
        state.entries.push(CheckpointEntry {
            ref_name: format!("refs/snap/session/{name}"),
            kind: String::from("snap"),
            session_key: String::from("session"),
            name: name.to_owned(),
            commit: commit.clone(),
            timestamp_ms: 1,
            subject: name.to_owned(),
            query: Some(self.workspace.source_root.clone()),
            channel: String::from("console"),
            restore_index: None,
            parent_commit,
            user_id: String::from("desktop"),
            session_id: String::from("thread"),
            thread_id: String::from("thread"),
        });
        state.heads.insert(String::from("session"), commit.clone());
        super::super::write_state(&self.staged, &state).unwrap();
        commit
    }

    fn restore(&self, mut budget: u64) -> Result<(), String> {
        rewrite_staged(
            &self.control,
            &self.workspace,
            &self.staged,
            &self.source_identity().data_key,
            &self.context().identity.data_key,
            &BTreeSet::from([String::from("thread")]),
            &mut budget,
        )
    }

    fn context(&self) -> WorkspaceContext {
        WorkspaceContext {
            root: self.workspace.destination.clone(),
            root_text: self.workspace.destination.to_string_lossy().into_owned(),
            state_dir: self.staged.clone(),
            control_dir: self.control.clone(),
            identity: CheckpointIdentity {
                data_key: WorkspaceDataKey::LegacyAgent(String::from("default")),
                workspace_root: self.workspace.destination.to_string_lossy().into_owned(),
            },
        }
    }

    fn source_identity(&self) -> CheckpointIdentity {
        CheckpointIdentity {
            data_key: WorkspaceDataKey::LegacyAgent(String::from("default")),
            workspace_root: self.workspace.source_root.clone(),
        }
    }

    fn files(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut result = BTreeMap::from([(
            PathBuf::from("state.json"),
            fs::read(self.staged.join("state.json")).unwrap(),
        )]);
        for entry in fs::read_dir(self.staged.join("snapshots")).unwrap() {
            let path = entry.unwrap().path();
            result.insert(
                path.strip_prefix(&self.staged).unwrap().to_path_buf(),
                fs::read(path).unwrap(),
            );
        }
        result
    }
}

#[test]
fn rebased_snapshots_update_graph_edges_and_agent_paths_without_changing_history() {
    let fixture = Fixture::new(false);
    let thread = fixture.thread();
    let config = json!({"workspace_dir": fixture.workspace.source_root, "project_dir": format!("{}\\project", fixture.workspace.source_root),
        "description": fixture.workspace.source_root});
    let config_bytes = serde_json::to_vec(&config).unwrap();
    let first = fixture.add(
        "first",
        &thread,
        Some(("files/agent.json", &config_bytes)),
        false,
    );
    let second = fixture.add("second", &thread, None, false);
    let old_state: CheckpointState =
        super::super::read_state(&fixture.staged, &fixture.source_identity()).unwrap();
    fixture.restore(MAX_SNAPSHOT_BYTES).unwrap();
    let state = super::super::read_state(&fixture.staged, &fixture.context().identity).unwrap();
    assert_ne!(state.entries[0].commit, first);
    assert_ne!(state.entries[1].commit, second);
    assert_eq!(
        state.entries[1].parent_commit.as_deref(),
        Some(state.entries[0].commit.as_str())
    );
    assert_eq!(state.heads["session"], state.entries[1].commit);
    for (old, new) in old_state.entries.iter().zip(&state.entries) {
        let mut expected = old.clone();
        expected.commit = new.commit.clone();
        expected.parent_commit = new.parent_commit.clone();
        assert_eq!(
            serde_json::to_value(new).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        let loaded = super::super::load_snapshot_index(&fixture.context(), new).unwrap();
        let mut expected = thread.clone();
        expected.thread.workspace_root =
            Some(fixture.workspace.destination.to_string_lossy().into_owned());
        assert_eq!(loaded.checkpoint, expected);
    }
    assert_eq!(
        fs::read_dir(fixture.staged.join("snapshots"))
            .unwrap()
            .count(),
        2
    );
    let path = fixture
        .staged
        .join("snapshots")
        .join(format!("{}.zip", state.entries[0].commit));
    let mut zip = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let actual: Value = serde_json::from_reader(zip.by_name("files/agent.json").unwrap()).unwrap();
    let mut expected = config;
    expected["workspace_dir"] = json!(fixture.workspace.destination);
    expected["project_dir"] = json!(fixture.workspace.destination.join("project"));
    assert_eq!(actual, expected);
}

#[test]
fn unchanged_local_paths_preserve_original_zip_commits_and_bytes() {
    let fixture = Fixture::new(true);
    let thread = fixture.thread();
    fixture.add("first", &thread, None, false);
    fixture.add("second", &thread, None, false);
    let before = fixture.files();
    fixture.restore(MAX_SNAPSHOT_BYTES).unwrap();
    assert_eq!(fixture.files(), before);
}

#[test]
fn new_workspace_key_rewrites_envelopes_but_preserves_external_thread_projects() {
    let fixture = Fixture::new(false);
    let mut thread = fixture.thread();
    thread.thread.workspace_root = Some(String::from("D:\\external-project"));
    let old = fixture.add("external", &thread, None, false);
    let target_key = WorkspaceDataKey::Workspace(uuid::Uuid::now_v7());
    let mut budget = MAX_SNAPSHOT_BYTES;
    rewrite_staged(
        &fixture.control,
        &fixture.workspace,
        &fixture.staged,
        &fixture.source_identity().data_key,
        &target_key,
        &BTreeSet::from([String::from("thread")]),
        &mut budget,
    )
    .unwrap();
    let mut context = fixture.context();
    context.identity.data_key = target_key;
    let state = super::super::read_state(&fixture.staged, &context.identity).unwrap();
    assert_eq!(state.identity, context.identity);
    assert_ne!(state.entries[0].commit, old);
    assert_eq!(state.heads["session"], state.entries[0].commit);
    let loaded = super::super::load_snapshot_index(&context, &state.entries[0]).unwrap();
    assert_eq!(loaded.checkpoint, thread);
    assert_eq!(
        loaded.file_hashes.keys().collect::<Vec<_>>(),
        vec!["notes.md"]
    );
}

#[test]
fn settings_only_restore_rebinds_identity_and_rejects_an_incorrect_source() {
    let fixture = Fixture::new(true);
    let mut state = CheckpointState::new(fixture.source_identity());
    state.auto_enabled = true;
    super::super::write_state(&fixture.staged, &state).unwrap();
    let before = fixture.files();
    let target_key = WorkspaceDataKey::Workspace(uuid::Uuid::now_v7());
    let mut budget = MAX_SNAPSHOT_BYTES;
    assert!(
        rewrite_staged(
            &fixture.control,
            &fixture.workspace,
            &fixture.staged,
            &target_key,
            &target_key,
            &BTreeSet::new(),
            &mut budget,
        )
        .is_err()
    );
    assert_eq!(fixture.files(), before);
    rewrite_staged(
        &fixture.control,
        &fixture.workspace,
        &fixture.staged,
        &fixture.source_identity().data_key,
        &target_key,
        &BTreeSet::new(),
        &mut budget,
    )
    .unwrap();
    state.identity.data_key = target_key;
    let actual = super::super::read_state(&fixture.staged, &state.identity).unwrap();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(state).unwrap()
    );
}

#[test]
fn managed_agent_checkpoint_files_are_allowed_inside_the_core_workspaces_subtree() {
    let mut fixture = Fixture::new(false);
    fixture.workspace.destination = fixture.control.join("workspaces/writer");
    fixture.add("snapshot", &fixture.thread(), None, false);
    fixture.restore(MAX_SNAPSHOT_BYTES).unwrap();
    let state = super::super::read_state(&fixture.staged, &fixture.context().identity).unwrap();
    let loaded = super::super::load_snapshot_index(&fixture.context(), &state.entries[0]).unwrap();
    assert_eq!(
        loaded.checkpoint.thread.workspace_root,
        Some(fixture.workspace.destination.to_string_lossy().into_owned())
    );
    assert!(loaded.file_hashes.contains_key("notes.md"));
    assert_eq!(
        fs::read(fixture.control.join("database")).unwrap(),
        b"live control data"
    );
}

#[test]
fn nested_paths_links_expansion_and_invalid_turns_fail_without_changing_live_data() {
    for kind in [
        "control",
        "traversal",
        "link",
        "budget",
        "turn",
        "foreign-thread",
        "foreign-root",
        "foreign-key",
    ] {
        let fixture = Fixture::new(false);
        let mut thread = fixture.thread();
        let extra = match kind {
            "control" => Some(("files/.CORE/database", b"unsafe".as_slice())),
            "traversal" => Some(("files/../escape", b"unsafe".as_slice())),
            _ => None,
        };
        match kind {
            "turn" => thread.turns.push(Turn {
                id: String::from("turn"),
                thread_id: String::from("other"),
                status: TurnStatus::Completed,
                items: Vec::new(),
                error: None,
            }),
            "foreign-thread" => thread.thread.id = String::from("other"),
            _ => {}
        }
        let mut identity = fixture.source_identity();
        match kind {
            "foreign-root" => identity.workspace_root = String::from("D:\\outside"),
            "foreign-key" => identity.data_key = WorkspaceDataKey::Workspace(uuid::Uuid::now_v7()),
            _ => {}
        }
        fixture.add_with_identity("snapshot", &thread, extra, kind == "link", identity);
        let before = fixture.files();
        assert!(
            fixture
                .restore(if kind == "budget" {
                    1
                } else {
                    MAX_SNAPSHOT_BYTES
                })
                .is_err(),
            "{kind}"
        );
        assert_eq!(fixture.files(), before);
        assert_eq!(
            fs::read(fixture.control.join("database")).unwrap(),
            b"live control data"
        );
    }
}

#[test]
fn unselected_threads_and_unreferenced_archives_are_rejected_before_rewriting() {
    let fixture = Fixture::new(false);
    fixture.add("snapshot", &fixture.thread(), None, false);
    let before = fixture.files();
    let mut remaining = MAX_SNAPSHOT_BYTES;
    assert!(
        rewrite_staged(
            &fixture.control,
            &fixture.workspace,
            &fixture.staged,
            &fixture.source_identity().data_key,
            &fixture.context().identity.data_key,
            &BTreeSet::new(),
            &mut remaining
        )
        .is_err()
    );
    assert_eq!(fixture.files(), before);
    fs::write(fixture.staged.join("snapshots/orphan.zip"), "unreferenced").unwrap();
    let before = fixture.files();
    assert!(fixture.restore(MAX_SNAPSHOT_BYTES).is_err());
    assert_eq!(fixture.files(), before);
}
