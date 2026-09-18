//! Stage selected Workspace trees without swapping the live Core data subtree.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use sha2::Digest as _;
use sha2::Sha256;
use zip::ZipArchive;

use super::super::DesktopWorkspace;
use super::super::desktop_agents;
use super::super::desktop_agents::identity::MARKER_NAME;
use super::super::desktop_agents::restore::AgentRestorePlan;
use super::super::desktop_restore_files::RECOVERY_PREFIX;
use super::super::desktop_restore_files::RestoreFiles;
use super::Manifest;
use super::ManifestEntry;

struct Payload {
    name: String,
    destination: PathBuf,
    expected: ManifestEntry,
    agent_config_mapping: Option<(String, PathBuf)>,
    inline_bytes: Option<Vec<u8>>,
}

struct WorkspaceStage {
    destination: PathBuf,
    payloads: Vec<Payload>,
    protected: Vec<PathBuf>,
}

/// Credential-derived indicators must be persisted even for secrets-only
/// restoration; all unrelated provider configuration remains unchanged.
pub(super) fn stage_model_registry(
    desktop: &DesktopWorkspace,
    bytes: &[u8],
    files: &mut RestoreFiles,
) -> Result<(), String> {
    let destination = desktop.data_dir.join("models").join("registry.json");
    ensure_parent(&destination, files)
        .map_err(|_| String::from("Model registry parent is invalid"))?;
    files
        .stage_replace(&destination, |replacement| {
            write_private(replacement, bytes)
        })
        .map_err(|_| String::from("Model registry staging failed"))
}

/// Global files are overlays, not permission to replace the Core data root.
/// Model registry bytes have already passed typed candidate hydration.
pub(super) fn stage_globals<R: Read + Seek>(
    desktop: &DesktopWorkspace,
    archive: &mut ZipArchive<R>,
    manifest: &Manifest,
    model_registry: Option<&[u8]>,
    files: &mut RestoreFiles,
) -> Result<(), String> {
    let mut payloads = collect_payloads(manifest, super::CONFIG_PREFIX, &desktop.data_dir, &[])?;
    payloads.retain(|payload| payload.name != super::GLOBAL_AGENT_STATE_FILE);
    for payload in &payloads {
        let relative = payload
            .destination
            .strip_prefix(&desktop.data_dir)
            .map_err(|_| String::from("Invalid global restore target"))?;
        let first = relative
            .components()
            .next()
            .unwrap()
            .as_os_str()
            .to_string_lossy()
            .to_lowercase();
        if matches!(
            first.as_str(),
            "backups" | "agents" | "workspaces" | "skill_pool" | "local-models" | "checkpoints"
        ) || !super::is_global_config_file(relative)
        {
            return Err(String::from("Backup global payload targets protected data"));
        }
    }
    for payload in payloads {
        ensure_parent(&payload.destination, files)
            .map_err(|_| String::from("Global restore parent is invalid"))?;
        files
            .stage_replace(&payload.destination, |replacement| {
                if payload.name == "data/config/models/registry.json" {
                    let bytes = model_registry
                        .ok_or_else(|| invalid("Model registry was not validated"))?;
                    write_private(replacement, bytes)
                } else {
                    extract_file(replacement, &payload, archive)
                }
            })
            .map_err(|_| String::from("Global restore staging failed"))?;
    }
    Ok(())
}

pub(super) fn stage_skill_pool<R: Read + Seek>(
    desktop: &DesktopWorkspace,
    archive: &mut ZipArchive<R>,
    manifest: &Manifest,
    files: &mut RestoreFiles,
) -> Result<(), String> {
    let target = desktop.data_dir.join("skill_pool");
    let protected = protected_paths(&target, &desktop.data_dir)
        .map_err(|_| String::from("Skill pool restore target is invalid"))?;
    let payloads = collect_payloads(manifest, super::SKILL_POOL_PREFIX, &target, &protected)?;
    // Match the existing product: missing scope content never clears local data.
    if payloads.is_empty() {
        return Ok(());
    }
    stage_tree(&target, &payloads, &protected, archive, files)
        .map_err(|_| String::from("Skill pool restore staging failed"))
}

