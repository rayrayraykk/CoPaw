use super::*;

#[path = "desktop_restore_identity_tests.rs"]
mod identity;

fn entries(path: &Path) -> Vec<String> {
    let mut names = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn staged_files_and_directories_apply_and_rollback_as_one_transaction() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    fs::write(root.join("updated"), "before").unwrap();
    fs::write(root.join("deleted"), "keep me").unwrap();
    fs::create_dir(root.join("tree")).unwrap();
    fs::write(root.join("tree/old"), "old tree").unwrap();
    let mut transaction = RestoreFiles::default();
    transaction
        .stage_replace(&root.join("updated"), |path| fs::write(path, "after"))
        .unwrap();
    transaction.stage_delete(&root.join("deleted")).unwrap();
    transaction
        .stage_replace(&root.join("created"), |path| fs::write(path, "new"))
        .unwrap();
    transaction
        .stage_replace(&root.join("tree"), |path| {
            fs::create_dir(path)?;
            fs::write(path.join("new"), "new tree")
        })
        .unwrap();
    assert_eq!(fs::read_to_string(root.join("updated")).unwrap(), "before");
    assert_eq!(fs::read_to_string(root.join("deleted")).unwrap(), "keep me");
    assert!(!root.join("created").exists());
    transaction.apply().unwrap();
    assert_eq!(fs::read_to_string(root.join("updated")).unwrap(), "after");
    assert!(!root.join("deleted").exists());
    assert_eq!(fs::read_to_string(root.join("created")).unwrap(), "new");
    assert_eq!(entries(&root.join("tree")), ["new"]);
    transaction.rollback().unwrap();
    transaction.rollback().unwrap();
    drop(transaction);
    assert_eq!(entries(&root), ["deleted", "tree", "updated"]);
    assert_eq!(entries(&root.join("tree")), ["old"]);
    assert_eq!(
        fs::read_to_string(root.join("tree/old")).unwrap(),
        "old tree"
    );
    assert_eq!(fs::read_to_string(root.join("updated")).unwrap(), "before");
    assert_eq!(fs::read_to_string(root.join("deleted")).unwrap(), "keep me");
}

#[cfg(unix)]
#[test]
fn replacing_a_private_directory_retains_its_permissions_and_rollback_contents() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().unwrap();
    let target = directory
        .path()
        .canonicalize()
        .unwrap()
        .join("private-workspace");
    fs::create_dir(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(target.join("original"), "private data").unwrap();
    let mut transaction = RestoreFiles::default();
    transaction
        .stage_workspace_replace(&target, |replacement| {
            fs::create_dir(replacement)?;
            fs::set_permissions(replacement, fs::Permissions::from_mode(0o755))?;
            fs::write(replacement.join("restored"), "restored data")
        })
        .unwrap();
    transaction.apply().unwrap();
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(entries(&target), ["restored"]);
    drop(transaction);
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(entries(&target), ["original"]);
    assert_eq!(fs::read(target.join("original")).unwrap(), b"private data");
}

#[test]
fn failed_replacement_rolls_back_the_current_original_and_earlier_swaps() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    for name in ["first", "second"] {
        fs::write(root.join(name), name).unwrap();
    }
    let mut transaction = RestoreFiles::default();
    for name in ["first", "second"] {
        transaction
            .stage_replace(&root.join(name), |path| fs::write(path, "replacement"))
            .unwrap();
    }
    // A late rename failure happens after second's original has already moved.
    fs::remove_file(transaction.swaps[1].path("replacement")).unwrap();
    assert!(transaction.apply().is_err());
    drop(transaction);
    assert_eq!(entries(&root), ["first", "second"]);
    for name in ["first", "second"] {
        assert_eq!(fs::read_to_string(root.join(name)).unwrap(), name);
    }
}

