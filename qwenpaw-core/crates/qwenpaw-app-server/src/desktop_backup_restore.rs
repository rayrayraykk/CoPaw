//! Application-owned Backup restore transaction for the unchanged Console.

use std::collections::BTreeSet;
use std::io::SeekFrom;
use std::sync::atomic::Ordering;

use qwenpaw_core::CoreRestoreGuard;
use qwenpaw_core::McpOAuthRestore;
use serde::Deserialize;

use super::*;
use crate::desktop_agent_settings;
use crate::desktop_agents;
use crate::desktop_environment;
use crate::desktop_local_models;
use crate::desktop_mcp;
use crate::desktop_models;
use crate::desktop_restore_files::RestoreFiles;
use restore_credentials::CandidateCredentials;
use restore_credentials::CredentialKey;
use restore_credentials::CredentialRestore;

#[derive(Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum RestoreMode {
    Full,
    #[default]
    Custom,
}

#[derive(Deserialize)]
#[allow(clippy::struct_excessive_bools)] // Existing Console wire contract.
pub(super) struct RestoreRequest {
    #[serde(default = "enabled")]
    include_agents: bool,
    #[serde(default)]
    agent_ids: Vec<String>,
    #[serde(default = "enabled")]
    include_global_config: bool,
    #[serde(default)]
    include_secrets: bool,
    #[serde(default = "enabled")]
    include_skill_pool: bool,
    #[serde(default)]
    default_workspace_dir: Option<String>,
    #[serde(default)]
    mode: RestoreMode,
    #[serde(default)]
    preserve_local_protected_config: Option<bool>,
    #[serde(default)]
    trust_mode: Option<TrustMode>,
}

/// Retain the exclusive lease and originals after an inverse operation fails.
/// A subsequent explicit restore request retries recovery before doing new work.
pub(super) struct FailedRestore {
    lease: CoreRestoreGuard,
    files: RestoreFiles,
    credentials: CredentialRestore,
    oauth: Option<McpOAuthRestore>,
}

impl FailedRestore {
    fn rollback(&mut self) -> bool {
        let oauth = self
            .oauth
            .as_mut()
            .is_none_or(|restore| restore.rollback().is_ok());
        let credentials = self.credentials.rollback().is_ok();
        let files = self.files.rollback().is_ok();
        oauth && credentials && files
    }
}

/// Retry retained inverse operations once after owned workers have drained.
/// Keep the application gate closed: shutdown must not resume normal work.
pub(super) async fn recover_on_shutdown(server: &AppServer) {
    let Ok(state) = backup_state(server) else {
        return;
    };
    let state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut coordinator = state.coordinator.blocking_lock();
        if let Some(pending) = coordinator.recovery.as_mut() {
            if pending.rollback() {
                coordinator.recovery = None;
            } else {
                warn!("Restore rollback remains incomplete at shutdown; recovery data retained");
            }
        }
    })
    .await;
    if result.is_err() {
        warn!("Restore shutdown recovery task failed; application remains stopped");
    }
}

pub(super) async fn restore_backup(
    State(server): State<AppServer>,
    AxumPath(id): AxumPath<String>,
    Json(request): Json<RestoreRequest>,
) -> Result<Json<Value>, ApiError> {
    server.ensure_agent_publication_available()?;
    validate_backup_id(&id)?;
    if request.agent_ids.len() > 256 {
        return Err(bad_request("Too many restore Agent identifiers"));
    }
    if request.include_agents {
        for id in &request.agent_ids {
            desktop_agents::validate_agent_id(id, true)
                .map_err(|_| bad_request("Invalid restore Agent identifier"))?;
        }
    }
    let state = backup_state(&server)?.clone();
    let recovery = {
        let mut coordinator = state.coordinator.lock().await;
        if server.inner.shutdown.is_cancelled()
            || coordinator.active_job.is_some()
            || (coordinator.restore_active && coordinator.recovery.is_none())
        {
            return Err(conflict("Backup operation already running"));
        }
        coordinator.restore_active = true;
        state.restoring.store(true, Ordering::Release);
        coordinator.recovery.take()
    };
    let workers = state.workers.clone();
    workers
        .spawn_blocking(move || {
            tokio::runtime::Handle::current().block_on(async move {
                let mut stopped = recovery.is_some();
                let result = if let Some(mut pending) = recovery {
                    if pending.rollback() {
                        drop(pending);
                        execute(&server, &state, &id, request, &mut stopped).await
                    } else {
                        state.coordinator.lock().await.recovery = Some(pending);
                        Err(internal(
                            "Restore rollback remains incomplete; recovery data retained",
                        ))
                    }
                } else {
                    execute(&server, &state, &id, request, &mut stopped).await
                };
                if state.coordinator.lock().await.recovery.is_none() {
                    if stopped && !server.inner.shutdown.is_cancelled() {
                        // Match ordinary startup: unavailable local assets fall back
                        // to the persisted remote provider, rather than a dead port.
                        desktop_local_models::resume(&server).await;
                    }
                    server
                        .inner
                        .desktop_heartbeat_revision
                        .send_modify(|value| *value = value.wrapping_add(1));
                    state.coordinator.lock().await.restore_active = false;
                    state.restoring.store(false, Ordering::Release);
                }
                result
            })
        })
        .await
        .map_err(|_| internal("Restore worker failed; inspect recovery state before retrying"))?
}

