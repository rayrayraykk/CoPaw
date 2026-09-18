//! Rebase validated checkpoint archives inside an owned restore staging tree.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use qwenpaw_core::Core;
use qwenpaw_core::ThreadCheckpoint;
use serde_json::Value;
use zip::ZipArchive;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use super::super::desktop_agents;
use super::super::desktop_agents::restore::WorkspaceRestore;
use super::super::desktop_agents::restore::remap_agent_config;
use super::super::desktop_agents::restore::remap_workspace_path;
use super::super::desktop_backups::validate_archive_name;
use super::CheckpointEntry;
use super::CheckpointIdentity;
use super::MAX_SNAPSHOT_BYTES;
use super::MAX_SNAPSHOT_FILES;
use super::MAX_THREAD_BYTES;
use super::WorkspaceContext;
use super::WorkspaceDataKey;

/// Only writes the caller-owned staging tree. The live checkpoint directory
/// remains under the surrounding file transaction's commit/rollback lifetime.
pub(crate) fn rewrite_staged(
    control_dir: &Path,
    workspace: &WorkspaceRestore,
    staged: &Path,
    source_key: &WorkspaceDataKey,
    target_key: &WorkspaceDataKey,
    allowed_threads: &BTreeSet<String>,
    remaining_expanded_bytes: &mut u64,
) -> Result<(), String> {
    let source = CheckpointIdentity {
        data_key: source_key.clone(),
        workspace_root: workspace.source_root.clone(),
    };
    let mut state = super::read_state(staged, &source).map_err(|_| {
        error("Backup checkpoint state is invalid or belongs to a different Workspace")
    })?;
    if state
        .entries
        .iter()
        .any(|entry| !allowed_threads.contains(&entry.thread_id))
    {
        return Err(error("Backup checkpoint references an unselected Thread"));
    }
    validate_staged_names(staged, &state.entries)?;
    if !staged.join("state.json").exists() {
        return Ok(());
    }
    let context = WorkspaceContext {
        root: workspace.destination.clone(),
        root_text: workspace.destination.to_string_lossy().into_owned(),
        state_dir: staged.to_path_buf(),
        control_dir: control_dir.to_path_buf(),
        identity: CheckpointIdentity {
            data_key: target_key.clone(),
            workspace_root: workspace.destination.to_string_lossy().into_owned(),
        },
    };
    let mut commits = BTreeMap::new();
    for entry in &state.entries {
        let commit = rewrite_snapshot(&context, &source, entry, remaining_expanded_bytes)?;
        commits.insert(entry.commit.clone(), commit);
    }
    if state.identity == context.identity && commits.iter().all(|(old, new)| old == new) {
        return Ok(());
    }
    state.identity = context.identity.clone();
    for entry in &mut state.entries {
        entry.commit = commits[&entry.commit].clone();
        if let Some(parent) = &mut entry.parent_commit
            && let Some(mapped) = commits.get(parent)
        {
            *parent = mapped.clone();
        }
    }
    for head in state.heads.values_mut() {
        *head = commits[head].clone();
    }
    super::validate_state(&state).map_err(error)?;
    super::write_state(staged, &state)
        .map_err(|_| error("Restored checkpoint state could not be staged"))?;
    let retained = commits.values().collect::<BTreeSet<_>>();
    for old in commits.keys().filter(|old| !retained.contains(old)) {
        fs::remove_file(staged.join("snapshots").join(format!("{old}.zip")))
            .map_err(|_| error("Old staged checkpoint copy could not be removed"))?;
    }
    Ok(())
}

fn validate_staged_names(staged: &Path, entries: &[CheckpointEntry]) -> Result<(), String> {
    let expected = entries
        .iter()
        .map(|entry| format!("{}.zip", entry.commit))
        .collect::<BTreeSet<_>>();
    for item in fs::read_dir(staged).map_err(|_| error("Checkpoint staging tree is unavailable"))? {
        let item = item.map_err(|_| error("Checkpoint staging entry is unavailable"))?;
        let path = item.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| error("Checkpoint staging entry is unavailable"))?;
        if item.file_name() == "state.json"
            && metadata.is_file()
            && !super::is_link_or_junction(&metadata)
        {
            continue;
        }
        if item.file_name() != "snapshots"
            || !metadata.is_dir()
            || super::is_link_or_junction(&metadata)
        {
            return Err(error("Backup checkpoint contains an unexpected entry"));
        }
        for snapshot in
            fs::read_dir(path).map_err(|_| error("Checkpoint snapshots are unavailable"))?
        {
            let snapshot = snapshot.map_err(|_| error("Checkpoint snapshot is unavailable"))?;
            if !snapshot
                .file_name()
                .to_str()
                .is_some_and(|name| expected.contains(name))
            {
                return Err(error("Backup checkpoint contains an unreferenced archive"));
            }
        }
    }
    Ok(())
}

