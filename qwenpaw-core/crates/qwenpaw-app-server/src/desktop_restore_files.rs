//! Same-filesystem restore swaps retained through the surrounding transaction.

use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

#[path = "desktop_restore_identity.rs"]
mod identity;
use identity::{Identity, Tree};

#[path = "desktop_publication_files.rs"]
pub(super) mod publication;

pub(super) const RECOVERY_PREFIX: &str = ".qwenpaw-restore-";

#[derive(Default)]
pub(super) struct RestoreFiles {
    swaps: Vec<Swap>,
    created_directories: Vec<(PathBuf, Identity)>,
    committed: bool,
}

struct Swap {
    target: PathBuf,
    parent: PathBuf,
    parent_identity: Identity,
    recovery: Option<tempfile::TempDir>,
    recovery_identity: Identity,
    original_tree: Option<Tree>,
    replacement_tree: Option<Tree>,
    replacement: bool,
    original_moved: bool,
    replacement_installed: bool,
    policy: TargetPolicy,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetPolicy {
    RegularOnly,
    WorkspaceEntry,
}

impl RestoreFiles {
    pub(super) fn create_directory(&mut self, path: &Path) -> io::Result<()> {
        if self.committed || self.swaps.iter().any(Swap::is_dirty) || !path.is_absolute() {
            return Err(invalid("Cannot create this restore directory"));
        }
        fs::create_dir(path)?;
        self.created_directories
            .push((path.to_path_buf(), Identity::directory(path)?));
        Ok(())
    }

    /// The caller validates authorization, scope and archive paths before
    /// staging. `build` writes a regular file or directory at the supplied path.
    pub(super) fn stage_replace(
        &mut self,
        target: &Path,
        build: impl FnOnce(&Path) -> io::Result<()>,
    ) -> io::Result<()> {
        self.stage_replacement(target, build, TargetPolicy::RegularOnly)
    }

    /// Full Workspace restore may replace a link entry, never its referent.
    /// Archive output must still be a regular file or directory in staging.
    pub(super) fn stage_workspace_replace(
        &mut self,
        target: &Path,
        build: impl FnOnce(&Path) -> io::Result<()>,
    ) -> io::Result<()> {
        self.stage_replacement(target, build, TargetPolicy::WorkspaceEntry)
    }

    fn stage_replacement(
        &mut self,
        target: &Path,
        build: impl FnOnce(&Path) -> io::Result<()>,
        policy: TargetPolicy,
    ) -> io::Result<()> {
        let mut swap = self.prepare(target, true, policy)?;
        let replacement = swap.path("replacement");
        build(&replacement)?;
        check_regular_target(&replacement)?;
        if let Ok(metadata) = fs::symlink_metadata(&swap.target)
            && !is_link(&metadata)
            && ((metadata.is_file() && replacement.is_file())
                || (metadata.is_dir() && replacement.is_dir()))
        {
            fs::set_permissions(&replacement, metadata.permissions())?;
        }
        swap.replacement_tree = Some(Tree::capture(&replacement)?);
        self.swaps.push(swap);
        Ok(())
    }

    pub(super) fn stage_delete(&mut self, target: &Path) -> io::Result<()> {
        let swap = self.prepare(target, false, TargetPolicy::RegularOnly)?;
        self.swaps.push(swap);
        Ok(())
    }

    /// Remove an entry excluded from a full Workspace backup, without following
    /// a local symlink or Windows junction during exchange or rollback.
    pub(super) fn stage_workspace_delete(&mut self, target: &Path) -> io::Result<()> {
        let swap = self.prepare(target, false, TargetPolicy::WorkspaceEntry)?;
        self.swaps.push(swap);
        Ok(())
    }

    fn prepare(&self, target: &Path, replacement: bool, policy: TargetPolicy) -> io::Result<Swap> {
        if self.committed || self.swaps.iter().any(Swap::is_dirty) {
            return Err(invalid("Cannot stage an applied restore transaction"));
        }
        if !target.is_absolute() {
            return Err(invalid("Restore destination must be absolute"));
        }
        let parent = target
            .parent()
            .ok_or_else(|| invalid("Restore destination has no parent"))?
            .canonicalize()?;
        let name = target
            .file_name()
            .ok_or_else(|| invalid("Restore destination has no filename"))?;
        let target = parent.join(name);
        check_optional_target(&target, policy)?;
        let parent_identity = Identity::directory(&parent)?;
        let original_tree = Tree::optional(&target)?;
        // Conservative case folding also rejects ambiguous names in archives
        // prepared on a case-sensitive host for a case-insensitive filesystem.
        let key = target.to_string_lossy().to_lowercase();
        for swap in &self.swaps {
            let existing = swap.target.to_string_lossy().to_lowercase();
            let key = Path::new(&key);
            let existing = Path::new(&existing);
            if key.starts_with(existing) || existing.starts_with(key) {
                return Err(invalid("Restore destinations overlap"));
            }
        }
        let recovery = tempfile::Builder::new()
            .prefix(RECOVERY_PREFIX)
            .tempdir_in(&parent)?;
        let recovery_identity = Identity::directory(recovery.path())?;
        Ok(Swap {
            target,
            parent,
            parent_identity,
            recovery: Some(recovery),
            recovery_identity,
            original_tree,
            replacement_tree: None,
            replacement,
            original_moved: false,
            replacement_installed: false,
            policy,
        })
    }

    pub(super) fn apply(&mut self) -> io::Result<()> {
        if self.committed || self.swaps.iter().any(Swap::is_dirty) {
            return Err(invalid("Restore transaction was already applied"));
        }
        for index in 0..self.swaps.len() {
            if let Err(error) = self.swaps[index].apply() {
                return match self.rollback() {
                    Ok(()) => Err(error),
                    Err(_) => Err(io::Error::other(
                        "Restore failed and file rollback is incomplete; recovery data retained",
                    )),
                };
            }
        }
        Ok(())
    }

