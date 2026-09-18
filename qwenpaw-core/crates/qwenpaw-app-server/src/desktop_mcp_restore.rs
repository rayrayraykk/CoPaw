//! MCP candidate hydration and restart-safe credential materialization.

use std::collections::BTreeMap;

use qwenpaw_core::Core;
use qwenpaw_core::McpClientSettings;

use super::DATA_VERSION;
use super::DesktopCredentialStore;
use super::StoredMcpData;
use super::StoredMcpSecrets;
use super::client_secrets;
use super::validate_backup_secrets;
use super::with_secrets;
use super::without_secrets;

/// Only the detached candidate changes. Returned credentials must join the
/// outer credential transaction, including preserved bootstrap values. An
/// empty object is an explicit clear, not absence that can revive old values.
/// The caller has already merged/protected MCP metadata and filtered secret
/// overrides; this helper never accesses the separate OAuth credential store.
pub(crate) fn hydrate(
    candidate: &Core,
    current: &[McpClientSettings],
    credentials: &dyn DesktopCredentialStore,
    overrides: &BTreeMap<String, Option<String>>,
) -> Result<BTreeMap<String, String>, &'static str> {
    let metadata = match candidate
        .read_mcp_data()
        .map_err(|_| "Restored MCP configuration could not be read")?
    {
        Some(data) => serde_json::from_str::<StoredMcpData>(&data)
            .map_err(|_| "Restored MCP configuration is invalid")?,
        None => StoredMcpData {
            version: DATA_VERSION,
            clients: current.iter().cloned().map(without_secrets).collect(),
        },
    };
    if metadata.version != DATA_VERSION
        || metadata
            .clients
            .iter()
            .any(|client| client_secrets(client) != StoredMcpSecrets::default())
    {
        return Err("Restored MCP metadata contains invalid or sensitive fields");
    }
    candidate
        .validate_mcp_client_settings(metadata.clients.clone())
        .map_err(|_| "Restored MCP configuration is invalid")?;
    let serialized =
        serde_json::to_string(&metadata).map_err(|_| "Restored MCP configuration is invalid")?;
    let mut materialized = BTreeMap::new();
    let mut clients = Vec::with_capacity(metadata.clients.len());
    for client in metadata.clients {
        let fields = match overrides.get(&client.key) {
            Some(value) => decode(value.as_deref())?,
            None => match current.iter().find(|local| local.key == client.key) {
                Some(local) => client_secrets(local),
                None => decode(
                    credentials
                        .load_mcp_client_secrets(&client.key)
                        .map_err(|_| "Local MCP credential could not be read")?
                        .as_deref(),
                )?,
            },
        };
        let encoded =
            serde_json::to_string(&fields).map_err(|_| "Restored MCP credential is invalid")?;
        validate_backup_secrets(&encoded)?;
        materialized.insert(client.key.clone(), encoded);
        clients.push(with_secrets(client, fields));
    }
    candidate
        .validate_mcp_client_bindings(clients.clone())
        .map_err(|_| "Restored MCP configuration is invalid")?;
    candidate
        .write_mcp_data(&serialized)
        .map_err(|_| "Restored MCP configuration could not be staged")?;
    candidate
        .replace_mcp_client_settings(clients)
        .map_err(|_| "Restored MCP runtime could not be activated")?;
    Ok(materialized)
}

fn decode(value: Option<&str>) -> Result<StoredMcpSecrets, &'static str> {
    match value {
        Some(value) => {
            validate_backup_secrets(value)?;
            serde_json::from_str(value).map_err(|_| "Restored MCP credential is invalid")
        }
        None => Ok(StoredMcpSecrets::default()),
    }
}
