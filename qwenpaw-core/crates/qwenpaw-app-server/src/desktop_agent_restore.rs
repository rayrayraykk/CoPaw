//! Read-only Agent registry and workspace planning for full/custom restoration.

use std::collections::BTreeSet;
use std::io;

use std::collections::BTreeMap;
use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use serde_json::Value;
use serde_json::json;

use super::AgentBackupSnapshot;
use super::AgentCatalog;
use super::AgentReference;
use super::AgentRegistryBackup;
use super::BackupAgent;
use super::CATALOG_SCHEMA_VERSION;
use super::DesktopWorkspace;
use super::MAX_CATALOG_BYTES;
use super::default_agent_config;
use super::grouped_order;
use super::identity::bind_workspace;
use super::read_catalog;
use super::validate_backup_snapshot;
use super::validate_catalog;
use super::validate_registry_backup;

#[derive(Debug)]
pub(crate) struct AgentRestorePlan {
    pub(crate) catalog: Vec<u8>,
    pub(crate) agents: AgentBackupSnapshot,
    pub(crate) workspaces: Vec<WorkspaceRestore>,
    pub(crate) identity_markers: BTreeMap<PathBuf, Vec<u8>>,
    pub(crate) current_bindings: BTreeMap<String, super::WorkspaceDataKey>,
    pub(crate) archived_bindings: BTreeMap<String, super::WorkspaceDataKey>,
    #[allow(dead_code)] // Reserved for per-Agent runtime unload coordination.
    pub(crate) removed_agent_ids: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct WorkspaceRestore {
    pub(crate) id: String,
    pub(crate) source_root: String,
    pub(crate) destination: PathBuf,
}

/// `full_registry` is supplied only for full mode with global config selected.
/// Missing scope/payload never grants permission to remove a workspace.
pub(crate) fn plan_restore(
    desktop: &DesktopWorkspace,
    archived_workspaces: &AgentBackupSnapshot,
    full_registry: Option<&AgentRegistryBackup>,
    selected_ids: &BTreeSet<String>,
    fallback_directory: Option<&str>,
) -> Result<AgentRestorePlan, String> {
    validate_backup_snapshot(archived_workspaces)?;
    if let Some(registry) = full_registry {
        validate_registry_backup(registry)?;
    }
    let current =
        read_catalog(desktop).map_err(|_| String::from("Local Agent registry is invalid"))?;
    let base = fallback_base(desktop, fallback_directory)?;
    let selected = archived_workspaces
        .agents
        .iter()
        .filter(|agent| selected_ids.contains(&agent.id))
        .map(|agent| (agent.id.as_str(), agent))
        .collect::<BTreeMap<_, _>>();
    let mut next = match full_registry {
        None => current.clone(),
        Some(registry) => restore_registry(registry, &current, &base)?,
    };
    let mut workspaces = Vec::new();
    // Retain the archive's ordering when adding selected Agents in custom mode.
    for archived in &archived_workspaces.agents {
        if !selected.contains_key(archived.id.as_str()) {
            continue;
        }
        if let Some(registry) = full_registry
            && !registry.agents.contains(&archived.reference())
        {
            return Err(String::from("Backup Agent snapshots disagree"));
        }
        let destination = destination_for(&archived.id, &current, &base)?;
        validate_destination(desktop, &destination)?;
        let mut config = archived.config.clone();
        remap_agent_config(&mut config, &archived.workspace_dir, &destination);
        if !next.agents.contains_key(&archived.id) {
            next.order.push(archived.id.clone());
        }
        let data_key = bind_workspace(&mut next, &destination, !destination.is_dir())
            .map_err(|_| String::from("Restore Workspace identity is unavailable"))?;
        next.agents.insert(
            archived.id.clone(),
            AgentReference {
                workspace_dir: destination.to_string_lossy().into_owned(),
                enabled: archived.enabled,
                pinned: archived.pinned,
                config: config.clone(),
                data_key: Some(data_key),
            },
        );
        workspaces.push(WorkspaceRestore {
            id: archived.id.clone(),
            source_root: archived.workspace_dir.clone(),
            destination,
        });
    }
    for agent in next.agents.values_mut() {
        agent.config["workspace_dir"] = json!(agent.workspace_dir);
    }
    validate_planned_destinations(desktop, &current, &next, &workspaces)?;
    next.order = grouped_order(&next, &next.order);
    let mut plan = finish_plan(desktop, &current, next, workspaces)?;
    plan.archived_bindings = archived_workspaces
        .agents
        .iter()
        .map(|agent| {
            (
                agent.id.clone(),
                agent
                    .data_key
                    .clone()
                    .unwrap_or_else(|| super::WorkspaceDataKey::LegacyAgent(agent.id.clone())),
            )
        })
        .collect();
    Ok(plan)
}

fn validate_planned_destinations(
    desktop: &DesktopWorkspace,
    current: &AgentCatalog,
    next: &AgentCatalog,
    workspaces: &[WorkspaceRestore],
) -> Result<(), String> {
    let mut destinations = workspaces
        .iter()
        .map(|target| (target.id.clone(), target.destination.clone()))
        .collect::<Vec<_>>();
    for (id, agent) in &next.agents {
        if workspaces.iter().any(|target| &target.id == id) {
            continue;
        }
        let destination = PathBuf::from(&agent.workspace_dir);
        if current
            .agents
            .get(id)
            .is_none_or(|old| old.workspace_dir != agent.workspace_dir)
        {
            validate_destination(desktop, &destination)?;
            destinations.push((id.clone(), destination));
        }
    }
    validate_collisions(desktop, current, &destinations)
}

fn finish_plan(
    desktop: &DesktopWorkspace,
    current: &AgentCatalog,
    mut next: AgentCatalog,
    workspaces: Vec<WorkspaceRestore>,
) -> Result<AgentRestorePlan, String> {
    next.revision = current
        .revision
        .checked_add(1)
        .ok_or_else(|| String::from("Agent registry revision overflow"))?;
    validate_catalog(&next, desktop)
        .map_err(|_| String::from("Restored Agent registry is invalid"))?;
    let catalog = serde_json::to_vec_pretty(&next)
        .map_err(|_| String::from("Restored Agent registry is invalid"))?;
    if catalog.len() as u64 > MAX_CATALOG_BYTES {
        return Err(String::from("Restored Agent registry is too large"));
    }
    let removed_agent_ids = current
        .order
        .iter()
        .filter(|id| !next.agents.contains_key(*id))
        .cloned()
        .collect();
    let agents = next
        .order
        .iter()
        .map(|id| {
            let agent = &next.agents[id];
            BackupAgent {
                id: id.clone(),
                workspace_dir: agent.workspace_dir.clone(),
                enabled: agent.enabled,
                pinned: agent.pinned,
                config: agent.config.clone(),
                data_key: agent.data_key.clone(),
            }
        })
        .collect();
    let agents = AgentBackupSnapshot { version: 2, agents };
    validate_backup_snapshot(&agents)?;
    let identity_markers = next
        .agents
        .values()
        .map(|agent| {
            let key = agent
                .data_key
                .as_ref()
                .ok_or_else(|| String::from("Restore identity is missing"))?;
            let bytes = super::identity::marker_for_binding(Path::new(&agent.workspace_dir), key)
                .map_err(|_| String::from("Restore identity is invalid"))?;
            Ok((PathBuf::from(&agent.workspace_dir), bytes))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    Ok(AgentRestorePlan {
        catalog,
        agents,
        workspaces,
        identity_markers,
        current_bindings: current
            .agents
            .iter()
            .filter_map(|(id, agent)| agent.data_key.clone().map(|key| (id.clone(), key)))
            .collect(),
        archived_bindings: BTreeMap::new(),
        removed_agent_ids,
    })
}

fn restore_registry(
    registry: &AgentRegistryBackup,
    current: &AgentCatalog,
    base: &Path,
) -> Result<AgentCatalog, String> {
    let mut next = AgentCatalog {
        schema_version: CATALOG_SCHEMA_VERSION,
        revision: current.revision,
        order: Vec::new(),
        agents: BTreeMap::new(),
        workspace_keys: current.workspace_keys.clone(),
        bootstrap_identity: false,
    };
    for reference in &registry.agents {
        let destination = destination_for(&reference.id, current, base)?;
        let config = current.agents.get(&reference.id).map_or_else(
            || {
                default_agent_config(
                    &reference.id,
                    &reference.id,
                    "",
                    &destination,
                    "qwenpaw",
                    "en",
                    None,
                )
            },
            |agent| {
                let mut config = agent.config.clone();
                remap_agent_config(&mut config, &agent.workspace_dir, &destination);
                config
            },
        );
        next.order.push(reference.id.clone());
        let data_key = bind_workspace(&mut next, &destination, !destination.is_dir())
            .map_err(|_| String::from("Restore Workspace identity is unavailable"))?;
        next.agents.insert(
            reference.id.clone(),
            AgentReference {
                workspace_dir: destination.to_string_lossy().into_owned(),
                enabled: reference.enabled,
                pinned: reference.pinned,
                config,
                data_key: Some(data_key),
            },
        );
    }
    Ok(next)
}

fn fallback_base(desktop: &DesktopWorkspace, value: Option<&str>) -> Result<PathBuf, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(desktop.data_dir.join("workspaces"));
    };
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(String::from("Restore workspace base is invalid"));
    }
    let path = if value == "~" {
        dirs::home_dir().ok_or_else(|| String::from("Home directory is unavailable"))?
    } else if let Some(relative) = value.strip_prefix("~/") {
        dirs::home_dir()
            .ok_or_else(|| String::from("Home directory is unavailable"))?
            .join(relative)
    } else {
        std::path::absolute(value)
            .map_err(|_| String::from("Restore workspace base is unavailable"))?
    };
    resolve_missing_directory(&path)
}