/// The caller owns an exclusive application restore lease and a validated
/// archive/manifest. Nothing is exchanged until the outer file transaction's
/// `apply`; it must be retained through credential and Core commit or rollback.
pub(super) fn stage_agents<R: Read + Seek>(
    desktop: &DesktopWorkspace,
    plan: &AgentRestorePlan,
    archive: &mut ZipArchive<R>,
    manifest: &Manifest,
    files: &mut RestoreFiles,
) -> Result<(), String> {
    let mut stages = Vec::new();
    for workspace in &plan.workspaces {
        let prefix = format!("{}{}/", super::WORKSPACE_PREFIX, workspace.id);
        let protected = protected_paths(&workspace.destination, &desktop.data_dir)
            .map_err(|_| String::from("Restore workspace could not be inspected"))?;
        let mut payloads = collect_payloads(manifest, &prefix, &workspace.destination, &protected)?;
        if payloads.iter().any(|payload| {
            payload
                .destination
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(MARKER_NAME))
        }) {
            return Err(String::from("Archive cannot provide a Workspace identity"));
        }
        let identity = plan
            .identity_markers
            .get(&workspace.destination)
            .ok_or_else(|| String::from("Planned Workspace identity is missing"))?;
        payloads.push(Payload {
            name: String::new(),
            destination: workspace.destination.join(MARKER_NAME),
            expected: ManifestEntry {
                size: identity.len() as u64,
                sha256: String::new(),
            },
            agent_config_mapping: None,
            inline_bytes: Some(identity.clone()),
        });
        if let Some(config) = payloads
            .iter_mut()
            .find(|payload| payload.destination == workspace.destination.join("agent.json"))
        {
            if config.expected.size > desktop_agents::MAX_AGENT_CONFIG_BYTES as u64 {
                return Err(String::from(
                    "Backup Agent configuration exceeds its size limit",
                ));
            }
            config.agent_config_mapping =
                Some((workspace.source_root.clone(), workspace.destination.clone()));
        }
        stages.push(WorkspaceStage {
            destination: workspace.destination.clone(),
            payloads,
            protected,
        });
    }
    // Complete target inspection for every selected Agent before staging any.
    for stage in &stages {
        stage_tree(
            &stage.destination,
            &stage.payloads,
            &stage.protected,
            archive,
            files,
        )
        .map_err(|_| String::from("Restore workspace staging failed"))?;
    }
    for (root, bytes) in &plan.identity_markers {
        if !root.is_dir()
            || plan
                .workspaces
                .iter()
                .any(|workspace| &workspace.destination == root)
        {
            continue;
        }
        let marker = root.join(MARKER_NAME);
        if fs::symlink_metadata(&marker).is_ok_and(|metadata| metadata.is_file())
            && fs::File::open(&marker)
                .and_then(|file| {
                    let mut current = Vec::new();
                    file.take(bytes.len() as u64 + 1)
                        .read_to_end(&mut current)?;
                    Ok(current == *bytes)
                })
                .unwrap_or(false)
        {
            continue;
        }
        files
            .stage_replace(&marker, |replacement| write_private(replacement, bytes))
            .map_err(|_| String::from("Restore Workspace identity staging failed"))?;
    }
    let catalog = desktop_agents::catalog_path(desktop);
    ensure_parent(&catalog, files)
        .and_then(|()| {
            files.stage_replace(&catalog, |replacement| {
                write_private(replacement, &plan.catalog)
            })
        })
        .map_err(|_| String::from("Restore Agent catalog staging failed"))
}

fn collect_payloads(
    manifest: &Manifest,
    prefix: &str,
    destination: &Path,
    protected: &[PathBuf],
) -> Result<Vec<Payload>, String> {
    let mut payloads = Vec::new();
    let mut keys = BTreeSet::new();
    let mut total = 0_u64;
    for (name, expected) in &manifest.entries {
        let Some(relative) = name.strip_prefix(prefix) else {
            continue;
        };
        super::validate_archive_name(name)?;
        if relative.split('/').any(is_recovery_name) {
            return Err(String::from(
                "Backup workspace contains reserved recovery data",
            ));
        }
        let target = destination.join(relative);
        let key = path_key(&target);
        if protected.iter().any(|path| {
            let protected = path_key(path);
            key.starts_with(&protected) || protected.starts_with(&key)
        }) {
            return Err(String::from(
                "Backup workspace would overwrite protected data",
            ));
        }
        if !keys.insert(key) {
            return Err(String::from("Backup workspace contains duplicate paths"));
        }
        total = total.saturating_add(expected.size);
        if expected.size > super::MAX_FILE_BYTES || total > super::MAX_ARCHIVE_BYTES {
            return Err(String::from("Backup workspace exceeds its size limit"));
        }
        payloads.push(Payload {
            name: name.clone(),
            destination: target,
            expected: expected.clone(),
            agent_config_mapping: None,
            inline_bytes: None,
        });
    }
    if payloads.len() > super::MAX_ARCHIVE_FILES {
        return Err(String::from("Backup workspace contains too many files"));
    }
    for path in &keys {
        if path.ancestors().skip(1).any(|parent| keys.contains(parent)) {
            return Err(String::from(
                "Backup workspace contains conflicting file and directory paths",
            ));
        }
    }
    Ok(payloads)
}