    /// Retry all remaining inverse operations, even if another inverse fails.
    pub(super) fn rollback(&mut self) -> io::Result<()> {
        if self.committed {
            return Err(invalid("Cannot roll back a committed restore transaction"));
        }
        let mut first_error = None;
        for swap in self.swaps.iter_mut().rev() {
            if let Err(error) = swap.rollback() {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Only call after files, credentials and runtime state have all committed.
    pub(super) fn commit(&mut self) {
        self.committed = true;
    }
}

impl Swap {
    fn path(&self, name: &str) -> PathBuf {
        self.recovery
            .as_ref()
            .expect("restore recovery directory exists until Drop")
            .path()
            .join(name)
    }

    fn is_dirty(&self) -> bool {
        self.original_moved || self.replacement_installed
    }

    fn check_parent(&self) -> io::Result<()> {
        if self.parent.canonicalize()? != self.parent {
            return Err(invalid("Restore destination parent changed"));
        }
        self.parent_identity.check_directory(&self.parent)?;
        self.recovery_identity.check_directory(
            self.recovery
                .as_ref()
                .ok_or_else(|| invalid("Recovery directory is unavailable"))?
                .path(),
        )
    }

    fn apply(&mut self) -> io::Result<()> {
        self.check_parent()?;
        if Tree::optional(&self.target)? != self.original_tree {
            return Err(invalid("Restore target changed after staging"));
        }
        if check_optional_target(&self.target, self.policy)? {
            fs::rename(&self.target, self.path("original"))?;
            self.original_moved = true;
        }
        if self.replacement {
            self.replacement_tree
                .as_ref()
                .ok_or_else(|| invalid("Replacement identity is missing"))?
                .check(&self.path("replacement"))?;
            fs::rename(self.path("replacement"), &self.target)?;
            self.replacement_installed = true;
        }
        Ok(())
    }

    fn rollback(&mut self) -> io::Result<()> {
        if !self.is_dirty() {
            return Ok(());
        }
        self.check_parent()?;
        if self.original_moved {
            self.original_tree
                .as_ref()
                .ok_or_else(|| invalid("Original identity is missing"))?
                .check(&self.path("original"))?;
        }
        if self.replacement_installed {
            self.replacement_tree
                .as_ref()
                .ok_or_else(|| invalid("Replacement identity is missing"))?
                .check(&self.target)?;
            if fs::symlink_metadata(self.path("replacement")).is_ok() {
                return Err(invalid("Restore rollback staging destination is occupied"));
            }
            fs::rename(&self.target, self.path("replacement"))?;
            self.replacement_installed = false;
        }
        if self.original_moved {
            // Never replace something created independently while we rolled
            // back another entry. Keep the original for a retry or recovery.
            if fs::symlink_metadata(&self.target).is_ok() {
                return Err(invalid("Restore rollback destination is occupied"));
            }
            fs::rename(self.path("original"), &self.target)?;
            self.original_moved = false;
        }
        Ok(())
    }

    fn cleanup(&mut self, preserve: bool) {
        if self.recovery.is_none() {
            return;
        }
        let safe = !preserve && self.cleanup_safe().is_ok();
        if let Some(directory) = self.recovery.take() {
            if safe {
                if let Err(error) = directory.close() {
                    tracing::warn!(%error, "Restore transaction cleanup did not finish");
                }
            } else {
                let path = directory.keep();
                tracing::error!(recovery_dir = %path.display(), "Restore cleanup unsafe or rollback incomplete; recovery data retained");
            }
        }
    }

    fn cleanup_safe(&self) -> io::Result<()> {
        self.check_parent()?;
        for entry in fs::read_dir(self.recovery.as_ref().unwrap().path())? {
            let entry = entry?;
            let name = entry.file_name();
            let expected = if name == "original" && self.original_moved {
                self.original_tree.as_ref()
            } else if name == "replacement" && !self.replacement_installed {
                self.replacement_tree.as_ref()
            } else {
                None
            };
            expected
                .ok_or_else(|| invalid("Recovery directory has independent entries"))?
                .check(&entry.path())?;
        }
        Ok(())
    }
}

impl Drop for Swap {
    fn drop(&mut self) {
        self.cleanup(self.is_dirty());
    }
}

impl Drop for RestoreFiles {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.rollback();
        }
        for swap in &mut self.swaps {
            // TempDir cleanup must not follow a parent redirected after
            // staging, even if no live file has been swapped yet.
            swap.cleanup(!self.committed && swap.is_dirty());
        }
        if !self.committed {
            for (path, identity) in self.created_directories.iter().rev() {
                if path
                    .canonicalize()
                    .as_ref()
                    .is_ok_and(|canonical| canonical == path)
                    && identity.check_directory(path).is_ok()
                {
                    // Never recursively remove a directory: a retained recovery
                    // or independently created file must survive failed cleanup.
                    let _ = fs::remove_dir(path);
                }
            }
        }
    }
}

fn check_optional_target(path: &Path, policy: TargetPolicy) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            let valid = if is_link(&metadata) {
                policy == TargetPolicy::WorkspaceEntry
            } else {
                metadata.is_file() || metadata.is_dir()
            };
            if !valid {
                return Err(invalid(
                    "Restore destination must be a regular file or directory",
                ));
            }
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn check_regular_target(path: &Path) -> io::Result<()> {
    if !check_optional_target(path, TargetPolicy::RegularOnly)? {
        return Err(invalid("Restore replacement was not staged"));
    }
    Ok(())
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
#[path = "desktop_restore_files_tests.rs"]
mod tests;
