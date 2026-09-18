//! Original `QwenPaw` control-plane mappings for an already connected Codex.

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Map, Value, json};

use super::{CodexClient, Error};

/// Public account fields only; authentication is separate from model availability.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccountStatus {
    pub authenticated: bool,
    pub account: Option<Map<String, Value>>,
}

/// Original Console model-picker entry, without provider-private metadata.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HarnessModel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_default: bool,
    pub reasoning_efforts: Vec<String>,
    pub default_reasoning_effort: Option<String>,
}

/// Original read-only provider skill entry; private file paths are not exposed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HarnessDiscoveredSkill {
    pub name: String,
    pub description: String,
    pub provider_id: &'static str,
    pub source: String,
    pub enabled: bool,
    pub read_only: bool,
    pub scope: &'static str,
}

impl CodexClient {
    /// Discovers provider-owned skills for the requested workspace, without reload.
    ///
    /// # Errors
    /// Returns protocol/timeout failures or invalid paths/data, never partial results.
    pub async fn discover_skills(
        &self,
        cwd: &Path,
        timeout: Duration,
    ) -> Result<Vec<HarnessDiscoveredSkill>, Error> {
        let cwd = cwd
            .to_str()
            .ok_or(Error::Io(std::io::ErrorKind::InvalidInput))?;
        let response = self
            .request(
                "skills/list",
                json!({"cwds":[cwd],"forceReload":false}),
                timeout,
            )
            .await?;
        skill_entries(response)
    }

    /// Reads status without refreshing tokens, exposing only original public fields.
    ///
    /// # Errors
    /// Returns request failures or malformed account data, never a fabricated login.
    pub async fn account_status(&self, timeout: Duration) -> Result<AccountStatus, Error> {
        let result = self
            .request("account/read", json!({"refreshToken": false}), timeout)
            .await?;
        let result = object_or_empty(result)?;
        let account = match result.get("account") {
            None | Some(Value::Null) => None,
            Some(Value::Object(account)) if account.is_empty() => None,
            Some(Value::Object(account)) => Some(account),
            Some(_) => return Err(Error::InvalidFrame),
        };
        Ok(AccountStatus {
            authenticated: account.is_some(),
            account: account.map(|account| {
                ["type", "email", "planType"]
                    .into_iter()
                    .filter_map(|key| {
                        account
                            .get(key)
                            .map(|value| (key.to_owned(), value.clone()))
                    })
                    .collect()
            }),
        })
    }

    /// Starts the original Codex-managed browser or device-code login flow.
    ///
    /// # Errors
    /// Returns request failures or a malformed login response.
    pub async fn start_login(&self, device_code: bool, timeout: Duration) -> Result<Value, Error> {
        let params = if device_code {
            json!({"type":"chatgptDeviceCode"})
        } else {
            json!({"type":"chatgpt", "useHostedLoginSuccessPage":true, "appBrand":"codex"})
        };
        let response = self.request("account/login/start", params, timeout).await?;
        Ok(Value::Object(object_or_empty(response)?))
    }

    /// Signs out through the provider, without reading or deleting credential files.
    ///
    /// # Errors
    /// Returns request failures; no successful result is returned before the reply.
    pub async fn logout(&self, timeout: Duration) -> Result<(), Error> {
        self.request("account/logout", json!({}), timeout).await?;
        Ok(())
    }

    /// Reads every model page within one deadline; never returns a partial catalog.
    ///
    /// # Errors
    /// Returns request failures, invalid pages/cursors or an overall timeout.
    pub async fn models(&self, timeout: Duration) -> Result<Vec<HarnessModel>, Error> {
        tokio::time::timeout(timeout, self.model_pages(timeout))
            .await
            .map_err(|_| Error::Timeout)?
    }

