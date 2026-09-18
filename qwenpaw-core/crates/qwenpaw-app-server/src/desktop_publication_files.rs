//! Restartable file receipts for the two regular Agent publication files.

use std::io::{Read as _, Write as _};

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt as _};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use qwenpaw_core::AgentPublicationState;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use super::identity::RegularFile;
use super::{Identity, Path, PathBuf, RECOVERY_PREFIX, RestoreFiles, Swap, Tree, fs, invalid, io};

const MAX_JOURNAL_BYTES: u64 = 128 * 1024;

/// Two-file recovery owned by a trusted host, not an HTTP or backup payload.
/// Drop deliberately retains staging; the host must finish recovery explicitly.
pub struct AgentPublicationFiles {
    journal: Journal,
    persisted: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    installation: Uuid,
    transaction: Uuid,
    agent_id: String,
    entries: [Entry; 2],
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    target: NativePath,
    parent_identity: Identity,
    recovery_name: String,
    recovery_identity: Identity,
    #[serde(deserialize_with = "required_original")]
    original: Option<RegularFile>,
    replacement: RegularFile,
}

fn required_original<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<RegularFile>, D::Error> {
    Option::<RegularFile>::deserialize(deserializer)
}

// Preserve non-Unicode Unix paths and Windows code units without lossy conversion.
#[derive(Serialize, Deserialize)]
#[serde(tag = "platform", content = "units", deny_unknown_fields)]
enum NativePath {
    #[cfg(unix)]
    Unix(Vec<u8>),
    #[cfg(windows)]
    Windows(Vec<u16>),
}

impl NativePath {
    fn from_path(path: &Path) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;
            Self::Unix(path.as_os_str().as_bytes().to_vec())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt as _;
            Self::Windows(path.as_os_str().encode_wide().collect())
        }
    }

    fn path(&self) -> PathBuf {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt as _;
            let Self::Unix(bytes) = self;
            std::ffi::OsString::from_vec(bytes.clone()).into()
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt as _;
            let Self::Windows(units) = self;
            std::ffi::OsString::from_wide(units).into()
        }
    }
}

impl AgentPublicationFiles {
    /// Stages only the authorized Agent config and installation catalog.
    /// The host must persist this receipt and its trusted digest before applying.
    ///
    /// # Errors
    /// Returns an error for invalid targets, unavailable staging, or changed files.
    pub fn prepare(
        installation: Uuid,
        transaction: Uuid,
        agent_id: &str,
        config: &Path,
        config_bytes: &[u8],
        catalog: &Path,
        catalog_bytes: &[u8],
    ) -> io::Result<Self> {
        if installation.is_nil()
            || transaction.is_nil()
            || !qwenpaw_storage::is_valid_agent_id(agent_id)
            || config.file_name() != Some(std::ffi::OsStr::new("agent.json"))
            || catalog.file_name() != Some(std::ffi::OsStr::new("catalog.json"))
        {
            return Err(invalid("Invalid Agent publication file scope"));
        }
        let mut staged = RestoreFiles::default();
        for (path, bytes) in [(config, config_bytes), (catalog, catalog_bytes)] {
            RegularFile::optional(path)?;
            staged.stage_replace(path, |target| write_new(target, bytes))?;
        }
        let entries = staged
            .swaps
            .iter()
            .map(Entry::from_staged)
            .collect::<io::Result<Vec<_>>>()?;
        let entries = entries
            .try_into()
            .map_err(|_| invalid("Invalid publication file count"))?;
        for directory in staged
            .swaps
            .iter_mut()
            .filter_map(|swap| swap.recovery.take())
        {
            let _ = directory.keep();
        }
        staged.commit();
        Ok(Self {
            journal: Journal {
                version: 1,
                installation,
                transaction,
                agent_id: agent_id.into(),
                entries,
            },
            persisted: false,
        })
    }

    /// Persists a private immutable receipt without replacing an existing file.
    /// Store the returned digest in trusted installation-local control storage.
    ///
    /// # Errors
    /// Returns an error if the receipt exists or file/directory durability fails.
    pub fn persist(&mut self, path: &Path) -> io::Result<[u8; 32]> {
        if self.persisted {
            return Err(invalid("Publication receipt is already persisted"));
        }
        let bytes = self.encode()?;
        let parent = path
            .parent()
            .ok_or_else(|| invalid("Missing publication receipt parent"))?;
        if !path.is_absolute() || parent.canonicalize()? != parent {
            return Err(invalid("Invalid publication receipt location"));
        }
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist_noclobber(path).map_err(|error| error.error)?;
        sync_directory(parent)?;
        self.persisted = true;
        Ok(Sha256::digest(bytes).into())
    }

    /// Agent identity bound by the trusted receipt digest, for credential recovery.
    #[must_use]
    pub fn agent_id(&self) -> &str {
        &self.journal.agent_id
    }