#[test]
fn rollback_continues_after_an_error_and_retries_only_dirty_swaps() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let mut transaction = RestoreFiles::default();
    for name in ["first", "second"] {
        fs::write(root.join(name), name).unwrap();
        transaction
            .stage_replace(&root.join(name), |path| fs::write(path, "new"))
            .unwrap();
    }
    transaction.apply().unwrap();
    let obstruction = transaction.swaps[1].path("replacement");
    fs::create_dir(&obstruction).unwrap();
    fs::write(obstruction.join("occupied"), "test obstruction").unwrap();
    assert!(transaction.rollback().is_err());
    assert_eq!(fs::read_to_string(root.join("first")).unwrap(), "first");
    assert_eq!(fs::read_to_string(root.join("second")).unwrap(), "new");
    assert!(!transaction.swaps[0].is_dirty());
    assert!(transaction.swaps[1].is_dirty());
    fs::write(root.join("first"), "updated after rollback").unwrap();
    fs::remove_file(obstruction.join("occupied")).unwrap();
    fs::remove_dir(&obstruction).unwrap();
    transaction.rollback().unwrap();
    drop(transaction);
    assert_eq!(entries(&root), ["first", "second"]);
    assert_eq!(
        fs::read_to_string(root.join("first")).unwrap(),
        "updated after rollback"
    );
    assert_eq!(fs::read_to_string(root.join("second")).unwrap(), "second");
}

#[test]
fn dropping_an_incomplete_rollback_retains_original_data_for_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let target = root.join("document");
    fs::write(&target, "original").unwrap();
    let mut transaction = RestoreFiles::default();
    transaction
        .stage_replace(&target, |path| fs::write(path, "new"))
        .unwrap();
    transaction.apply().unwrap();
    let original = transaction.swaps[0].path("original");
    fs::create_dir(transaction.swaps[0].path("replacement")).unwrap();
    drop(transaction);
    assert_eq!(fs::read_to_string(original).unwrap(), "original");
    assert_eq!(fs::read_to_string(target).unwrap(), "new");
    assert_eq!(entries(&root).len(), 2);
}

#[test]
fn dropping_an_uncommitted_transaction_rolls_back_but_commit_keeps_replacements() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let target = root.join("document");
    fs::write(&target, "original").unwrap();
    for commit in [false, true] {
        let mut transaction = RestoreFiles::default();
        transaction
            .stage_replace(&target, |path| fs::write(path, "new"))
            .unwrap();
        transaction.apply().unwrap();
        if commit {
            transaction.commit();
        }
        drop(transaction);
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            if commit { "new" } else { "original" }
        );
        assert_eq!(entries(&root), ["document"]);
    }
}

#[test]
fn rejects_overlapping_destinations_and_incomplete_staging_without_changing_files() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    fs::create_dir(root.join("tree")).unwrap();
    let mut transaction = RestoreFiles::default();
    transaction
        .stage_replace(&root.join("tree"), |path| fs::create_dir(path))
        .unwrap();
    assert!(transaction.stage_delete(&root.join("TREE")).is_err());
    assert!(transaction.stage_delete(&root.join("tree/child")).is_err());
    assert!(
        transaction
            .stage_replace(&root.join("missing"), |_| Ok(()))
            .is_err()
    );
    drop(transaction);
    assert_eq!(entries(&root), ["tree"]);
    assert!(entries(&root.join("tree")).is_empty());
}

#[cfg(unix)]
#[test]
fn rejects_link_targets_and_detects_a_replaced_parent_before_writing() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    fs::create_dir(root.join("parent")).unwrap();
    fs::create_dir(root.join("outside")).unwrap();
    fs::write(root.join("outside/document"), "outside").unwrap();
    symlink(root.join("outside/document"), root.join("link")).unwrap();
    let mut transaction = RestoreFiles::default();
    assert!(transaction.stage_delete(&root.join("link")).is_err());
    transaction
        .stage_replace(&root.join("parent/document"), |path| fs::write(path, "new"))
        .unwrap();
    fs::rename(root.join("parent"), root.join("moved-parent")).unwrap();
    symlink(root.join("outside"), root.join("parent")).unwrap();
    assert!(transaction.apply().is_err());
    drop(transaction);
    assert_eq!(
        fs::read_to_string(root.join("outside/document")).unwrap(),
        "outside"
    );
}