fn rewrite_snapshot(
    context: &WorkspaceContext,
    source: &CheckpointIdentity,
    entry: &CheckpointEntry,
    remaining: &mut u64,
) -> Result<String, String> {
    let path = super::checkpoint_archive_path(context, entry)
        .map_err(|_| error("Backup checkpoint archive is unavailable"))?;
    super::verify_archive_digest(&path, &entry.commit)
        .map_err(|_| error("Backup checkpoint archive digest is invalid"))?;
    let input =
        fs::File::open(&path).map_err(|_| error("Backup checkpoint archive is unavailable"))?;
    let mut archive =
        ZipArchive::new(input).map_err(|_| error("Backup checkpoint archive is invalid"))?;
    if archive.len() > MAX_SNAPSHOT_FILES + 2 {
        return Err(error("Backup checkpoint has too many files"));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(context.state_dir.join("snapshots"))
        .map_err(|_| error("Checkpoint archive could not be staged"))?;
    let changed = rewrite_entries(
        &mut archive,
        temporary.as_file_mut(),
        context,
        source,
        entry,
        remaining,
    )?;
    if !changed {
        return Ok(entry.commit.clone());
    }
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|_| error("Checkpoint archive could not be staged"))?;
    if temporary
        .as_file()
        .metadata()
        .map_err(|_| error("Checkpoint archive is unavailable"))?
        .len()
        > MAX_SNAPSHOT_BYTES
    {
        return Err(error("Restored checkpoint archive exceeds its size limit"));
    }
    temporary
        .as_file_mut()
        .rewind()
        .map_err(|_| error("Checkpoint archive could not be verified"))?;
    let commit = super::reader_digest(temporary.as_file_mut())
        .map_err(|_| error("Checkpoint archive could not be verified"))?;
    temporary
        .persist_noclobber(
            context
                .state_dir
                .join("snapshots")
                .join(format!("{commit}.zip")),
        )
        .map_err(|_| error("Restored checkpoint archive conflicts with existing staged data"))?;
    Ok(commit)
}

fn rewrite_entries(
    archive: &mut ZipArchive<fs::File>,
    output: &mut fs::File,
    context: &WorkspaceContext,
    source: &CheckpointIdentity,
    checkpoint: &CheckpointEntry,
    remaining: &mut u64,
) -> Result<bool, String> {
    let mut writer = ZipWriter::new(output);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o600);
    let mut seen = BTreeSet::new();
    let mut changed = false;
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| error("Backup checkpoint entry is invalid"))?;
        let name = entry.name().to_owned();
        let key = PathBuf::from(name.to_lowercase());
        if entry.is_dir()
            || !seen.insert(key)
            || entry.unix_mode().is_some_and(|mode| {
                let kind = mode & 0o170_000;
                kind != 0 && kind != 0o100_000
            })
        {
            return Err(error("Backup checkpoint entry is unsafe or duplicated"));
        }
        for part in name.split('/') {
            validate_archive_name(part)?;
        }
        expanded = expanded.saturating_add(entry.size());
        *remaining = remaining
            .checked_sub(entry.size())
            .ok_or_else(|| error("Backup nested checkpoints exceed their expansion limit"))?;
        if expanded > MAX_SNAPSHOT_BYTES {
            return Err(error("Backup checkpoint exceeds its expansion limit"));
        }
        writer
            .start_file(&name, options)
            .map_err(|_| error("Checkpoint entry could not be staged"))?;
        match name.as_str() {
            "thread.json" => {
                let bytes = read_bounded(&mut entry, MAX_THREAD_BYTES as u64)?;
                let (bytes, mapped) =
                    remap_thread(&bytes, checkpoint, context, &source.workspace_root)?;
                changed |= mapped;
                writer
                    .write_all(&bytes)
                    .map_err(|_| error("Checkpoint Thread could not be staged"))?;
            }
            "checkpoint.id" => {
                let bytes = read_bounded(&mut entry, super::MAX_SNAPSHOT_IDENTITY_BYTES)?;
                let (bytes, mapped) = remap_snapshot_identity(&bytes, source, &context.identity)?;
                changed |= mapped;
                writer
                    .write_all(&bytes)
                    .map_err(|_| error("Checkpoint ID could not be staged"))?;
            }
            _ => {
                let relative = name
                    .strip_prefix("files/")
                    .ok_or_else(|| error("Backup checkpoint contains an unknown entry"))?;
                validate_destination(context, relative)?;
                if relative == "agent.json" {
                    let bytes =
                        read_bounded(&mut entry, desktop_agents::MAX_AGENT_CONFIG_BYTES as u64)?;
                    let mut config: Value = serde_json::from_slice(&bytes)
                        .map_err(|_| error("Checkpoint Agent config is invalid"))?;
                    if !config.is_object() {
                        return Err(error("Checkpoint Agent config is invalid"));
                    }
                    let original = config.clone();
                    remap_agent_config(&mut config, &source.workspace_root, &context.root);
                    changed |= config != original;
                    serde_json::to_writer(&mut writer, &config)
                        .map_err(|_| error("Checkpoint Agent config could not be staged"))?;
                } else {
                    copy_bounded(&mut entry, &mut writer)?;
                }
            }
        }
    }
    if !seen.contains(Path::new("thread.json"))
        || !seen.contains(Path::new("checkpoint.id"))
        || seen
            .iter()
            .any(|path| path.ancestors().skip(1).any(|parent| seen.contains(parent)))
    {
        return Err(error(
            "Backup checkpoint identities or file paths are invalid",
        ));
    }
    writer
        .finish()
        .map_err(|_| error("Checkpoint archive could not be finalized"))?;
    Ok(changed)
}

