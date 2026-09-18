//! Capture restore entries without following links outside the selected tree.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use cap_fs_ext::{FollowSymlinks, MetadataExt as _, OpenOptionsFollowExt as _};
use cap_std::ambient_authority;
#[cfg(unix)]
use cap_std::fs::PermissionsExt as _;
use cap_std::fs::{Dir, Metadata, OpenOptions, Permissions};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Identity(u64, u64);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RegularFile {
    identity: Identity,
    #[cfg(unix)]
    mode: u32,
    #[cfg(windows)]
    readonly: bool,
    digest: [u8; 32],
}

impl RegularFile {
    pub(super) fn optional(path: &Path) -> io::Result<Option<Self>> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if !metadata.is_file() => return Err(changed()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
            Ok(_) => {}
        }
        Tree::optional(path)?
            .as_ref()
            .map(Tree::regular)
            .transpose()
    }
}

impl Identity {
    fn of(metadata: &Metadata) -> Self {
        Self(metadata.dev(), metadata.ino())
    }

    pub(super) fn directory(path: &Path) -> io::Result<Self> {
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(changed());
        }
        Ok(Self::of(
            &Dir::open_ambient_dir(path, ambient_authority())?.dir_metadata()?,
        ))
    }

    pub(super) fn check_directory(&self, path: &Path) -> io::Result<()> {
        if *self != Self::directory(path)? {
            return Err(changed());
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Tree {
    identity: Identity,
    permissions: Permissions,
    content: Content,
}

#[derive(Debug, PartialEq, Eq)]
enum Content {
    File([u8; 32]),
    Directory(BTreeMap<OsString, Tree>),
    Link(PathBuf),
}

impl Tree {
    pub(super) fn regular(&self) -> io::Result<RegularFile> {
        let Content::File(digest) = self.content else {
            return Err(changed());
        };
        Ok(RegularFile {
            identity: self.identity.clone(),
            #[cfg(unix)]
            mode: self.permissions.mode(),
            #[cfg(windows)]
            readonly: self.permissions.readonly(),
            digest,
        })
    }

    pub(super) fn optional(path: &Path) -> io::Result<Option<Self>> {
        match Self::capture(path) {
            Ok(tree) => Ok(Some(tree)),
            Err(error)
                if error.kind() == io::ErrorKind::NotFound
                    && matches!(std::fs::symlink_metadata(path), Err(e) if e.kind() == io::ErrorKind::NotFound) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn capture(path: &Path) -> io::Result<Self> {
        let parent = path.parent().ok_or_else(changed)?;
        let name = path.file_name().ok_or_else(changed)?;
        Self::at(
            &Dir::open_ambient_dir(parent, ambient_authority())?,
            Path::new(name),
        )
    }

    pub(super) fn check(&self, path: &Path) -> io::Result<()> {
        if *self != Self::capture(path)? {
            return Err(changed());
        }
        Ok(())
    }

    fn at(parent: &Dir, name: &Path) -> io::Result<Self> {
        let before = parent.symlink_metadata(name)?;
        let identity = Identity::of(&before);
        let content = if before.is_symlink() {
            Content::Link(parent.read_link_contents(name)?)
        } else if before.is_dir() {
            let directory = parent.open_dir(name)?;
            if identity != Identity::of(&directory.dir_metadata()?) {
                return Err(changed());
            }
            let mut children = BTreeMap::new();
            for entry in directory.entries()? {
                let name = entry?.file_name();
                children.insert(name.clone(), Self::at(&directory, Path::new(&name))?);
            }
            Content::Directory(children)
        } else if before.is_file() {
            let mut options = OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            let mut file = parent.open_with(name, &options)?;
            if identity != Identity::of(&file.metadata()?) {
                return Err(changed());
            }
            Content::File(digest(&mut file, before.len())?)
        } else {
            return Err(changed());
        };
        let after = parent.symlink_metadata(name)?;
        if identity != Identity::of(&after)
            || before.len() != after.len()
            || before.modified()? != after.modified()?
            || before.permissions() != after.permissions()
        {
            return Err(changed());
        }
        Ok(Self {
            identity,
            permissions: before.permissions(),
            content,
        })
    }
}

fn digest(file: &mut impl io::Read, length: u64) -> io::Result<[u8; 32]> {
    // A growing file must not keep the synchronous publication gate forever.
    let mut file = file.take(length.checked_add(1).ok_or_else(changed)?);
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            if total != length {
                return Err(changed());
            }
            return Ok(digest.finalize().into());
        }
        total += count as u64;
        digest.update(&buffer[..count]);
    }
}

fn changed() -> io::Error {
    io::Error::other("Restore entry identity or contents changed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_identity_digest_rejects_growth_and_truncation_with_bounded_reads() {
        assert!(digest(&mut io::repeat(b'x'), 2).is_err());
        assert!(digest(&mut io::Cursor::new(b"x"), 2).is_err());
        assert_eq!(
            digest(&mut io::Cursor::new(b"xx"), 2).unwrap(),
            <[u8; 32]>::from(Sha256::digest(b"xx"))
        );
        assert!(digest(&mut io::repeat(b'x'), u64::MAX).is_err());
    }
}