fn private_archive(
    server: &AppServer,
    id: &str,
) -> Result<(ZipArchive<fs::File>, PathBuf), ApiError> {
    let source = find_archive(&backups_directory(server)?, id)
        .map_err(|_| internal("Backup could not be located"))?
        .ok_or_else(|| not_found("Backup not found"))?;
    let metadata =
        fs::symlink_metadata(&source).map_err(|_| bad_request("Backup is unavailable"))?;
    if !metadata.is_file() || metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(bad_request("Backup archive is invalid or too large"));
    }
    let source_file = fs::File::open(&source).map_err(|_| bad_request("Backup is unavailable"))?;
    let mut private =
        tempfile::tempfile().map_err(|_| internal("Private restore staging failed"))?;
    let size = std::io::copy(&mut source_file.take(MAX_ARCHIVE_BYTES + 1), &mut private)
        .map_err(|_| internal("Private restore staging failed"))?;
    if size > MAX_ARCHIVE_BYTES {
        return Err(bad_request("Backup archive is too large"));
    }
    private
        .seek(SeekFrom::Start(0))
        .map_err(|_| internal("Private restore staging failed"))?;
    Ok((
        ZipArchive::new(private).map_err(|_| bad_request("Backup is not a ZIP"))?,
        source,
    ))
}

fn optional_entry<T: for<'de> Deserialize<'de>>(
    archive: &mut ZipArchive<fs::File>,
    manifest: &Manifest,
    name: &str,
) -> Result<Option<T>, ApiError> {
    manifest
        .entries
        .contains_key(name)
        .then(|| {
            read_json_entry(archive, name, MAX_FILE_BYTES)
                .map_err(|_| bad_request("Backup restore metadata is invalid"))
        })
        .transpose()
}

