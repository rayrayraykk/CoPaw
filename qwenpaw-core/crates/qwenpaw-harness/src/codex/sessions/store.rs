use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::codex::Error;

fn permissions(path: &Path) -> Result<Option<std::fs::Permissions>, Error> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(metadata.permissions())),
        Ok(_) => Err(Error::InvalidSessionState),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn open(directory: &Path) -> Result<(PathBuf, BTreeMap<String, String>), Error> {
    if !directory.is_absolute() {
        return Err(Error::Io(std::io::ErrorKind::InvalidInput));
    }
    std::fs::create_dir_all(directory)?;
    let path = directory.join("codex_sessions.json");
    if permissions(&path)?.is_none() {
        return Ok((path, BTreeMap::new()));
    }
    let bytes = std::fs::read(&path)?;
    let Ok(Value::Object(payload)) = serde_json::from_slice(&bytes) else {
        return Ok((path, BTreeMap::new()));
    };
    let mut threads = BTreeMap::new();
    for (key, value) in payload {
        if key.is_empty() || !super::super::control::truthy(&value) {
            continue;
        }
        let id = value.as_str().ok_or(Error::InvalidSessionState)?;
        threads.insert(key, id.to_owned());
    }
    Ok((path, threads))
}

pub(super) fn write(path: &Path, threads: &BTreeMap<String, String>) -> Result<(), Error> {
    let mode = permissions(path)?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().ok_or(Error::InvalidSessionState)?)?;
    let bytes = serde_json::to_vec_pretty(threads).map_err(|_| Error::InvalidSessionState)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    if let Some(mode) = mode {
        temporary.as_file().set_permissions(mode)?;
    }
    temporary
        .persist(path)
        .map_err(|error| Error::from(error.error))?;
    Ok(())
}