    async fn model_pages(&self, timeout: Duration) -> Result<Vec<HarnessModel>, Error> {
        let mut cursor = Value::Null;
        let mut seen = HashSet::new();
        let mut models = Vec::new();
        loop {
            let result = self
                .request(
                    "model/list",
                    json!({"cursor":cursor,"includeHidden":false}),
                    timeout,
                )
                .await?;
            let mut page = object_or_empty(result)?;
            match page.remove("data") {
                None => {}
                Some(Value::Array(items)) => {
                    for item in items {
                        let model = model_entry(item)?;
                        if !model.id.is_empty() {
                            models.push(model);
                        }
                    }
                }
                Some(_) => return Err(Error::InvalidFrame),
            }
            cursor = page.remove("nextCursor").unwrap_or(Value::Null);
            if !truthy(&cursor) {
                return Ok(models);
            }
            let Value::String(next) = &cursor else {
                return Err(Error::InvalidFrame);
            };
            if !seen.insert(next.clone()) {
                return Err(Error::InvalidFrame);
            }
        }
    }
}

fn object_or_empty(value: Value) -> Result<Map<String, Value>, Error> {
    match value {
        Value::Null => Ok(Map::new()),
        Value::Object(object) => Ok(object),
        _ => Err(Error::InvalidFrame),
    }
}

pub(super) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn text(value: &Value) -> Result<String, Error> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Bool(true) => Ok("True".to_owned()),
        Value::Bool(false) => Ok("False".to_owned()),
        Value::Number(value) if value.is_i64() || value.is_u64() => Ok(value.to_string()),
        _ => Err(Error::InvalidFrame),
    }
}

pub(super) fn first_text(item: &Map<String, Value>, fields: &[&str]) -> Result<String, Error> {
    fields
        .iter()
        .filter_map(|field| item.get(*field))
        .find(|value| truthy(value))
        .map(text)
        .transpose()
        .map(Option::unwrap_or_default)
}

fn model_entry(value: Value) -> Result<HarnessModel, Error> {
    let Value::Object(item) = value else {
        return Err(Error::InvalidFrame);
    };
    let mut efforts = Vec::new();
    if let Some(options) = item.get("supportedReasoningEfforts") {
        let Value::Array(options) = options else {
            return Err(Error::InvalidFrame);
        };
        for option in options {
            let Value::Object(option) = option else {
                return Err(Error::InvalidFrame);
            };
            if let Some(value) = option.get("reasoningEffort").filter(|value| truthy(value)) {
                efforts.push(text(value)?);
            }
        }
    }
    Ok(HarnessModel {
        id: first_text(&item, &["model", "id"])?,
        name: first_text(&item, &["displayName", "model", "id"])?,
        description: first_text(&item, &["description"])?,
        is_default: item.get("isDefault").is_some_and(truthy),
        reasoning_efforts: efforts,
        default_reasoning_effort: item
            .get("defaultReasoningEffort")
            .filter(|value| truthy(value))
            .map(text)
            .transpose()?,
    })
}

fn skill_entries(value: Value) -> Result<Vec<HarnessDiscoveredSkill>, Error> {
    let mut result = object_or_empty(value)?;
    let data = result.remove("data").unwrap_or_else(|| json!([]));
    let Value::Array(entries) = data else {
        return Err(Error::InvalidFrame);
    };
    let mut seen = HashSet::new();
    let mut discovered = Vec::new();
    for entry in entries {
        let Value::Object(mut entry) = entry else {
            continue;
        };
        let skills = entry.remove("skills").unwrap_or(Value::Null);
        if !truthy(&skills) {
            continue;
        }
        let Value::Array(skills) = skills else {
            return Err(Error::InvalidFrame);
        };
        for item in skills {
            let Value::Object(item) = item else {
                continue;
            };
            let name = first_text(&item, &["name"])?;
            let source = first_text(&item, &["scope"])?;
            if name.is_empty() || !seen.insert((name.clone(), source.clone())) {
                continue;
            }
            discovered.push(HarnessDiscoveredSkill {
                name,
                description: first_text(&item, &["description"])?,
                provider_id: "codex",
                source,
                enabled: item.get("enabled").is_none_or(truthy),
                read_only: true,
                scope: "provider",
            });
        }
    }
    Ok(discovered)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod skills_tests;