#[allow(clippy::too_many_lines)]
async fn execute(
    server: &AppServer,
    state: &BackupsState,
    id: &str,
    request: RestoreRequest,
    stopped: &mut bool,
) -> Result<Json<Value>, ApiError> {
    let (mut archive, source) = private_archive(server, id)?;
    let key = signing_key(credentials(server)?.as_ref())
        .map_err(|_| internal("Backup signing key could not be loaded"))?;
    let mut validated =
        validate_open_archive(&mut archive, &key).map_err(|error| bad_request(&error))?;
    require_trust(&validated.trust, request.trust_mode)?;
    if !matches!(validated.trust, ArchiveTrust::Local) {
        // Trust acceptance is an explicit action of its own, as on import.
        // Sign the validated private copy before restore preflight; a later
        // restore failure does not revoke the user's recorded trust decision.
        validated.meta.accepted_via_trust = Some(true);
        resign_open_archive(
            &mut archive,
            &backups_directory(server)?,
            &mut validated.meta,
            &key,
        )
        .map_err(|_| internal("Trusted backup could not be signed"))?
        .persist(&source)
        .map_err(|_| internal("Backup trust acceptance could not be persisted"))?;
    }
    let preserve = request.preserve_local_protected_config.unwrap_or(
        validated.meta.accepted_via_trust == Some(true)
            || !matches!(validated.trust, ArchiveTrust::Local),
    );
    let manifest: Manifest = read_json_entry(&mut archive, MANIFEST_FILE, 8 * 1024 * 1024)
        .map_err(|error| bad_request(&error))?;
    let archived_state: Option<qwenpaw_storage::StoreBackup> =
        optional_entry(&mut archive, &manifest, CORE_STATE_FILE)?;
    let agents: desktop_agents::AgentBackupSnapshot =
        optional_entry(&mut archive, &manifest, AGENT_STATE_FILE)?.unwrap_or(
            desktop_agents::AgentBackupSnapshot {
                version: 1,
                agents: Vec::new(),
            },
        );
    let globals: Option<desktop_agents::AgentRegistryBackup> =
        optional_entry(&mut archive, &manifest, GLOBAL_AGENT_STATE_FILE)?;
    let secret: Option<SecretSnapshot> = optional_entry(&mut archive, &manifest, SECRETS_FILE)?;
    let globals_enabled = request.include_global_config
        && validated.meta.scope.include_global_config
        && archived_state.is_some();
    let secret = secret.filter(|_| request.include_secrets && validated.meta.scope.include_secrets);
    let selected = if request.include_agents && validated.meta.scope.include_agents {
        request.agent_ids.iter().cloned().collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };
    if selected
        .iter()
        .any(|id| !agents.agents.iter().any(|agent| &agent.id == id))
    {
        return Err(bad_request("Selected Agent is not present in the backup"));
    }
    let desktop = desktop_workspace(server)?;
    let full_registry = (globals_enabled && request.mode == RestoreMode::Full)
        .then_some(globals.as_ref())
        .flatten();
    // Destination and logical Core validation precede cancellation of live work.
    desktop_agents::restore::plan_restore(
        desktop,
        &agents,
        full_registry,
        &selected,
        request.default_workspace_dir.as_deref(),
    )
    .map_err(|error| bad_request(&error))?;
    if let Some(snapshot) = &archived_state {
        server
            .inner
            .core
            .prepare_restore(snapshot)
            .map_err(|_| bad_request("Backup Core state is invalid"))?;
    }
    for cancellation in server
        .inner
        .desktop_skill_cancellations
        .read()
        .await
        .values()
    {
        cancellation.cancel();
    }
    *stopped = true;
    desktop_local_models::shutdown(server).await;
    crate::desktop_checkpoints::runtime::cancel_all_and_drain(server).await;
    let lease = server
        .inner
        .core
        .begin_restore(Duration::from_secs(30))
        .await
        .map_err(|_| conflict("Active operations could not be stopped for restore"))?;
    server.ensure_agent_publication_available()?;
    if server.inner.shutdown.is_cancelled() {
        return Err(conflict("Application is shutting down"));
    }
    let current = server
        .inner
        .core
        .backup_snapshot(MAX_FILE_BYTES)
        .map_err(|_| internal("Local Core snapshot failed"))?;
    let archived_state = archived_state.as_ref().unwrap_or(&current);
    let plan = desktop_agents::restore::plan_restore(
        desktop,
        &agents,
        full_registry,
        &selected,
        request.default_workspace_dir.as_deref(),
    )
    .map_err(|error| bad_request(&error))?;
    let mut merged = restore_state::merge_for_agents(
        &server.inner.core,
        &current,
        archived_state,
        &plan,
        globals_enabled,
        preserve,
    )
    .map_err(bad_request)?;
    let selected_workspace = if globals_enabled {
        restore_preferred_workspace(
            desktop,
            &mut merged.snapshot,
            &plan,
            &agents,
            globals.as_ref(),
            &manifest,
        )
    } else {
        desktop.selected.read().await.clone()
    };
    let candidate = lease
        .prepare_restore(&merged.snapshot)
        .map_err(|_| bad_request("Restored Core state is invalid"))?;
    let known_ids = desktop_agents::backup_agent_snapshot(server)
        .await?
        .agents
        .into_iter()
        .map(|agent| agent.id)
        .collect::<Vec<_>>();
    let known = secrets::known_keys(server, &known_ids).map_err(internal)?;
    let mut secret_plan = secrets::plan_restore(
        secret.as_ref(),
        request.include_secrets,
        merged.preserved_local_keys.iter().any(|key| key == "mcp"),
        &known,
    )
    .map_err(bad_request)?;
    if globals_enabled || secret.is_some() {
        let materialized = desktop_environment::hydrate_restore(
            &candidate,
            &server.inner.core,
            secret.as_ref().map(|secret| &secret.environment),
        )
        .map_err(bad_request)?;
        secret_plan.credentials.extend(
            materialized
                .into_iter()
                .map(|(key, value)| (CredentialKey::Environment(key), Some(value))),
        );
        let overrides = secret_plan
            .credentials
            .iter()
            .filter_map(|(key, value)| {
                if let CredentialKey::McpClient(key) = key {
                    Some((key.clone(), value.clone()))
                } else {
                    None
                }
            })
            .collect();
        let materialized = desktop_mcp::restore::hydrate(
            &candidate,
            &server.inner.core.mcp_client_settings(),
            credentials(server)?.as_ref(),
            &overrides,
        )
        .map_err(bad_request)?;
        secret_plan.credentials.extend(
            materialized
                .into_iter()
                .map(|(key, value)| (CredentialKey::McpClient(key), Some(value))),
        );
    }
    desktop_agent_settings::hydrate_restore(&candidate, &plan.agents).map_err(bad_request)?;
    if !globals_enabled && !selected.contains("default") {
        candidate
            .replace_agent_runtime_config(
                server
                    .inner
                    .core
                    .agent_runtime_config()
                    .map_err(|_| internal("Local Agent runtime is unavailable"))?,
            )
            .map_err(|_| internal("Local Agent runtime could not be retained"))?;
    }
    let mut registry = None;
    if globals_enabled || secret.is_some() {
        if let Some((provider, value)) =
            desktop_models::backup_runtime_credential(server).map_err(bad_request)?
        {
            let key = if provider == "openai-compatible" {
                CredentialKey::ApiKey
            } else {
                CredentialKey::AgentSetting(format!("model-provider-api-key:{provider}"))
            };
            secret_plan.credentials.entry(key).or_insert(value);
        }
        let bytes = if globals_enabled
            && manifest
                .entries
                .contains_key("data/config/models/registry.json")
        {
            let mut bytes = Vec::new();
            archive
                .by_name("data/config/models/registry.json")
                .map_err(|_| bad_request("Model registry is missing"))?
                .take(2 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| bad_request("Model registry could not be read"))?;
            bytes
        } else {
            desktop_models::restore_registry_bytes(server).map_err(bad_request)?
        };
        let overlay = CandidateCredentials {
            live: credentials(server)?.as_ref(),
            replacements: &secret_plan.credentials,
        };
        registry = Some(
            desktop_models::hydrate_restore(&candidate, &overlay, &bytes).map_err(bad_request)?,
        );
    }
    let mut files = RestoreFiles::default();
    if !selected.is_empty() || full_registry.is_some() {
        restore_workspaces::stage_agents(desktop, &plan, &mut archive, &manifest, &mut files)
            .map_err(|error| bad_request(&error))?;
    }
    if !selected.is_empty() {
        restore_workspaces::stage_checkpoints(
            desktop,
            &plan,
            archived_state,
            &mut archive,
            &manifest,
            &mut files,
        )
        .map_err(|error| bad_request(&error))?;
    }
    if globals_enabled {
        restore_workspaces::stage_globals(
            desktop,
            &mut archive,
            &manifest,
            registry.as_deref(),
            &mut files,
        )
        .map_err(|error| bad_request(&error))?;
    }
    if let Some(bytes) = registry.as_deref()
        && !(globals_enabled
            && manifest
                .entries
                .contains_key("data/config/models/registry.json"))
    {
        restore_workspaces::stage_model_registry(desktop, bytes, &mut files)
            .map_err(|error| bad_request(&error))?;
    }
    if request.include_skill_pool && validated.meta.scope.include_skill_pool {
        restore_workspaces::stage_skill_pool(desktop, &mut archive, &manifest, &mut files)
            .map_err(|error| bad_request(&error))?;
    }
    let credential_tx =
        CredentialRestore::prepare(credentials(server)?.clone(), secret_plan.credentials)
            .map_err(internal)?;
    let oauth = secret_plan
        .oauth
        .as_ref()
        .map(|snapshot| lease.prepare_oauth_restore(&candidate, snapshot))
        .transpose()
        .map_err(|_| {
            bad_request("Restored OAuth credentials do not match the target configuration")
        })?;
    // Obtain cache locks before changing any live files or credentials.
    let mut aliases = server.inner.desktop_session_aliases.write().await;
    let mut approvals = server.inner.desktop_pending_approvals.write().await;
    let mut push = server.inner.desktop_push_messages.write().await;
    let mut workspace = desktop.selected.write().await;
    let mut transaction = FailedRestore {
        lease,
        files,
        credentials: credential_tx,
        oauth,
    };
    let applied = async {
        transaction
            .files
            .apply()
            .map_err(|_| internal("Restore file exchange failed"))?;
        transaction.credentials.apply().map_err(internal)?;
        if let Some(oauth) = &mut transaction.oauth {
            oauth
                .apply()
                .map_err(|_| internal("Restore OAuth credential exchange failed"))?;
        }
        transaction
            .lease
            .apply(&candidate, MAX_FILE_BYTES)
            .await
            .map_err(|_| internal("Restore Core commit failed"))?;
        transaction.files.commit();
        Ok::<(), ApiError>(())
    }
    .await;
    if let Err(error) = applied {
        if !transaction.rollback() {
            state.coordinator.lock().await.recovery = Some(transaction);
            return Err(internal(
                "Restore rollback is incomplete; recovery data retained",
            ));
        }
        return Err(error);
    }
    *aliases = super::super::DesktopSessionAliases::default();
    approvals.clear();
    push.clear();
    *workspace = selected_workspace;
    Ok(Json(
        json!({"ok": true, "preserved_local_keys": merged.preserved_local_keys}),
    ))
}

