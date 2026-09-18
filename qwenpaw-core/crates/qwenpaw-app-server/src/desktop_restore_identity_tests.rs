//! Independent data must survive failed restore rollback and cleanup.

use super::*;

#[test]
fn restore_identity_changed_target_survives_rollback_and_drop() {
    for same_bytes in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let target = root.join("document");
        fs::write(&target, b"original").unwrap();
        let mut transaction = RestoreFiles::default();
        transaction
            .stage_replace(&target, |p| fs::write(p, b"replacement"))
            .unwrap();
        transaction.apply().unwrap();
        let original = transaction.swaps[0].path("original");
        fs::rename(&target, root.join("displaced")).unwrap();
        let independent = if same_bytes {
            b"replacement".as_slice()
        } else {
            b"independent".as_slice()
        };
        fs::write(&target, independent).unwrap();
        assert!(transaction.rollback().is_err());
        drop(transaction);
        assert_eq!(fs::read(&target).unwrap(), independent);
        assert_eq!(fs::read(original).unwrap(), b"original");
        assert_eq!(fs::read(root.join("displaced")).unwrap(), b"replacement");
    }
}

#[test]
fn restore_identity_in_place_file_and_directory_edits_are_preserved() {
    for tree in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let target = root.join("document");
        fs::write(&target, b"original").unwrap();
        let mut transaction = RestoreFiles::default();
        transaction
            .stage_replace(&target, |p| {
                if tree {
                    fs::create_dir(p)?;
                    fs::write(p.join("item"), b"replacement")
                } else {
                    fs::write(p, b"replacement")
                }
            })
            .unwrap();
        transaction.apply().unwrap();
        let original = transaction.swaps[0].path("original");
        let edited = if tree {
            target.join("item")
        } else {
            target.clone()
        };
        fs::write(&edited, b"independent").unwrap();
        assert!(transaction.rollback().is_err());
        drop(transaction);
        assert_eq!(fs::read(edited).unwrap(), b"independent");
        assert_eq!(fs::read(original).unwrap(), b"original");
    }
}

#[test]
fn restore_identity_committed_cleanup_preserves_unknown_recovery_entries() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let target = root.join("document");
    fs::write(&target, b"original").unwrap();
    let mut transaction = RestoreFiles::default();
    transaction
        .stage_replace(&target, |p| fs::write(p, b"replacement"))
        .unwrap();
    transaction.apply().unwrap();
    let unknown = transaction.swaps[0].path("independent");
    fs::write(&unknown, b"do not clean up").unwrap();
    transaction.commit();
    drop(transaction);
    assert_eq!(fs::read(unknown).unwrap(), b"do not clean up");
    assert_eq!(fs::read(target).unwrap(), b"replacement");
}

#[test]
fn restore_identity_replaced_regular_parent_is_not_the_original_parent() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let parent = root.join("parent");
    fs::create_dir(&parent).unwrap();
    let target = parent.join("document");
    fs::write(&target, b"original").unwrap();
    let mut transaction = RestoreFiles::default();
    transaction
        .stage_replace(&target, |p| fs::write(p, b"replacement"))
        .unwrap();
    let recovery_name = transaction.swaps[0]
        .recovery
        .as_ref()
        .unwrap()
        .path()
        .file_name()
        .unwrap()
        .to_owned();
    fs::rename(&parent, root.join("displaced-parent")).unwrap();
    fs::create_dir(&parent).unwrap();
    let fake = parent.join(&recovery_name);
    fs::create_dir(&fake).unwrap();
    fs::write(fake.join("independent"), b"keep").unwrap();
    fs::write(&target, b"independent").unwrap();
    assert!(transaction.apply().is_err());
    drop(transaction);
    assert_eq!(fs::read(&target).unwrap(), b"independent");
    assert_eq!(fs::read(fake.join("independent")).unwrap(), b"keep");
    assert_eq!(
        fs::read(root.join("displaced-parent/document")).unwrap(),
        b"original"
    );
}

