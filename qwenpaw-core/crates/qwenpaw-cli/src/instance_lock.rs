//! Keep one CLI host in charge of startup recovery and writes per data directory.

use std::fs::{File, OpenOptions};
use std::path::Path;

use anyhow::Context;

const LOCK_FILE: &str = ".core-instance.lock";

pub(super) fn acquire(database: &Path) -> anyhow::Result<File> {
    let directory = database
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(directory).context("failed to create Core data directory")?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(LOCK_FILE))
        .context("failed to open Core instance lock")?;
    fs2::FileExt::try_lock_exclusive(&lock).map_err(|error| {
        if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() {
            anyhow::anyhow!("Core data directory is already in use by another process")
        } else {
            anyhow::Error::new(error).context("failed to acquire Core instance lock")
        }
    })?;
    // Keep the same file across releases. Unlinking it permits two lock inodes.
    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn an_existing_lock_file_is_not_a_stale_owner_or_truncated() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("threads.sqlite3");
        let path = directory.path().join(LOCK_FILE);
        std::fs::write(&path, b"retained bytes").unwrap();
        let first = acquire(&database).unwrap();
        assert_eq!(
            acquire(&database).unwrap_err().to_string(),
            "Core data directory is already in use by another process"
        );
        drop(first);
        assert_eq!(std::fs::read(&path).unwrap(), b"retained bytes");
        let second = acquire(&database).unwrap();
        assert!(!database.exists());
        drop(second);
        assert_eq!(std::fs::read(&path).unwrap(), b"retained bytes");
        assert!(path.is_file());
    }

    #[test]
    fn independent_data_directories_do_not_share_ownership() {
        let directory = tempfile::tempdir().unwrap();
        let first = acquire(&directory.path().join("第一 Core/threads.sqlite3")).unwrap();
        let second = acquire(&directory.path().join("第二 Core/threads.sqlite3")).unwrap();
        drop((first, second));
    }

    #[test]
    fn invalid_data_directory_fails_without_changing_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("not-a-directory");
        std::fs::write(&parent, b"unchanged").unwrap();
        assert_eq!(
            acquire(&parent.join("threads.sqlite3"))
                .unwrap_err()
                .to_string(),
            "failed to create Core data directory"
        );
        assert_eq!(std::fs::read(parent).unwrap(), b"unchanged");
    }

    #[cfg(unix)]
    #[test]
    fn directory_symlinks_share_the_original_lock() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original");
        let alias = directory.path().join("alias");
        let first = acquire(&original.join("threads.sqlite3")).unwrap();
        std::os::unix::fs::symlink(&original, &alias).unwrap();
        assert_eq!(
            acquire(&alias.join("threads.sqlite3"))
                .unwrap_err()
                .to_string(),
            "Core data directory is already in use by another process"
        );
        drop(first);
        let second = acquire(&alias.join("threads.sqlite3")).unwrap();
        drop(second);
    }
}