    /// Encodes non-secret recovery metadata for installation-local persistence.
    ///
    /// # Errors
    /// Returns an error if encoding fails or the receipt exceeds its limit.
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        let bytes = serde_json::to_vec(&self.journal)
            .map_err(|_| invalid("Cannot encode publication receipt"))?;
        if bytes.len() as u64 > MAX_JOURNAL_BYTES {
            return Err(invalid("Publication receipt is too large"));
        }
        Ok(bytes)
    }

    /// Reopens a receipt only with its trusted digest and expected host binding.
    /// The digest must not be obtained from the untrusted receipt itself.
    ///
    /// # Errors
    /// Returns an error for links, oversized/corrupt receipts, or foreign bindings.
    pub fn read(
        path: &Path,
        digest: [u8; 32],
        installation: Uuid,
        transaction: Uuid,
        catalog: &Path,
    ) -> io::Result<Self> {
        let parent = path
            .parent()
            .ok_or_else(|| invalid("Missing receipt parent"))?;
        let name = path
            .file_name()
            .ok_or_else(|| invalid("Missing receipt name"))?;
        let directory = Dir::open_ambient_dir(parent, ambient_authority())?;
        if !directory.symlink_metadata(name)?.is_file() {
            return Err(invalid("Publication receipt is not a regular file"));
        }
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let file = directory.open_with(name, &options)?;
        if !file.metadata()?.is_file() {
            return Err(invalid("Publication receipt is not a regular file"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_JOURNAL_BYTES + 1).read_to_end(&mut bytes)?;
        Self::from_persisted(&bytes, digest, installation, transaction, catalog)
    }

    /// Loads metadata already durably stored by the trusted host with its decision.
    ///
    /// # Errors
    /// Returns an error for a corrupt receipt or mismatched host binding.
    pub fn from_persisted(
        bytes: &[u8],
        digest: [u8; 32],
        installation: Uuid,
        transaction: Uuid,
        catalog: &Path,
    ) -> io::Result<Self> {
        if bytes.len() as u64 > MAX_JOURNAL_BYTES
            || <[u8; 32]>::from(Sha256::digest(bytes)) != digest
        {
            return Err(invalid("Publication receipt digest mismatch"));
        }
        let journal: Journal =
            serde_json::from_slice(bytes).map_err(|_| invalid("Publication receipt is invalid"))?;
        if journal.version != 1
            || !qwenpaw_storage::is_valid_agent_id(&journal.agent_id)
            || installation.is_nil()
            || transaction.is_nil()
            || journal.installation != installation
            || journal.transaction != transaction
            || journal.entries[1].target.path() != catalog
        {
            return Err(invalid("Publication receipt binding mismatch"));
        }
        for (entry, name) in journal.entries.iter().zip(["agent.json", "catalog.json"]) {
            let target = entry.target.path();
            if !target.is_absolute()
                || target.file_name() != Some(std::ffi::OsStr::new(name))
                || !entry.recovery_name.starts_with(RECOVERY_PREFIX)
                || Path::new(&entry.recovery_name).components().count() != 1
                || !matches!(
                    Path::new(&entry.recovery_name).components().next(),
                    Some(std::path::Component::Normal(_))
                )
            {
                return Err(invalid("Publication receipt target is invalid"));
            }
        }
        Ok(Self {
            journal,
            persisted: true,
        })
    }

    /// Publishes both staged files; the host commits SQLite only after success.
    /// On error, retain this receipt and invoke rollback before opening admission.
    ///
    /// # Errors
    /// Returns an error for missing durability, non-initial state, or failed swaps.
    pub fn apply(&self) -> io::Result<()> {
        if !self.persisted {
            return Err(invalid("Persist the publication receipt before applying"));
        }
        for entry in &self.journal.entries {
            entry.check_initial()?;
        }
        for entry in &self.journal.entries {
            entry.check_initial()?;
            if entry.original.is_some() {
                move_file(&entry.target.path(), &entry.recovery().join("original"))?;
            }
            move_file(&entry.recovery().join("replacement"), &entry.target.path())?;
        }
        Ok(())
    }

    /// Reconstructs and reverses unfinished swaps from actual file locations.
    /// Attempts every entry even if another entry must retain recovery data.
    ///
    /// # Errors
    /// Returns an error for independent changes or unavailable original files.
    pub fn rollback(&self) -> io::Result<()> {
        let mut first = None;
        for entry in self.journal.entries.iter().rev() {
            if let Err(error) = entry.rollback() {
                first.get_or_insert(error);
            }
        }
        first.map_or(Ok(()), Err)
    }

    /// Cleans verified staging only after rollback or committed publication.
    /// Does not delete the receipt or the host's SQLite decision.
    ///
    /// # Errors
    /// Returns an error for unknown entries, changed targets, or failed cleanup.
    pub fn cleanup(&self, state: AgentPublicationState) -> io::Result<()> {
        for entry in &self.journal.entries {
            entry.check_cleanup(state)?;
        }
        for entry in &self.journal.entries {
            entry.check_cleanup(state)?;
            let recovery = entry.recovery();
            if !entry.check_recovery()? {
                continue;
            }
            for name in ["original", "replacement"] {
                let path = recovery.join(name);
                if RegularFile::optional(&path)?.is_some() {
                    entry.check_cleanup(state)?;
                    fs::remove_file(path)?;
                    sync_directory(&recovery)?;
                }
            }
            fs::remove_dir(&recovery)?;
            sync_directory(entry.parent())?;
        }
        Ok(())
    }
}

impl Entry {
    fn from_staged(swap: &Swap) -> io::Result<Self> {
        swap.check_parent()?;
        let recovery = swap.recovery.as_ref().unwrap().path();
        fs::File::open(swap.path("replacement"))?.sync_all()?;
        sync_directory(recovery)?;
        sync_directory(&swap.parent)?;
        Ok(Self {
            target: NativePath::from_path(&swap.target),
            parent_identity: swap.parent_identity.clone(),
            recovery_name: recovery
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .ok_or_else(|| invalid("Invalid recovery directory name"))?
                .into(),
            recovery_identity: swap.recovery_identity.clone(),
            original: swap.original_tree.as_ref().map(Tree::regular).transpose()?,
            replacement: swap
                .replacement_tree
                .as_ref()
                .ok_or_else(|| invalid("Missing staged replacement"))?
                .regular()?,
        })
    }

    fn parent(&self) -> PathBuf {
        self.target.path().parent().unwrap().to_path_buf()
    }
    fn recovery(&self) -> PathBuf {
        self.parent().join(&self.recovery_name)
    }

    fn check_recovery(&self) -> io::Result<bool> {
        let parent = self.parent();
        if parent.canonicalize()? != parent {
            return Err(invalid("Publication parent changed"));
        }
        self.parent_identity.check_directory(&parent)?;
        match fs::symlink_metadata(self.recovery()) {
            Ok(_) => {
                self.recovery_identity.check_directory(&self.recovery())?;
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn positions(
        &self,
    ) -> io::Result<(
        Option<RegularFile>,
        Option<RegularFile>,
        Option<RegularFile>,
    )> {
        let exists = self.check_recovery()?;
        let target = RegularFile::optional(&self.target.path())?;
        if !exists {
            return Ok((target, None, None));
        }
        for entry in fs::read_dir(self.recovery())? {
            let name = entry?.file_name();
            if name != "original" && name != "replacement" {
                return Err(invalid("Publication staging contains independent entries"));
            }
        }
        let original = RegularFile::optional(&self.recovery().join("original"))?;
        let replacement = RegularFile::optional(&self.recovery().join("replacement"))?;
        if (original.is_some() && original != self.original)
            || replacement
                .as_ref()
                .is_some_and(|value| value != &self.replacement)
        {
            return Err(invalid("Publication staging changed independently"));
        }
        Ok((target, original, replacement))
    }

    fn check_initial(&self) -> io::Result<()> {
        let (target, original, replacement) = self.positions()?;
        if target != self.original
            || original.is_some()
            || replacement.as_ref() != Some(&self.replacement)
        {
            return Err(invalid("Publication files are not in their staged state"));
        }
        Ok(())
    }

    fn rollback(&self) -> io::Result<()> {
        let (target, original, replacement) = self.positions()?;
        if target == self.original && original.is_none() {
            return Ok(());
        }
        if target.as_ref() == Some(&self.replacement)
            && original == self.original
            && replacement.is_none()
        {
            move_file(&self.target.path(), &self.recovery().join("replacement"))?;
        }
        let (target, original, replacement) = self.positions()?;
        if target.is_none()
            && original == self.original
            && replacement.as_ref() == Some(&self.replacement)
        {
            if original.is_some() {
                move_file(&self.recovery().join("original"), &self.target.path())?;
            }
            return Ok(());
        }
        Err(invalid(
            "Publication files cannot be rolled back without overwriting independent data",
        ))
    }

    fn check_cleanup(&self, state: AgentPublicationState) -> io::Result<()> {
        let (target, original, replacement) = self.positions()?;
        let valid = match state {
            AgentPublicationState::Prepared => target == self.original && original.is_none(),
            AgentPublicationState::Committed => {
                target.as_ref() == Some(&self.replacement) && replacement.is_none()
            }
        };
        if !valid {
            return Err(invalid(
                "Publication cleanup state does not match the decision",
            ));
        }
        Ok(())
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn sync_directory(path: impl AsRef<Path>) -> io::Result<()> {
    Dir::open_ambient_dir(path, ambient_authority())?
        .into_std_file()
        .sync_all()
}

fn move_file(source: &Path, destination: &Path) -> io::Result<()> {
    match fs::symlink_metadata(destination) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        _ => {
            return Err(invalid(
                "Publication move destination is occupied or unavailable",
            ));
        }
    }
    fs::rename(source, destination)?;
    sync_directory(
        source
            .parent()
            .ok_or_else(|| invalid("Missing publication source parent"))?,
    )?;
    sync_directory(
        destination
            .parent()
            .ok_or_else(|| invalid("Missing publication destination parent"))?,
    )
}

#[cfg(test)]
#[path = "desktop_publication_files_tests.rs"]
mod tests;
