//! Native Workspace identity, independent of reusable public Agent IDs.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read as _;
use std::io::Write as _;
use std::path::Path;

use uuid::Uuid;

use super::AgentCatalog;
use super::ApiError;
use super::CATALOG_SCHEMA_VERSION;
use super::internal;
use super::validate_agent_id;
use super::validate_registered_workspace_path;

pub(crate) const MARKER_NAME: &str = ".qwenpaw-workspace.json";
const MAX_MARKER_BYTES: u64 = 512;

pub(crate) use qwenpaw_storage::WorkspaceDataKey;

pub(super) fn hydrate_legacy(catalog: &mut AgentCatalog) -> Result<(), ApiError> {
    if catalog.schema_version == 2 {
        catalog.schema_version = CATALOG_SCHEMA_VERSION;
        catalog.bootstrap_identity = true;
        return Ok(());
    }
    if catalog.schema_version != 1 {
        return Ok(());
    }
    if !catalog.workspace_keys.is_empty()
        || catalog
            .agents
            .values()
            .any(|agent| agent.data_key.is_some())
    {
        return Err(internal(
            "Legacy Agent catalog contains Workspace identities",
        ));
    }
    for (id, agent) in &mut catalog.agents {
        let key = WorkspaceDataKey::LegacyAgent(id.clone());
        agent.data_key = Some(key.clone());
        if catalog
            .workspace_keys
            .insert(agent.workspace_dir.clone(), key)
            .is_some()
        {
            return Err(internal("Agent Workspaces are not unique"));
        }
    }
    catalog.schema_version = CATALOG_SCHEMA_VERSION;
    catalog.bootstrap_identity = true;
    Ok(())
}

pub(super) fn key_for_workspace(
    catalog: &AgentCatalog,
    path: &Path,
    created: bool,
) -> Result<WorkspaceDataKey, ApiError> {
    let marker = read_marker(path)?;
    if !created
        && let Some(key) = catalog.workspace_keys.get(path.to_string_lossy().as_ref())
        && marker.as_ref() == Some(key)
    {
        return Ok(key.clone());
    }
    Ok(WorkspaceDataKey::Workspace(Uuid::now_v7()))
}

pub(super) fn bind_workspace(
    catalog: &mut AgentCatalog,
    path: &Path,
    created: bool,
) -> Result<WorkspaceDataKey, ApiError> {
    let key = key_for_workspace(catalog, path, created)?;
    catalog
        .workspace_keys
        .insert(path.to_string_lossy().into_owned(), key.clone());
    Ok(key)
}

pub(super) fn bootstrap(catalog: &AgentCatalog) -> Result<(), ApiError> {
    for (path, key) in &catalog.workspace_keys {
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_dir() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            _ => return Err(internal("Workspace identity root is unavailable")),
        }
        if read_marker(Path::new(path))?
            .as_ref()
            .is_some_and(|marker| marker != key)
        {
            return Err(internal(
                "Existing Workspace identity conflicts with native upgrade",
            ));
        }
        write_marker(Path::new(path), key)?;
    }
    Ok(())
}

fn read_marker(root: &Path) -> Result<Option<WorkspaceDataKey>, ApiError> {
    read_marker_bytes(root)?
        .map(|bytes| decode_marker(&bytes))
        .transpose()
}

pub(crate) fn matches_marker(root: &Path, key: &WorkspaceDataKey) -> Result<bool, ApiError> {
    Ok(read_marker(root)?.as_ref() == Some(key))
}

fn read_marker_bytes(root: &Path) -> Result<Option<Vec<u8>>, ApiError> {
    let path = root.join(MARKER_NAME);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_MARKER_BYTES => {}
        _ => return Err(internal("Workspace identity file is invalid")),
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(MAX_MARKER_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| internal("Workspace identity could not be read"))?;
    if bytes.len() as u64 > MAX_MARKER_BYTES {
        return Err(internal("Workspace identity file is too large"));
    }
    Ok(Some(bytes))
}

fn decode_marker(bytes: &[u8]) -> Result<WorkspaceDataKey, ApiError> {
    let key: WorkspaceDataKey = serde_json::from_slice(bytes)
        .map_err(|_| internal("Workspace identity file is invalid"))?;
    match &key {
        WorkspaceDataKey::LegacyAgent(id) => validate_agent_id(id, true)?,
        WorkspaceDataKey::Workspace(id) if id.is_nil() => {
            return Err(internal("Workspace identity is invalid"));
        }
        WorkspaceDataKey::Workspace(_) => {}
    }
    Ok(key)
}

pub(super) fn marker_for_binding(root: &Path, key: &WorkspaceDataKey) -> Result<Vec<u8>, ApiError> {
    if let Some(bytes) = read_marker_bytes(root)?
        && &decode_marker(&bytes)? == key
    {
        return Ok(bytes);
    }
    marker_bytes(key)
}

pub(super) fn marker_bytes(key: &WorkspaceDataKey) -> Result<Vec<u8>, ApiError> {
    serde_json::to_vec(key).map_err(|_| internal("Workspace identity could not be encoded"))
}

pub(super) fn write_marker(root: &Path, key: &WorkspaceDataKey) -> Result<(), ApiError> {
    if read_marker(root)?.as_ref() == Some(key) {
        return Ok(());
    }
    let bytes = marker_bytes(key)?;
    let mut staged = tempfile::Builder::new()
        .prefix(".qwenpaw-write-")
        .tempfile_in(root)
        .map_err(|_| internal("Workspace identity could not be staged"))?;
    staged
        .write_all(&bytes)
        .and_then(|()| staged.flush())
        .map_err(|_| internal("Workspace identity could not be staged"))?;
    staged
        .persist(root.join(MARKER_NAME))
        .map_err(|_| internal("Workspace identity could not be published"))?;
    Ok(())
}

pub(super) fn validate(catalog: &AgentCatalog) -> Result<(), ApiError> {
    let mut retained = BTreeSet::new();
    for (path, key) in &catalog.workspace_keys {
        validate_registered_workspace_path(path)?;
        match key {
            WorkspaceDataKey::LegacyAgent(id) => validate_agent_id(id, true)?,
            WorkspaceDataKey::Workspace(id) if id.is_nil() => {
                return Err(internal("Workspace identity is invalid"));
            }
            WorkspaceDataKey::Workspace(_) => {}
        }
        if !retained.insert(key) {
            return Err(internal("Workspace identities are not unique"));
        }
    }
    let mut active = BTreeSet::new();
    for agent in catalog.agents.values() {
        let Some(key) = &agent.data_key else {
            return Err(internal("Agent Workspace identity is missing"));
        };
        if catalog.workspace_keys.get(&agent.workspace_dir) != Some(key) || !active.insert(key) {
            return Err(internal("Agent Workspace binding is invalid"));
        }
    }
    Ok(())
}
