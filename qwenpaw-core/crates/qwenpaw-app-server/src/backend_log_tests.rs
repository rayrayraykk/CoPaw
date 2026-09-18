use pretty_assertions::assert_eq;

use super::*;

#[test]
fn backend_log_appends_reopens_and_preserves_existing_contents() {
    let root = tempfile::tempdir().unwrap();
    for message in ["first\n", "second\n"] {
        let mut log = BackendLog::with_limits(root.path(), 100, 3).unwrap();
        log.write_all(message.as_bytes()).unwrap();
        log.flush().unwrap();
    }
    assert_eq!(
        std::fs::read(root.path().join(NAME)).unwrap(),
        b"first\nsecond\n"
    );
}

#[test]
fn backend_log_rotates_in_order_and_keeps_only_configured_backups() {
    let root = tempfile::tempdir().unwrap();
    let mut log = BackendLog::with_limits(root.path(), 8, 2).unwrap();
    for message in ["one\n", "two\n", "tri\n", "end\n"] {
        log.write_all(message.as_bytes()).unwrap();
    }
    drop(log);
    for (name, expected) in [
        (NAME, "end\n"),
        ("qwenpaw.log.1", "tri\n"),
        ("qwenpaw.log.2", "two\n"),
    ] {
        assert_eq!(
            std::fs::read_to_string(root.path().join(name)).unwrap(),
            expected
        );
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 3);
}

#[test]
fn backend_log_zero_backups_disables_rotation_like_the_original() {
    let root = tempfile::tempdir().unwrap();
    let mut log = BackendLog::with_limits(root.path(), 1, 0).unwrap();
    log.write_all(b"first\nsecond\n").unwrap();
    assert_eq!(
        std::fs::read(root.path().join(NAME)).unwrap(),
        b"first\nsecond\n"
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn backend_log_failed_rotation_preserves_the_current_record_and_reserved_directory() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("qwenpaw.log.1")).unwrap();
    let mut log = BackendLog::with_limits(root.path(), 5, 1).unwrap();
    log.write_all(b"first\n").unwrap();
    log.write_all(b"second\n").unwrap();
    assert_eq!(
        std::fs::read(root.path().join(NAME)).unwrap(),
        b"first\nsecond\n"
    );
    assert!(root.path().join("qwenpaw.log.1").is_dir());
}

#[test]
fn backend_log_clones_serialize_complete_records() {
    let root = tempfile::tempdir().unwrap();
    let log = BackendLog::with_limits(root.path(), 1024 * 1024, 3).unwrap();
    let tasks = (0..16)
        .map(|i| {
            let mut log = log.clone();
            std::thread::spawn(move || {
                log.write_all(format!("record {i:02}\n").as_bytes())
                    .unwrap();
            })
        })
        .collect::<Vec<_>>();
    for task in tasks {
        task.join().unwrap();
    }
    let contents = std::fs::read_to_string(root.path().join(NAME)).unwrap();
    let mut actual = contents.lines().map(str::to_owned).collect::<Vec<_>>();
    actual.sort();
    assert_eq!(
        actual,
        (0..16)
            .map(|i| format!("record {i:02}"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn backend_log_size_parser_matches_original_units_and_rejects_bad_values() {
    for (raw, expected) in [
        ("5 MiB", 5 * 1024 * 1024),
        (" 1kb ", 1024),
        ("2G", 2 * 1024 * 1024 * 1024),
        ("32", 32),
    ] {
        assert_eq!(parse_size(raw), Some(expected), "{raw}");
    }
    for raw in [
        "",
        "0",
        "-1",
        "1.5MB",
        "1ki",
        "2PB",
        "18446744073709551615TiB",
    ] {
        assert_eq!(parse_size(raw), None, "{raw}");
    }
}

#[cfg(unix)]
#[test]
fn backend_log_refuses_symlink_sinks_and_creates_private_files() {
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(external.path(), "private fixture").unwrap();
    symlink(external.path(), root.path().join(NAME)).unwrap();
    assert!(BackendLog::with_limits(root.path(), 100, 3).is_err());
    assert_eq!(
        std::fs::read_to_string(external.path()).unwrap(),
        "private fixture"
    );
    let safe = tempfile::tempdir().unwrap();
    let _log = BackendLog::with_limits(safe.path(), 100, 3).unwrap();
    assert_eq!(
        std::fs::metadata(safe.path().join(NAME))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}