pub(super) fn stage_checkpoints<R: Read + Seek>(
    desktop: &DesktopWorkspace,
    plan: &AgentRestorePlan,
    archived_state: &qwenpaw_storage::StoreBackup,
    archive: &mut ZipArchive<R>,
    manifest: &Manifest,
    files: &mut RestoreFiles,
) -> Result<(), String> {
    let mut remaining = super::MAX_ARCHIVE_BYTES;
    for workspace in &plan.workspaces {
        let source_key = plan
            .archived_bindings
            .get(&workspace.id)
            .ok_or_else(|| String::from("Backup checkpoint Workspace binding is missing"))?;
        let target_key = plan
            .agents
            .agents
            .iter()
            .find(|agent| agent.id == workspace.id)
            .and_then(|agent| agent.data_key.as_ref())
            .ok_or_else(|| String::from("Restored checkpoint Workspace binding is missing"))?;
        let destination =
            super::super::desktop_checkpoints::state_directory(&desktop.data_dir, target_key);
        let protected = protected_paths(&destination, &desktop.data_dir)
            .map_err(|_| String::from("Checkpoint destination could not be inspected"))?;
        if !protected.is_empty() {
            return Err(String::from(
                "Checkpoint destination contains retained recovery data",
            ));
        }
        let prefix = format!("{}{}/", super::CHECKPOINT_PREFIX, workspace.id);
        let payloads = collect_payloads(manifest, &prefix, &destination, &[])?;
        let mut threads = archived_state.threads.clone();
        super::super::desktop_chats::filter_backup_threads(
            archived_state
                .settings
                .get("desktop_chat_catalog_data")
                .map(String::as_str),
            &BTreeSet::from([workspace.id.as_str()]),
            &mut threads,
            &plan.archived_bindings,
        )
        .map_err(str::to_owned)?;
        let allowed = threads.into_iter().map(|stored| stored.thread.id).collect();
        ensure_parent(&destination, files)
            .map_err(|_| String::from("Checkpoint destination is unavailable"))?;
        files
            .stage_replace(&destination, |replacement| {
                extract_tree(&destination, replacement, &payloads, archive)?;
                super::super::desktop_checkpoints::backup_restore::rewrite_staged(
                    &desktop.data_dir,
                    workspace,
                    replacement,
                    source_key,
                    target_key,
                    &allowed,
                    &mut remaining,
                )
                .map_err(io::Error::other)
            })
            .map_err(|_| String::from("Restore checkpoint staging failed"))?;
    }
    Ok(())
}

fn protected_paths(root: &Path, control: &Path) -> io::Result<Vec<PathBuf>> {
    let mut protected = Vec::new();
    let control_key = path_key(control);
    if control_key.starts_with(path_key(root)) {
        protected.push(control.to_path_buf());
    }
    let mut pending = vec![root.to_path_buf()];
    let mut inspected = 0;
    while let Some(directory) = pending.pop() {
        if path_key(&directory) == control_key {
            continue;
        }
        match fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.is_dir() && !is_link(&metadata) => {}
            Ok(_) => return Err(invalid("Restore directory changed")),
            Err(error) if error.kind() == io::ErrorKind::NotFound && directory == root => continue,
            Err(error) => return Err(error),
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound && directory == root => continue,
            Err(error) => return Err(error),
        };
        for entry in entries {
            inspected += 1;
            if inspected > super::MAX_ARCHIVE_FILES {
                return Err(invalid("Restore workspace contains too many entries"));
            }
            let entry = entry?;
            let path = entry.path();
            if is_recovery_name(&entry.file_name().to_string_lossy()) {
                protected.push(path);
            } else if path_key(&path) == control_key {
                // Keep the actual directory-entry spelling on filesystems
                // whose canonical paths can retain a caller's case alias.
                protected.push(path);
            } else {
                let metadata = fs::symlink_metadata(&path)?;
                if metadata.is_dir() && !is_link(&metadata) {
                    pending.push(path);
                }
            }
        }
    }
    protected.sort();
    protected.dedup();
    Ok(protected)
}

