use super::*;
use pretty_assertions::assert_eq;

#[path = "desktop_publication_file_process_tests.rs"]
mod process;

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    config: PathBuf,
    catalog: PathBuf,
    receipt: PathBuf,
    installation: Uuid,
    transaction: Uuid,
    digest: [u8; 32],
}

impl Fixture {
    fn new(original: bool) -> (Self, AgentPublicationFiles) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        Self::at(directory, root, original)
    }

    fn at(
        directory: tempfile::TempDir,
        root: PathBuf,
        original: bool,
    ) -> (Self, AgentPublicationFiles) {
        fs::create_dir_all(root.join("workspace")).unwrap();
        fs::create_dir_all(root.join("data/agents")).unwrap();
        let mut fixture = Self {
            config: root.join("workspace/agent.json"),
            catalog: root.join("data/agents/catalog.json"),
            receipt: root.join("publication.json"),
            root,
            _directory: directory,
            installation: Uuid::now_v7(),
            transaction: Uuid::now_v7(),
            digest: [0; 32],
        };
        if original {
            fs::write(&fixture.config, b"old config").unwrap();
            fs::write(&fixture.catalog, b"old catalog").unwrap();
        }
        let mut files = AgentPublicationFiles::prepare(
            fixture.installation,
            fixture.transaction,
            "writer",
            &fixture.config,
            b"new config",
            &fixture.catalog,
            b"new catalog",
        )
        .unwrap();
        assert!(files.apply().is_err());
        fixture.digest = files.persist(&fixture.receipt).unwrap();
        (fixture, files)
    }

    fn read(&self) -> io::Result<AgentPublicationFiles> {
        AgentPublicationFiles::read(
            &self.receipt,
            self.digest,
            self.installation,
            self.transaction,
            &self.catalog,
        )
    }

    fn originals(&self, present: bool) {
        for (path, content) in [
            (&self.config, b"old config".as_slice()),
            (&self.catalog, b"old catalog".as_slice()),
        ] {
            if present {
                assert_eq!(fs::read(path).unwrap(), content);
            } else {
                assert!(!path.exists());
            }
            assert_eq!(
                fs::read_dir(path.parent().unwrap()).unwrap().count(),
                usize::from(present)
            );
        }
    }
}

#[test]
fn publication_files_reopen_recovers_every_forward_rename_boundary() {
    for present in [false, true] {
        let total = if present { 4 } else { 2 };
        for boundary in 0..=total {
            let (fixture, files) = Fixture::new(present);
            let mut operations = Vec::new();
            for entry in &files.journal.entries {
                if present {
                    operations.push((entry.target.path(), entry.recovery().join("original")));
                }
                operations.push((entry.recovery().join("replacement"), entry.target.path()));
            }
            for (source, destination) in operations.into_iter().take(boundary) {
                move_file(&source, &destination).unwrap();
            }
            drop(files);
            let reopened = fixture.read().unwrap();
            assert_eq!(reopened.agent_id(), "writer");
            reopened.rollback().unwrap();
            reopened.rollback().unwrap();
            reopened.cleanup(AgentPublicationState::Prepared).unwrap();
            reopened.cleanup(AgentPublicationState::Prepared).unwrap();
            fixture.originals(present);
        }
    }
}

#[test]
fn publication_files_commit_reopen_keeps_new_values_and_cleanup_is_repeatable() {
    for present in [false, true] {
        let (fixture, files) = Fixture::new(present);
        files.apply().unwrap();
        assert!(files.apply().is_err());
        assert!(files.cleanup(AgentPublicationState::Prepared).is_err());
        drop(files);
        let reopened = fixture.read().unwrap();
        reopened.cleanup(AgentPublicationState::Committed).unwrap();
        fixture
            .read()
            .unwrap()
            .cleanup(AgentPublicationState::Committed)
            .unwrap();
        assert_eq!(fs::read(&fixture.config).unwrap(), b"new config");
        assert_eq!(fs::read(&fixture.catalog).unwrap(), b"new catalog");
        for path in [&fixture.config, &fixture.catalog] {
            assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        }
    }
}

#[test]
fn publication_files_reopen_recovers_interrupted_rollback_and_partial_cleanup() {
    for committed in [false, true] {
        for remove_directory in [false, true] {
            let (fixture, files) = Fixture::new(true);
            files.apply().unwrap();
            if !committed {
                let entry = &files.journal.entries[1];
                move_file(&entry.target.path(), &entry.recovery().join("replacement")).unwrap();
                fixture.read().unwrap().rollback().unwrap();
            }
            let directory = files.journal.entries[0].recovery();
            fs::remove_file(directory.join(if committed { "original" } else { "replacement" }))
                .unwrap();
            if remove_directory {
                fs::remove_dir(directory).unwrap();
            }
            drop(files);
            let reopened = fixture.read().unwrap();
            let state = if committed {
                AgentPublicationState::Committed
            } else {
                AgentPublicationState::Prepared
            };
            reopened.cleanup(state).unwrap();
            reopened.cleanup(state).unwrap();
            if committed {
                assert_eq!(fs::read(&fixture.config).unwrap(), b"new config");
                assert_eq!(fs::read(&fixture.catalog).unwrap(), b"new catalog");
            } else {
                fixture.originals(true);
            }
        }
    }
}

