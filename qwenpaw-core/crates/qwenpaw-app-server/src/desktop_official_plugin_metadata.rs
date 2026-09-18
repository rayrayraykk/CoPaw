//! Legacy catalog metadata normalization and PEP 440 version rules.

use std::collections::BTreeMap;
use std::str::FromStr;

use pep440_rs::Version;
use serde_json::{Value, json};

#[path = "desktop_official_plugin_versions.rs"]
mod versions;

#[cfg(test)]
#[path = "desktop_official_plugin_version_tests.rs"]
mod version_tests;

pub(super) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => String::from("None"),
        Value::Bool(true) => String::from("True"),
        Value::Bool(false) => String::from("False"),
        _ => value.to_string(),
    }
}

pub(super) fn field(entry: &Value, key: &str, fallback: &str) -> String {
    let value = &entry[key];
    if truthy(value) {
        text(value)
    } else {
        fallback.to_owned()
    }
}

fn localized(value: &Value) -> String {
    if value.is_object() {
        ["en-US", "en", "zh-CN", "zh"]
            .into_iter()
            .find_map(|key| truthy(&value[key]).then(|| text(&value[key])))
            .unwrap_or_default()
    } else if value.is_null() {
        String::new()
    } else {
        text(value)
    }
}

fn plugin_id(entry: &Value) -> String {
    if truthy(&entry["plugin_id"]) {
        return text(&entry["plugin_id"]);
    }
    let id = field(entry, "id", "");
    let version = field(entry, "version", "");
    if version.is_empty() {
        return id;
    }
    if let Some(id) = id.strip_suffix(&format!("-{version}")) {
        return id.to_owned();
    }
    let marker = format!("-{version}-");
    if let Some((id, tail)) = id.rsplit_once(&marker)
        && !id.is_empty()
        && tail.len() == 8
        && tail.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return id.to_owned();
    }
    id
}

fn upgrade(installed: &str, catalog: &str) -> bool {
    if installed.is_empty() || catalog.is_empty() {
        return false;
    }
    match versions::compare(catalog, installed) {
        Some(ordering) => ordering.is_gt(),
        None => installed != catalog,
    }
}

fn normalized(value: &str) -> String {
    let value = versions::trim(value);
    value.strip_prefix(['v', 'V']).unwrap_or(value).to_owned()
}

pub(super) fn compatible(entry: &Value) -> bool {
    let (minimum, maximum) = if let Some(constraint) = entry["qwenpaw_version"].as_object() {
        (
            constraint.get("min").map(|value| normalized(&text(value))),
            constraint.get("max").map(|value| normalized(&text(value))),
        )
    } else {
        let minimum = normalized(&field(entry, "min_version", ""));
        let maximum = normalized(&field(entry, "max_version", ""));
        if minimum.is_empty() && maximum.is_empty() {
            return true;
        }
        (Some(minimum), Some(maximum))
    };
    let Some(minimum) = minimum else {
        return false;
    };
    if field(entry, "plugin_id", &field(entry, "id", "")).is_empty() {
        return false;
    }
    if !versions::valid(&minimum) {
        return false;
    }
    // The original code validates/derives max even though its upper bound is disabled.
    if let Some(maximum) = maximum.filter(|value| !value.is_empty()) {
        if !versions::valid(&maximum) {
            return false;
        }
    } else {
        let mut parts = minimum.split('.');
        if parts.next().is_none_or(|value| !versions::decimal(value))
            || !versions::decimal(parts.next().unwrap_or("0"))
        {
            return false;
        }
    }
    let Ok(mut current) = Version::from_str(crate::PRODUCT_VERSION) else {
        return false;
    };
    if current.pre().is_some() {
        let release = current.release();
        current = Version::new((0..3).map(|index| release.get(index).copied().unwrap_or(0)));
    }
    versions::compare(&current.to_string(), &minimum).is_some_and(|ordering| !ordering.is_lt())
}

pub(super) fn row(
    entry: &Value,
    file_id: &str,
    base: &str,
    path: &str,
    installed: &BTreeMap<String, String>,
) -> Value {
    let id = plugin_id(entry);
    let version = field(entry, "version", "");
    let installed_version = installed.get(&id);
    let descriptions: BTreeMap<_, _> = entry["description"]
        .as_object()
        .into_iter()
        .flat_map(|descriptions| descriptions.iter())
        .filter(|(_, value)| truthy(value))
        .map(|(key, value)| (key.clone(), text(value)))
        .collect();
    json!({
        "id":field(entry,"id",file_id), "plugin_id":id,
        "name":localized(&entry["name"]), "description":localized(&entry["description"]),
        "description_i18n":descriptions, "version":version,
        "author":field(entry,"author",""), "kind":field(entry,"platform",""),
        "size":field(entry,"size",""), "sha256":field(entry,"sha256",""),
        "install_url":format!("{base}{path}"), "installed":installed_version.is_some(),
        "installed_version":installed_version,
        "upgrade_available":upgrade(installed_version.map_or("",String::as_str),&version)
    })
}