#[test]
fn restore_identity_changed_original_after_staging_is_not_published_over() {
    for deletion in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let target = root.join("document");
        fs::write(&target, b"original").unwrap();
        let mut transaction = RestoreFiles::default();
        if deletion {
            transaction.stage_delete(&target).unwrap();
        } else {
            transaction
                .stage_replace(&target, |p| fs::write(p, b"replacement"))
                .unwrap();
        }
        fs::write(&target, b"independent").unwrap();
        assert!(transaction.apply().is_err());
        drop(transaction);
        assert_eq!(entries(&root), ["document"]);
        assert_eq!(fs::read(target).unwrap(), b"independent");
    }
}

#[test]
fn restore_identity_replaced_recovery_directory_is_never_cleaned_as_owned() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let target = root.join("document");
    fs::write(&target, b"original").unwrap();
    let mut transaction = RestoreFiles::default();
    transaction
        .stage_replace(&target, |p| fs::write(p, b"replacement"))
        .unwrap();
    transaction.apply().unwrap();
    let recovery = transaction.swaps[0]
        .recovery
        .as_ref()
        .unwrap()
        .path()
        .to_path_buf();
    let displaced = root.join("displaced-recovery");
    fs::rename(&recovery, &displaced).unwrap();
    fs::create_dir(&recovery).unwrap();
    fs::write(recovery.join("independent"), b"keep").unwrap();
    assert!(transaction.rollback().is_err());
    transaction.commit();
    drop(transaction);
    assert_eq!(fs::read(recovery.join("independent")).unwrap(), b"keep");
    assert_eq!(fs::read(displaced.join("original")).unwrap(), b"original");
    assert_eq!(fs::read(target).unwrap(), b"replacement");
}

#[test]
fn restore_identity_committed_cleanup_preserves_changes_inside_original_tree() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let target = root.join("tree");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("item"), b"original").unwrap();
    let mut transaction = RestoreFiles::default();
    transaction
        .stage_replace(&target, |p| fs::create_dir(p))
        .unwrap();
    transaction.apply().unwrap();
    let original = transaction.swaps[0].path("original");
    fs::write(original.join("added"), b"independent").unwrap();
    transaction.commit();
    drop(transaction);
    assert_eq!(entries(&original), ["added", "item"]);
    assert_eq!(fs::read(original.join("added")).unwrap(), b"independent");
    assert!(entries(&target).is_empty());
}

#[test]
fn restore_identity_retry_recovers_only_when_its_own_replacement_returns() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let target = root.join("document");
    fs::write(&target, b"original").unwrap();
    let mut transaction = RestoreFiles::default();
    transaction
        .stage_replace(&target, |p| fs::write(p, b"replacement"))
        .unwrap();
    transaction.apply().unwrap();
    let parked = root.join("parked");
    fs::rename(&target, &parked).unwrap();
    fs::write(&target, b"independent").unwrap();
    for _ in 0..3 {
        assert!(transaction.rollback().is_err());
    }
    let independent = root.join("independent");
    fs::rename(&target, &independent).unwrap();
    fs::rename(&parked, &target).unwrap();
    transaction.rollback().unwrap();
    transaction.rollback().unwrap();
    drop(transaction);
    assert_eq!(entries(&root), ["document", "independent"]);
    assert_eq!(fs::read(target).unwrap(), b"original");
    assert_eq!(fs::read(independent).unwrap(), b"independent");
}

#[cfg(unix)]
#[test]
fn restore_identity_external_link_is_preserved_without_reading_its_referent() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let target = root.join("document");
    fs::write(&target, b"original").unwrap();
    let mut transaction = RestoreFiles::default();
    transaction
        .stage_replace(&target, |p| fs::write(p, b"replacement"))
        .unwrap();
    transaction.apply().unwrap();
    let original = transaction.swaps[0].path("original");
    fs::rename(&target, root.join("parked")).unwrap();
    let absent = root.join("missing-referent");
    std::os::unix::fs::symlink(&absent, &target).unwrap();
    assert!(transaction.rollback().is_err());
    drop(transaction);
    assert_eq!(fs::read_link(&target).unwrap(), absent);
    assert!(!absent.exists());
    assert_eq!(fs::read(original).unwrap(), b"original");
}
