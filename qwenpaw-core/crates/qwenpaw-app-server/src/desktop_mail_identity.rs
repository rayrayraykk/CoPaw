//! Mail authorization retains the Agent namespace inside each Workspace.

use std::collections::{BTreeMap, BTreeSet};

use qwenpaw_storage::WorkspaceDataKey;
use serde::Deserialize;

use super::{AgentContext, AgentMailAccessControl, MailAccessControlData, MailWorkspace};
use super::{MAX_AGENTS, MAX_DATA_BYTES, MAX_ENTRIES};
use crate::desktop_chats::RestoreBindings;

type Bindings = BTreeMap<String, WorkspaceDataKey>;
const INVALID: &str = "Mail access-control data is invalid";

#[cfg(test)]
#[path = "desktop_mail_identity_tests.rs"]
mod tests;

impl MailAccessControlData {
    pub(super) fn get(&self, context: &AgentContext) -> Option<&AgentMailAccessControl> {
        self.workspaces
            .iter()
            .find(|workspace| workspace.data_key == context.data_key)
            .and_then(|workspace| workspace.agents.get(&context.agent_id))
    }

    pub(super) fn get_mut(
        &mut self,
        context: &AgentContext,
    ) -> Option<&mut AgentMailAccessControl> {
        self.workspaces
            .iter_mut()
            .find(|workspace| workspace.data_key == context.data_key)
            .and_then(|workspace| workspace.agents.get_mut(&context.agent_id))
    }

    pub(super) fn entry(&mut self, context: &AgentContext) -> &mut AgentMailAccessControl {
        let index = self
            .workspaces
            .iter()
            .position(|workspace| workspace.data_key == context.data_key)
            .unwrap_or_else(|| {
                self.workspaces.push(MailWorkspace {
                    data_key: context.data_key.clone(),
                    agents: BTreeMap::new(),
                });
                self.workspaces.len() - 1
            });
        self.workspaces[index]
            .agents
            .entry(context.agent_id.clone())
            .or_default()
    }
}

pub(super) fn decode(
    serialized: Option<&str>,
    default_key: Option<&WorkspaceDataKey>,
) -> Result<MailAccessControlData, &'static str> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Legacy {
        version: u32,
        agents: BTreeMap<String, AgentMailAccessControl>,
    }
    let Some(serialized) = serialized else {
        return Ok(MailAccessControlData::default());
    };
    if serialized.len() > MAX_DATA_BYTES {
        return Err("Mail access-control data exceeds its size limit");
    }
    let value: serde_json::Value = serde_json::from_str(serialized).map_err(|_| INVALID)?;
    let data = if value["version"] == 1 {
        let old: Legacy = serde_json::from_value(value).map_err(|_| INVALID)?;
        debug_assert_eq!(old.version, 1);
        MailAccessControlData {
            version: 2,
            workspaces: old
                .agents
                .into_iter()
                .map(|(id, acl)| {
                    let data_key = if id == "default" {
                        default_key.cloned()
                    } else {
                        None
                    }
                    .unwrap_or_else(|| WorkspaceDataKey::LegacyAgent(id.clone()));
                    MailWorkspace {
                        data_key,
                        agents: BTreeMap::from([(id, acl)]),
                    }
                })
                .collect(),
        }
    } else {
        serde_json::from_value(value).map_err(|_| INVALID)?
    };
    validate(&data)?;
    Ok(data)
}

pub(super) fn encode(data: &MailAccessControlData) -> Result<String, &'static str> {
    validate(data)?;
    let mut data = data.clone();
    data.workspaces
        .sort_by(|left, right| left.data_key.cmp(&right.data_key));
    let serialized = serde_json::to_string(&data).map_err(|_| INVALID)?;
    if serialized.len() > MAX_DATA_BYTES {
        return Err("Mail access-control data exceeds its size limit");
    }
    Ok(serialized)
}

fn validate(data: &MailAccessControlData) -> Result<(), &'static str> {
    if data.version != 2 || data.workspaces.len() > MAX_AGENTS {
        return Err(INVALID);
    }
    let mut keys = BTreeSet::new();
    let mut agents = 0;
    let mut entries = 0;
    for workspace in &data.workspaces {
        if !workspace.data_key.is_valid() || !keys.insert(&workspace.data_key) {
            return Err(INVALID);
        }
        agents += workspace.agents.len();
        for (actor, acl) in &workspace.agents {
            if !qwenpaw_storage::is_valid_agent_id(actor)
                || acl
                    .pending
                    .iter()
                    .chain(&acl.approved_replay)
                    .any(|entry| &entry.agent_id != actor)
            {
                return Err(INVALID);
            }
            entries += acl.whitelist.len()
                + acl.blacklist.len()
                + acl.pending.len()
                + acl.approved_replay.len();
        }
    }
    if agents > MAX_AGENTS || entries > MAX_ENTRIES {
        return Err(INVALID);
    }
    Ok(())
}

pub(crate) fn filter_backup_data(
    serialized: &str,
    selected: &BTreeSet<&str>,
    bindings: &Bindings,
) -> Result<String, &'static str> {
    let mut data = decode(Some(serialized), bindings.get("default"))?;
    let keys = selected
        .iter()
        .filter_map(|id| bindings.get(*id))
        .collect::<BTreeSet<_>>();
    data.workspaces
        .retain(|workspace| keys.contains(&workspace.data_key));
    encode(&data)
}

pub(crate) fn merge_restore_data(
    current: Option<&str>,
    archived: Option<&str>,
    selected: &BTreeSet<&str>,
    bindings: &RestoreBindings<'_>,
) -> Result<String, &'static str> {
    let mut current = decode(current, bindings.current.get("default"))?;
    let incoming = decode(archived, bindings.source.get("default"))?;
    let removed = selected
        .iter()
        .filter_map(|id| bindings.current.get(*id))
        .collect::<BTreeSet<_>>();
    let mapped = selected
        .iter()
        .filter_map(|id| bindings.source.get(*id).zip(bindings.target.get(*id)))
        .collect::<BTreeMap<_, _>>();
    current
        .workspaces
        .retain(|workspace| !removed.contains(&workspace.data_key));
    for mut workspace in incoming.workspaces {
        let Some(target) = mapped.get(&workspace.data_key) else {
            continue;
        };
        if current
            .workspaces
            .iter()
            .any(|existing| &existing.data_key == *target)
        {
            return Err("Restored mail Workspace conflicts with unselected data");
        }
        workspace.data_key = (*target).clone();
        current.workspaces.push(workspace);
    }
    encode(&current)
}