fn remap_snapshot_identity(
    bytes: &[u8],
    source: &CheckpointIdentity,
    target: &CheckpointIdentity,
) -> Result<(Vec<u8>, bool), String> {
    let mut identity = super::decode_snapshot_identity(bytes)
        .map_err(|_| error("Backup checkpoint identity is invalid or unbound"))?;
    if identity.workspace != *source {
        return Err(error("Backup checkpoint Workspace identity is invalid"));
    }
    identity.workspace = target.clone();
    let bytes = serde_json::to_vec(&identity)
        .map_err(|_| error("Checkpoint identity could not be staged"))?;
    if bytes.len() as u64 > super::MAX_SNAPSHOT_IDENTITY_BYTES {
        return Err(error("Checkpoint identity exceeds its size limit"));
    }
    Ok((bytes, source != target))
}

fn remap_thread(
    bytes: &[u8],
    expected: &CheckpointEntry,
    context: &WorkspaceContext,
    source_root: &str,
) -> Result<(Vec<u8>, bool), String> {
    let mut checkpoint: ThreadCheckpoint =
        serde_json::from_slice(bytes).map_err(|_| error("Backup checkpoint Thread is invalid"))?;
    if checkpoint.thread.id != expected.thread_id {
        return Err(error("Backup checkpoint Thread identity is invalid"));
    }
    Core::validate_thread_checkpoint(&checkpoint)
        .map_err(|_| error("Backup checkpoint conversation state is invalid"))?;
    let before = checkpoint.thread.workspace_root.clone();
    if let Some(root) = &before
        && let Some(mapped) = remap_workspace_path(root, source_root, &context.root)
    {
        checkpoint.thread.workspace_root = Some(mapped);
    }
    let changed = checkpoint.thread.workspace_root != before;
    let bytes = serde_json::to_vec(&checkpoint)
        .map_err(|_| error("Restored checkpoint Thread is invalid"))?;
    if bytes.len() > MAX_THREAD_BYTES {
        return Err(error("Restored checkpoint Thread exceeds its size limit"));
    }
    Ok((bytes, changed))
}

fn validate_destination(context: &WorkspaceContext, relative: &str) -> Result<(), String> {
    super::validate_relative_path(relative)
        .map_err(|_| error("Backup checkpoint path is invalid"))?;
    let path = context.root.join(relative);
    let key = PathBuf::from(path.to_string_lossy().to_lowercase());
    let control = PathBuf::from(context.control_dir.to_string_lossy().to_lowercase());
    let root = PathBuf::from(context.root.to_string_lossy().to_lowercase());
    if super::excluded_directory(relative)
        || super::excluded_file(relative)
        || relative.split('/').any(|part| {
            part.to_ascii_lowercase()
                .starts_with(super::super::desktop_restore_files::RECOVERY_PREFIX)
        })
        || (control.starts_with(&root) && (key.starts_with(&control) || control.starts_with(&key)))
    {
        return Err(error("Backup checkpoint contains a protected destination"));
    }
    Ok(())
}

fn read_bounded(
    entry: &mut zip::read::ZipFile<'_, fs::File>,
    limit: u64,
) -> Result<Vec<u8>, String> {
    if entry.size() > limit {
        return Err(error("Backup checkpoint entry exceeds its size limit"));
    }
    let mut bytes = Vec::new();
    copy_bounded(entry, &mut bytes)?;
    Ok(bytes)
}

fn copy_bounded(
    entry: &mut zip::read::ZipFile<'_, fs::File>,
    output: &mut impl Write,
) -> Result<(), String> {
    let expected = entry.size();
    let count = io::copy(&mut entry.take(expected.saturating_add(1)), output)
        .map_err(|_| error("Backup checkpoint entry could not be decoded"))?;
    if count != expected {
        return Err(error("Backup checkpoint entry size changed"));
    }
    Ok(())
}

fn error(message: &str) -> String {
    message.to_owned()
}

#[cfg(test)]
#[path = "desktop_checkpoint_backup_restore_tests.rs"]
mod tests;
