//! Resolve public market detail URLs before the shared safe installer runs.

use super::*;
use crate::desktop_market;

pub(super) fn cancelled() -> ApiError {
    conflict(json!("Skill import cancelled by user"))
}

fn encode(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC)
        .to_string()
        .replace("%2D", "-")
        .replace("%2E", ".")
        .replace("%5F", "_")
        .replace("%7E", "~")
}

fn endpoint(base: &str, path: &str) -> Result<url::Url, ApiError> {
    desktop_market::endpoint(base, path).map_err(|message| bad_request(&message))
}

pub(super) async fn resolve(
    server: &AppServer,
    request: &HubInstallRequest,
) -> Result<Vec<u8>, ApiError> {
    let url = url::Url::parse(&request.bundle_url).map_err(|_| bad_request("Invalid skill URL"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(bad_request("Skill URLs must not contain credentials"));
    }
    let parts = url
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .map(|part| {
            percent_encoding::percent_decode_str(part)
                .decode_utf8()
                .map(|value| value.trim().to_owned())
                .map_err(|_| bad_request("Invalid skill URL encoding"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sources = &server.inner.desktop_market;
    match url.host_str().unwrap_or_default() {
        "platform.agentscope.io" | "modelscope.cn" | "www.modelscope.cn"
            if parts.first().is_some_and(|part| part == "skills") =>
        {
            let qwenpaw = url.host_str() == Some("platform.agentscope.io");
            let base = if qwenpaw {
                &sources.qwenpaw
            } else {
                &sources.modelscope
            };
            let path = if qwenpaw && parts.len() == 2 && Uuid::parse_str(&parts[1]).is_ok() {
                format!("/api/v1/skills/{}/download", encode(&parts[1]))
            } else if parts.len() >= 3 && !parts[1].is_empty() && !parts[2].is_empty() {
                let hint = if parts.len() >= 6 && parts[3] == "archive" && parts[4] == "zip" {
                    parts[5].strip_suffix(".zip").unwrap_or(&parts[5])
                } else {
                    ""
                };
                let version = if !request.version.trim().is_empty() {
                    request.version.trim()
                } else if !hint.is_empty() {
                    hint
                } else {
                    "master"
                };
                format!(
                    "/skills/{}/{}/archive/zip/{}",
                    encode(&parts[1]).replace("%40", "@"),
                    encode(&parts[2]),
                    encode(version)
                )
            } else {
                return Err(bad_request("Invalid market skill detail URL"));
            };
            download_hub_bytes(
                endpoint(base, &path)?.as_str(),
                HeaderMap::new(),
                MAX_SKILL_PACKAGE_BYTES,
            )
            .await
        }
        "api.aliyun.com" | "www.api.aliyun.com"
            if parts.len() >= 3
                && parts[0].eq_ignore_ascii_case("agentexplorer")
                && parts[1].eq_ignore_ascii_case("skills") =>
        {
            if parts[2].is_empty() {
                return Err(bad_request("Aliyun skill ID is missing"));
            }
            let (url, headers) = desktop_market::aliyun_skill_request(server, &parts[2])?;
            let bytes = download_hub_bytes(url.as_str(), headers, MAX_SKILL_CONTENT_BYTES).await?;
            let payload: Value = serde_json::from_slice(&bytes)
                .map_err(|_| bad_gateway("Aliyun returned invalid JSON"))?;
            let content = payload
                .get("content")
                .and_then(Value::as_str)
                .filter(|content| !content.trim().is_empty())
                .ok_or_else(|| bad_gateway("Aliyun GetSkillContent response is missing content"))?;
            Ok(json!({"name":parts[2],"files":{"SKILL.md":content}})
                .to_string()
                .into_bytes())
        }
        "clawhub.ai" | "www.clawhub.ai" => {
            let slug = parts
                .last()
                .filter(|part| !part.is_empty())
                .ok_or_else(|| bad_request("ClawHub skill slug is missing"))?;
            clawhub(server, slug, &request.version).await
        }
        _ => download_hub_bytes(url.as_str(), HeaderMap::new(), MAX_SKILL_PACKAGE_BYTES).await,
    }
}

async fn json_get(url: &url::Url) -> Result<Value, ApiError> {
    let bytes = download_hub_bytes(url.as_str(), HeaderMap::new(), MAX_SKILL_CONTENT_BYTES).await?;
    serde_json::from_slice(&bytes).map_err(|_| bad_gateway("ClawHub returned invalid JSON"))
}

// Detail, version metadata and bounded file hydration form one sequential import.
#[allow(clippy::too_many_lines)]
async fn clawhub(
    server: &AppServer,
    slug: &str,
    requested_version: &str,
) -> Result<Vec<u8>, ApiError> {
    let environment = server
        .inner
        .core
        .runtime_environment()
        .map_err(|_| internal("Application environment is unavailable"))?;
    let setting = |key: &str, fallback: &str| {
        environment
            .get(key)
            .cloned()
            .or_else(|| std::env::var(key).ok())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| fallback.to_owned())
    };
    let base = setting(
        "QWENPAW_SKILLS_HUB_BASE_URL",
        &server.inner.desktop_market.clawhub,
    );
    let path = |key, fallback: &str, version: &str| {
        setting(key, fallback)
            .replace("{slug}", &encode(slug))
            .replace("{version}", &encode(version))
    };
    let detail_url = endpoint(
        &base,
        &path(
            "QWENPAW_SKILLS_HUB_DETAIL_PATH",
            "/api/v1/skills/{slug}",
            "",
        ),
    )?;
    let detail = json_get(&detail_url).await?;
    if find_bundle_object(&detail).is_some() {
        return Ok(detail.to_string().into_bytes());
    }
    if let Some(content) = ["content", "skill_md", "skillMd"]
        .into_iter()
        .find_map(|key| detail.get(key).and_then(Value::as_str))
        .filter(|content| !content.trim().is_empty())
    {
        return Ok(json!({"name":slug,"files":{"SKILL.md":content}})
            .to_string()
            .into_bytes());
    }
    let skill = detail
        .get("skill")
        .ok_or_else(|| bad_gateway("ClawHub response is missing skill metadata"))?;
    let resolved_slug = skill
        .get("slug")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(slug);
    let version = if requested_version.is_empty() {
        detail
            .pointer("/latestVersion/version")
            .or_else(|| skill.pointer("/tags/latest"))
            .and_then(Value::as_str)
            .unwrap_or_default()
    } else {
        requested_version
    };
    let metadata = if detail
        .pointer("/version/files")
        .is_some_and(Value::is_array)
    {
        detail.clone()
    } else {
        if version.is_empty() {
            return Err(bad_gateway("ClawHub response is missing a skill version"));
        }
        let version_path = setting(
            "QWENPAW_SKILLS_HUB_VERSION_PATH",
            "/api/v1/skills/{slug}/versions/{version}",
        )
        .replace("{slug}", &encode(resolved_slug))
        .replace("{version}", &encode(version));
        json_get(&endpoint(&base, &version_path)?).await?
    };
    let entries = metadata
        .pointer("/version/files")
        .and_then(Value::as_array)
        .ok_or_else(|| bad_gateway("ClawHub response is missing file metadata"))?;
    if entries.len() > MAX_SKILL_FILES {
        return Err(payload_too_large("Hub Skill contains too many files"));
    }
    let version = metadata
        .pointer("/version/version")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(version);
    let file_path = setting("QWENPAW_SKILLS_HUB_FILE_PATH", "/api/v1/skills/{slug}/file")
        .replace("{slug}", &encode(resolved_slug));
    let mut files = BTreeMap::new();
    let mut total = 0_usize;
    for entry in entries {
        let Some(relative) = entry
            .get("path")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
        else {
            continue;
        };
        // Reject both platform path syntaxes before requesting any file.
        if relative.contains(['\\', ':'])
            || relative
                .split('/')
                .any(|part| matches!(part, "" | "." | ".."))
        {
            return Err(bad_request("ClawHub file path is invalid"));
        }
        if files.contains_key(relative) {
            return Err(bad_request("ClawHub contains duplicate file paths"));
        }
        let mut url = endpoint(&base, &file_path)?;
        url.query_pairs_mut().append_pair("path", relative);
        if !version.is_empty() {
            url.query_pairs_mut().append_pair("version", version);
        }
        let content =
            download_hub_bytes(url.as_str(), HeaderMap::new(), MAX_SKILL_CONTENT_BYTES).await?;
        total = total.saturating_add(content.len());
        if total > MAX_SKILL_PACKAGE_BYTES {
            return Err(payload_too_large("Hub Skill package is too large"));
        }
        let content = String::from_utf8(content)
            .map_err(|_| bad_gateway("ClawHub file is not UTF-8 text"))?;
        files.insert(relative, content);
    }
    if files.get("SKILL.md").is_none_or(String::is_empty) {
        return Err(bad_gateway("ClawHub response is missing SKILL.md"));
    }
    Ok(json!({"name":skill.get("displayName").and_then(Value::as_str).unwrap_or(resolved_slug),"files":files}).to_string().into_bytes())
}