#[test]
fn publication_files_independent_same_bytes_replacement_is_not_rolled_back_or_cleaned() {
    let (fixture, files) = Fixture::new(true);
    files.apply().unwrap();
    let retained = fixture.root.join("owned-new-config");
    fs::rename(&fixture.config, &retained).unwrap();
    fs::write(&fixture.config, b"new config").unwrap();
    drop(files);
    let reopened = fixture.read().unwrap();
    assert!(reopened.rollback().is_err());
    assert!(reopened.cleanup(AgentPublicationState::Prepared).is_err());
    assert!(reopened.cleanup(AgentPublicationState::Committed).is_err());
    assert_eq!(fs::read(&fixture.config).unwrap(), b"new config");
    assert_eq!(fs::read(&fixture.catalog).unwrap(), b"old catalog");
    fs::remove_file(&fixture.config).unwrap();
    fs::rename(retained, &fixture.config).unwrap();
    reopened.rollback().unwrap();
    reopened.cleanup(AgentPublicationState::Prepared).unwrap();
    fixture.originals(true);
}

#[test]
fn publication_files_unknown_staging_and_modified_original_are_preserved() {
    for unknown in [true, false] {
        let (fixture, files) = Fixture::new(true);
        files.apply().unwrap();
        let recovery = files.journal.entries[0].recovery();
        let path = recovery.join(if unknown { "independent" } else { "original" });
        fs::write(&path, b"independent contents").unwrap();
        drop(files);
        let reopened = fixture.read().unwrap();
        assert!(reopened.rollback().is_err());
        assert!(reopened.cleanup(AgentPublicationState::Committed).is_err());
        assert_eq!(fs::read(path).unwrap(), b"independent contents");
        assert_eq!(fs::read(&fixture.config).unwrap(), b"new config");
    }
}

#[test]
fn publication_files_changed_parent_and_recovery_directory_are_not_owned() {
    for parent in [false, true] {
        let (fixture, files) = Fixture::new(true);
        files.apply().unwrap();
        let original = if parent {
            fixture.config.parent().unwrap().to_path_buf()
        } else {
            files.journal.entries[0].recovery()
        };
        let moved = fixture.root.join("displaced");
        fs::rename(&original, &moved).unwrap();
        fs::create_dir(&original).unwrap();
        fs::write(original.join("independent"), b"keep").unwrap();
        let reopened = fixture.read().unwrap();
        assert!(reopened.rollback().is_err());
        assert!(reopened.cleanup(AgentPublicationState::Committed).is_err());
        assert_eq!(fs::read(original.join("independent")).unwrap(), b"keep");
        assert!(moved.exists());
    }
}

#[test]
fn publication_files_receipt_rejects_tampering_foreign_binding_and_overwrite() {
    let (fixture, mut files) = Fixture::new(true);
    let bytes = fs::read(&fixture.receipt).unwrap();
    assert!(files.persist(&fixture.receipt).is_err());
    files.persisted = false;
    assert!(files.persist(&fixture.receipt).is_err());
    assert_eq!(fs::read(&fixture.receipt).unwrap(), bytes);
    for (installation, transaction, catalog) in [
        (Uuid::now_v7(), fixture.transaction, fixture.catalog.clone()),
        (
            fixture.installation,
            Uuid::now_v7(),
            fixture.catalog.clone(),
        ),
        (
            fixture.installation,
            fixture.transaction,
            fixture.config.clone(),
        ),
    ] {
        assert!(
            AgentPublicationFiles::read(
                &fixture.receipt,
                fixture.digest,
                installation,
                transaction,
                &catalog
            )
            .is_err()
        );
    }
    fs::write(&fixture.receipt, b"{}").unwrap();
    assert!(fixture.read().is_err());
    assert_eq!(fs::read(&fixture.config).unwrap(), b"old config");
    files.cleanup(AgentPublicationState::Prepared).unwrap();
    fixture.originals(true);
}

#[test]
fn publication_files_initial_external_edit_rejects_apply_without_touching_the_other_file() {
    let (fixture, files) = Fixture::new(true);
    fs::write(&fixture.catalog, b"independent catalog").unwrap();
    assert!(files.apply().is_err());
    assert_eq!(fs::read(&fixture.config).unwrap(), b"old config");
    assert_eq!(fs::read(&fixture.catalog).unwrap(), b"independent catalog");
    assert!(files.rollback().is_err());
}

#[cfg(unix)]
#[test]
fn publication_files_unicode_path_roundtrips_and_symlink_receipts_are_rejected() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap().join("工作区");
    let (fixture, files) = Fixture::at(directory, root, true);
    files.apply().unwrap();
    drop(files);
    fixture.read().unwrap().rollback().unwrap();
    fixture
        .read()
        .unwrap()
        .cleanup(AgentPublicationState::Prepared)
        .unwrap();
    fixture.originals(true);
    let retained = fixture.root.join("retained-journal");
    fs::rename(&fixture.receipt, &retained).unwrap();
    symlink(retained, &fixture.receipt).unwrap();
    assert!(fixture.read().is_err());
}

#[cfg(unix)]
#[test]
fn publication_files_native_path_codec_preserves_non_unicode_bytes_without_filesystem_assumptions()
{
    use std::os::unix::ffi::OsStringExt as _;
    let path = PathBuf::from(std::ffi::OsString::from_vec(vec![b'/', b'w', 0xff]));
    let bytes = serde_json::to_vec(&NativePath::from_path(&path)).unwrap();
    assert_eq!(
        serde_json::from_slice::<NativePath>(&bytes).unwrap().path(),
        path
    );
}