fn destination_for(id: &str, current: &AgentCatalog, base: &Path) -> Result<PathBuf, String> {
    if let Some(agent) = current.agents.get(id) {
        match fs::metadata(&agent.workspace_dir) {
            Ok(metadata) if metadata.is_dir() => {
                return Path::new(&agent.workspace_dir)
                    .canonicalize()
                    .map_err(|_| String::from("Local Agent workspace is unavailable"));
            }
            Ok(_) => return Err(String::from("Local Agent workspace is not a directory")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(String::from("Local Agent workspace is unavailable")),
        }
    }
    resolve_missing_directory(&base.join(id))
}

fn resolve_missing_directory(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
        return Err(String::from("Restore workspace path is invalid"));
    }
    let mut ancestor = path.to_path_buf();
    let mut suffix = Vec::new();
    loop {
        match fs::symlink_metadata(&ancestor) {
            Ok(_) => {
                let mut resolved = ancestor
                    .canonicalize()
                    .map_err(|_| String::from("Restore workspace ancestor is unavailable"))?;
                if !resolved.is_dir() {
                    return Err(String::from(
                        "Restore workspace ancestor is not a directory",
                    ));
                }
                for name in suffix.into_iter().rev() {
                    resolved.push(name);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let name = ancestor
                    .file_name()
                    .ok_or_else(|| String::from("Restore workspace path is invalid"))?
                    .to_owned();
                suffix.push(name);
                if !ancestor.pop() {
                    return Err(String::from("Restore workspace ancestor is unavailable"));
                }
            }
            Err(_) => return Err(String::from("Restore workspace ancestor is unavailable")),
        }
    }
}

fn validate_destination(desktop: &DesktopWorkspace, destination: &Path) -> Result<(), String> {
    let managed = desktop.data_dir.join("workspaces");
    if destination.starts_with(&desktop.data_dir)
        && (destination == managed || !destination.starts_with(&managed))
    {
        return Err(String::from("Restore workspace overlaps Core control data"));
    }
    Ok(())
}

fn overlaps(left: &Path, right: &Path, control: &Path) -> bool {
    if left == right {
        return true;
    }
    let (parent, child) = if left.starts_with(right) {
        (right, left)
    } else if right.starts_with(left) {
        (left, right)
    } else {
        return false;
    };
    // A workspace containing Core data does not own the protected data subtree.
    !(control.starts_with(parent) && child.starts_with(control) && parent != control)
}

fn validate_collisions(
    desktop: &DesktopWorkspace,
    current: &AgentCatalog,
    targets: &[(String, PathBuf)],
) -> Result<(), String> {
    for (index, (target_id, destination)) in targets.iter().enumerate() {
        for (_, other) in &targets[..index] {
            if overlaps(destination, other, &desktop.data_dir) {
                return Err(String::from("Restored Agent workspaces overlap"));
            }
        }
        for (id, other) in &current.agents {
            if id == target_id || targets.iter().any(|(target_id, _)| target_id == id) {
                continue;
            }
            let other = resolve_missing_directory(Path::new(&other.workspace_dir))?;
            if overlaps(destination, &other, &desktop.data_dir) {
                return Err(String::from(
                    "Restore workspace overlaps an unselected Agent",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn remap_agent_config(config: &mut Value, source: &str, destination: &Path) {
    if let Some(project) = config.get("project_dir").and_then(Value::as_str)
        && let Some(mapped) = remap_workspace_path(project, source, destination)
    {
        config["project_dir"] = json!(mapped);
    }
    config["workspace_dir"] = json!(destination.to_string_lossy());
}

/// Map only the source Workspace and its descendants. External projects remain
/// references: the archive contains neither their files nor a target mapping.
pub(crate) fn remap_workspace_path(
    value: &str,
    source: &str,
    destination: &Path,
) -> Option<String> {
    let source = source.replace('\\', "/");
    let value = value.replace('\\', "/");
    if source.starts_with('/') != value.starts_with('/')
        || source.starts_with("//") != value.starts_with("//")
        || source.is_empty()
    {
        return None;
    }
    let windows = source.starts_with("//") || source.as_bytes().get(1) == Some(&b':');
    let root = source
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let parts = value
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < root.len()
        || !root.iter().zip(&parts).all(|(left, right)| {
            if windows {
                left.to_lowercase() == right.to_lowercase()
            } else {
                left == right
            }
        })
    {
        return None;
    }
    let suffix = &parts[root.len()..];
    if root.iter().any(|part| matches!(*part, "." | ".."))
        || suffix
            .iter()
            .any(|part| matches!(*part, "." | "..") || part.contains(':'))
    {
        return None;
    }
    Some(
        suffix
            .iter()
            .fold(destination.to_path_buf(), |path, part| path.join(part))
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(test)]
#[path = "desktop_agent_restore_tests.rs"]
mod tests;