fn stage_tree<R: Read + Seek>(
    target: &Path,
    payloads: &[Payload],
    protected: &[PathBuf],
    archive: &mut ZipArchive<R>,
    files: &mut RestoreFiles,
) -> io::Result<()> {
    if protected.iter().any(|path| path == target) {
        return Ok(());
    }
    if !protected.iter().any(|path| path.starts_with(target)) {
        ensure_parent(target, files)?;
        return files.stage_workspace_replace(target, |replacement| {
            extract_tree(target, replacement, payloads, archive)
        });
    }
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(target)? {
        names.insert(entry?.file_name());
    }
    for payload in payloads {
        if let Ok(relative) = payload.destination.strip_prefix(target)
            && let Some(first) = relative.components().next()
        {
            names.insert(first.as_os_str().to_owned());
        }
    }
    for name in names {
        let child = target.join(name);
        if protected.iter().any(|path| path == &child)
            || is_recovery_name(&child.file_name().unwrap_or_default().to_string_lossy())
        {
            continue;
        }
        if protected.iter().any(|path| path.starts_with(&child)) {
            stage_tree(&child, payloads, protected, archive, files)?;
        } else if payloads
            .iter()
            .any(|payload| payload.destination.starts_with(&child))
        {
            files.stage_workspace_replace(&child, |replacement| {
                extract_tree(&child, replacement, payloads, archive)
            })?;
        } else {
            files.stage_workspace_delete(&child)?;
        }
    }
    Ok(())
}

fn extract_tree<R: Read + Seek>(
    target: &Path,
    replacement: &Path,
    payloads: &[Payload],
    archive: &mut ZipArchive<R>,
) -> io::Result<()> {
    if let Some(payload) = payloads
        .iter()
        .find(|payload| payload.destination == target)
    {
        return extract_file(replacement, payload, archive);
    }
    fs::create_dir(replacement)?;
    for payload in payloads {
        let Ok(relative) = payload.destination.strip_prefix(target) else {
            continue;
        };
        let path = replacement.join(relative);
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| invalid("Restore file has no parent"))?,
        )?;
        extract_file(&path, payload, archive)?;
    }
    Ok(())
}

fn extract_file<R: Read + Seek>(
    destination: &Path,
    payload: &Payload,
    archive: &mut ZipArchive<R>,
) -> io::Result<()> {
    if let Some(bytes) = &payload.inline_bytes {
        return write_private(destination, bytes);
    }
    let mut source = archive.by_name(&payload.name)?;
    if source.size() != payload.expected.size
        || source.unix_mode().is_some_and(|mode| {
            let kind = mode & 0o170_000;
            kind != 0 && kind != 0o100_000
        })
    {
        return Err(invalid("Restore archive entry changed"));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("Restore file has no parent"))?;
    let mut output = tempfile::NamedTempFile::new_in(parent)?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count as u64);
        if total > payload.expected.size {
            return Err(invalid("Restore archive entry changed"));
        }
        digest.update(&buffer[..count]);
        output.write_all(&buffer[..count])?;
    }
    if total != payload.expected.size
        || format!("{:x}", digest.finalize()) != payload.expected.sha256
    {
        return Err(invalid("Restore archive digest changed"));
    }
    if let Some((source_root, destination_root)) = &payload.agent_config_mapping {
        let mut config: serde_json::Value = serde_json::from_reader(output.reopen()?)?;
        if !config.is_object() {
            return Err(invalid("Restored Agent configuration is not an object"));
        }
        desktop_agents::restore::remap_agent_config(&mut config, source_root, destination_root);
        output.as_file_mut().set_len(0)?;
        output.seek(SeekFrom::Start(0))?;
        serde_json::to_writer_pretty(&mut output, &config)?;
    }
    output.as_file().sync_all()?;
    output
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

fn ensure_parent(target: &Path, files: &mut RestoreFiles) -> io::Result<()> {
    let mut path = target
        .parent()
        .ok_or_else(|| invalid("Restore target has no parent"))?;
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if !metadata.is_dir() || is_link(&metadata) || path.canonicalize()? != path {
                    return Err(invalid("Restore parent changed"));
                }
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing.push(path.to_path_buf());
                path = path
                    .parent()
                    .ok_or_else(|| invalid("Restore parent is unavailable"))?;
            }
            Err(error) => return Err(error),
        }
    }
    for path in missing.into_iter().rev() {
        files.create_directory(&path)?;
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Restore file has no parent"))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path).map_err(|error| error.error)?;
    Ok(())
}

fn path_key(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_lowercase())
}

fn is_recovery_name(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with(RECOVERY_PREFIX)
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
#[path = "desktop_backup_restore_workspaces_tests.rs"]
mod tests;