fn restore_preferred_workspace(
    desktop: &DesktopWorkspace,
    snapshot: &mut qwenpaw_storage::StoreBackup,
    plan: &desktop_agents::restore::AgentRestorePlan,
    archived: &desktop_agents::AgentBackupSnapshot,
    global: Option<&desktop_agents::AgentRegistryBackup>,
    manifest: &Manifest,
) -> PathBuf {
    let raw = snapshot
        .settings
        .get("preferred_workspace")
        .cloned()
        .unwrap_or_default();
    let mut target = None;
    for agent in &plan.agents.agents {
        let source = archived
            .agents
            .iter()
            .find(|value| value.id == agent.id)
            .map(|value| value.workspace_dir.as_str())
            .or_else(|| {
                global
                    .and_then(|registry| registry.agents.iter().find(|value| value.id == agent.id))
                    .map(|value| value.workspace_dir.as_str())
            });
        if let Some(source) = source
            && let Some(mapped) = desktop_agents::restore::remap_workspace_path(
                &raw,
                source,
                Path::new(&agent.workspace_dir),
            )
        {
            target = restored_directory(Path::new(&mapped), plan, manifest);
            break;
        }
    }
    let target = target
        .or_else(|| restored_directory(Path::new(&raw), plan, manifest))
        .unwrap_or_else(|| desktop.initial.clone());
    snapshot.settings.insert(
        String::from("preferred_workspace"),
        target.to_string_lossy().into_owned(),
    );
    target
}

/// A currently existing directory can disappear when its selected tree is
/// exchanged. Only archived descendants establish a restored subdirectory.
fn restored_directory(
    path: &Path,
    plan: &desktop_agents::restore::AgentRestorePlan,
    manifest: &Manifest,
) -> Option<PathBuf> {
    let selected = plan
        .workspaces
        .iter()
        .filter(|workspace| path.starts_with(&workspace.destination))
        .max_by_key(|workspace| workspace.destination.components().count());
    if let Some(workspace) = selected {
        let relative = path.strip_prefix(&workspace.destination).ok()?;
        if relative.as_os_str().is_empty() {
            return Some(path.to_path_buf());
        }
        let relative = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let prefix = format!("{}{}/{relative}/", WORKSPACE_PREFIX, workspace.id);
        return manifest
            .entries
            .keys()
            .any(|name| name.starts_with(&prefix))
            .then(|| path.to_path_buf());
    }
    path.canonicalize().ok().filter(|path| path.is_dir())
}
