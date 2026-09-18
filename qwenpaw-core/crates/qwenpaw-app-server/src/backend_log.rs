//! Installation-local bounded log output and safe tail snapshots.

use std::fs::File;
use std::io::{self, Read as _, Seek as _, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt as _};
use cap_std::ambient_authority;
#[cfg(unix)]
use cap_std::fs::OpenOptionsExt as _;
use cap_std::fs::{Dir, OpenOptions};
use serde_json::{Value, json};

const NAME: &str = "qwenpaw.log";
const MAX_TAIL_BYTES: u64 = 512 * 1024;
const DEFAULT_MAX_BYTES: u64 = 5 * 1024 * 1024;
const DEFAULT_BACKUPS: usize = 3;

#[cfg(test)]
#[path = "backend_log_tests.rs"]
mod tests;

/// Cloneable append sink for one installation's backend diagnostics.
#[derive(Clone)]
pub struct BackendLog(Arc<Mutex<LogFile>>);

struct LogFile {
    directory: Dir,
    file: Option<File>,
    max_bytes: u64,
    backups: usize,
}

impl BackendLog {
    pub(super) fn open(root: &Path) -> io::Result<Self> {
        let max_bytes = std::env::var("QWENPAW_LOG_MAX_SIZE")
            .ok()
            .and_then(|value| parse_size(&value))
            .unwrap_or(DEFAULT_MAX_BYTES);
        let backups = std::env::var("QWENPAW_LOG_MAX_BACKUPS")
            .ok()
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(DEFAULT_BACKUPS);
        Self::with_limits(root, max_bytes, backups)
    }

    fn with_limits(root: &Path, max_bytes: u64, backups: usize) -> io::Result<Self> {
        let directory = Dir::open_ambient_dir(root, ambient_authority())?;
        let file = open_append(&directory)?;
        Ok(Self(Arc::new(Mutex::new(LogFile {
            directory,
            file: Some(file),
            max_bytes,
            backups,
        }))))
    }
}

fn parse_size(raw: &str) -> Option<u64> {
    let value = raw.trim();
    let split = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    let size: u64 = value[..split].parse().ok()?;
    let factor = match value[split..].trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024_u64.pow(2),
        "g" | "gb" | "gib" => 1024_u64.pow(3),
        "t" | "tb" | "tib" => 1024_u64.pow(4),
        _ => return None,
    };
    size.checked_mul(factor).filter(|value| *value > 0)
}

fn regular_or_missing(directory: &Dir, name: &str) -> io::Result<bool> {
    match directory.symlink_metadata(name) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Unsafe backend log file",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn open_append(directory: &Dir) -> io::Result<File> {
    regular_or_missing(directory, NAME)?;
    let mut options = OpenOptions::new();
    options
        .write(true)
        .append(true)
        .create(true)
        .follow(FollowSymlinks::No);
    #[cfg(unix)]
    options.mode(0o600);
    directory
        .open_with(NAME, &options)
        .map(cap_std::fs::File::into_std)
}

impl Write for BackendLog {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut log = self
            .0
            .lock()
            .map_err(|_| io::Error::other("Backend log lock failed"))?;
        if log.file.is_none() {
            log.file = Some(open_append(&log.directory)?);
        }
        let size = log.file.as_ref().unwrap().metadata()?.len();
        if log.backups > 0
            && size.saturating_add(u64::try_from(bytes.len()).unwrap()) >= log.max_bytes
        {
            log.file.take();
            if let Err(error) = log.rotate() {
                // Never recurse into tracing while holding the file lock.
                let _ = writeln!(io::stderr(), "WARNING Backend log rotation failed: {error}");
            }
            log.file = Some(open_append(&log.directory)?);
        }
        log.file.as_mut().unwrap().write_all(bytes)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut log = self
            .0
            .lock()
            .map_err(|_| io::Error::other("Backend log lock failed"))?;
        if let Some(file) = &mut log.file {
            file.flush()?;
        }
        Ok(())
    }
}

impl LogFile {
    fn rotate(&self) -> io::Result<()> {
        // Validate the reserved names before changing any archive.
        for index in 1..=self.backups {
            regular_or_missing(&self.directory, &format!("{NAME}.{index}"))?;
        }
        let oldest = format!("{NAME}.{}", self.backups);
        if regular_or_missing(&self.directory, &oldest)? {
            self.directory.remove_file(&oldest)?;
        }
        for index in (1..self.backups).rev() {
            let from = format!("{NAME}.{index}");
            if regular_or_missing(&self.directory, &from)? {
                self.directory
                    .rename(from, &self.directory, format!("{NAME}.{}", index + 1))?;
            }
        }
        regular_or_missing(&self.directory, NAME)?;
        self.directory
            .rename(NAME, &self.directory, format!("{NAME}.1"))
    }
}

pub(super) fn snapshot(root: &Path, lines: usize) -> io::Result<Value> {
    let path = match root.canonicalize() {
        Ok(root) => root.join(NAME),
        Err(error) if error.kind() == io::ErrorKind::NotFound => root.join(NAME),
        Err(error) => return Err(error),
    };
    let absent = || {
        json!({"path":path,"exists":false,"lines":lines,
        "updated_at":null,"size":0,"content":""})
    };
    let directory = match Dir::open_ambient_dir(root, ambient_authority()) {
        Ok(directory) => directory,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(absent()),
        Err(error) => return Err(error),
    };
    if !regular_or_missing(&directory, NAME)? {
        return Ok(absent());
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut file = match directory.open_with(NAME, &options) {
        Ok(file) => file.into_std(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(absent()),
        Err(error) => return Err(error),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Unsafe backend log file",
        ));
    }
    let updated_at = match metadata.modified()?.duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_secs_f64(),
        Err(error) => -error.duration().as_secs_f64(),
    };
    let size = metadata.len();
    file.seek(SeekFrom::Start(size.saturating_sub(MAX_TAIL_BYTES)))?;
    let mut bytes = Vec::new();
    file.take(size.min(MAX_TAIL_BYTES))
        .read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let separators = [
        '\n', '\r', '\u{000b}', '\u{000c}', '\u{001c}', '\u{001d}', '\u{001e}', '\u{0085}',
        '\u{2028}', '\u{2029}',
    ];
    let mut parts = text.split(separators).collect::<Vec<_>>();
    if text.ends_with(separators) {
        parts.pop();
    }
    let content = parts[parts.len().saturating_sub(lines)..].join("\n");
    Ok(
        json!({"path":path,"exists":true,"lines":lines,"updated_at":updated_at,
        "size":size,"content":content}),
    )
}
