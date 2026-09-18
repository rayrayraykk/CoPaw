use std::io::Read;
use std::path::{Component, Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, OpenOptionsSyncExt};
use cap_std::fs::{Dir, OpenOptions};
use qwenpaw_protocol::UserInput;
use qwenpaw_storage::{StoredMessage, StoredUserInput, StoredUserPart};
use serde_json::{Value, json};

use crate::{CoreError, FileGuardConfig, ModelProtocol};

pub(crate) const MAX_INLINE_IMAGE_BYTES: u64 = 2 * 1_048_576;
const MAX_TURN_IMAGE_BYTES: u64 = 16 * 1_048_576;
const MAX_IMAGES: usize = 32;

pub(crate) async fn prepare_async(
    input: Vec<UserInput>,
    root: Option<String>,
    guard: FileGuardConfig,
    item_id: String,
) -> Result<(Vec<UserInput>, Option<StoredUserInput>), CoreError> {
    tokio::task::spawn_blocking(move || prepare(input, root.as_deref(), &guard, item_id))
        .await
        .map_err(|_| invalid("image preparation failed"))?
}

pub(crate) fn prepare(
    input: Vec<UserInput>,
    workspace_root: Option<&str>,
    guard: &FileGuardConfig,
    item_id: String,
) -> Result<(Vec<UserInput>, Option<StoredUserInput>), CoreError> {
    if input
        .iter()
        .filter(|part| matches!(part, UserInput::Image { .. }))
        .count()
        > MAX_IMAGES
    {
        return Err(invalid("at most 32 images are allowed per turn"));
    }
    let root = Path::new(workspace_root.ok_or_else(|| invalid("images require a Workspace Root"))?)
        .canonicalize()
        .map_err(|_| invalid("workspace is unavailable"))?;
    let mut parts = Vec::new();
    let mut public = Vec::new();
    let mut total = 0;
    let mut references = Vec::new();
    for part in input {
        match part {
            UserInput::Text { text } => {
                if !text.trim().is_empty() {
                    let text = text.trim().to_owned();
                    parts.push(StoredUserPart::Text { text: text.clone() });
                    public.push(UserInput::Text { text });
                }
            }
            UserInput::Image { path } => {
                let image = snapshot(&root, &path, guard)?;
                if let StoredUserPart::Image {
                    path, size, data, ..
                } = &image
                {
                    if data.is_some() {
                        total += size;
                    }
                    if total > MAX_TURN_IMAGE_BYTES {
                        return Err(invalid("image snapshots exceed the 16 MiB turn limit"));
                    }
                    public.push(UserInput::Image { path: path.clone() });
                }
                parts.push(image);
            }
            reference @ UserInput::FileReference { .. } => {
                references.push(reference.clone());
                public.push(reference);
            }
        }
    }
    if !references.is_empty() {
        parts.push(StoredUserPart::Text {
            text: crate::runtime::compose_user_input(&references, workspace_root)?,
        });
    }
    Ok((public, Some(StoredUserInput { item_id, parts })))
}

fn snapshot(
    root: &Path,
    requested: &str,
    guard: &FileGuardConfig,
) -> Result<StoredUserPart, CoreError> {
    if requested.is_empty()
        || requested.len() > 4096
        || requested.chars().any(char::is_control)
        || requested.contains('\\')
        || requested.contains(':')
    {
        return Err(invalid(
            "use a workspace-relative image path with forward slashes",
        ));
    }
    let relative = Path::new(requested);
    if relative
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid(
            "image path must not be absolute or contain parent components",
        ));
    }
    let candidate = root
        .join(relative)
        .canonicalize()
        .map_err(|_| invalid("image path is unavailable"))?;
    if !candidate.starts_with(root) {
        return Err(invalid("image must stay inside the Workspace Root"));
    }
    if guard.enabled
        && guard.paths.iter().any(|entry| {
            let path = Path::new(entry);
            let protected = if path.is_absolute() {
                path.to_owned()
            } else {
                root.join(path)
            };
            candidate.starts_with(&protected)
                || protected
                    .canonicalize()
                    .is_ok_and(|resolved| candidate.starts_with(resolved))
        })
    {
        return Err(invalid("image path is protected by File Guard"));
    }
    let mut directory = Dir::open_ambient_dir(root, cap_std::ambient_authority())
        .map_err(|_| invalid("workspace is unavailable"))?;
    let mut components = relative.components().peekable();
    let mut normalized = PathBuf::new();
    while let Some(component) = components.next() {
        normalized.push(component);
        if components.peek().is_some() {
            directory = directory
                .open_dir_nofollow(component.as_os_str())
                .map_err(|_| {
                    invalid("image directories must exist and must not be symbolic links")
                })?;
            continue;
        }
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No).nonblock(true);
        let mut file = directory
            .open_with(component.as_os_str(), &options)
            .map_err(|_| invalid("image must be a readable file, not a symbolic link"))?;
        let metadata = file
            .metadata()
            .map_err(|_| invalid("cannot inspect image"))?;
        if !metadata.is_file() {
            return Err(invalid("image must be a regular file"));
        }
        let mut header = [0_u8; 12];
        file.read_exact(&mut header)
            .map_err(|_| invalid("image header is incomplete"))?;
        let mime_type = mime(&header)
            .ok_or_else(|| invalid("supported image formats are PNG, JPEG, GIF and WebP"))?;
        let size = metadata.len();
        let data = if size > MAX_INLINE_IMAGE_BYTES {
            None
        } else {
            let mut bytes = header.to_vec();
            file.by_ref()
                .take(MAX_INLINE_IMAGE_BYTES + 1 - 12)
                .read_to_end(&mut bytes)
                .map_err(|_| invalid("cannot read image"))?;
            let after = file
                .metadata()
                .map_err(|_| invalid("cannot inspect image"))?;
            if bytes.len() as u64 != size
                || after.len() != size
                || after.modified().ok() != metadata.modified().ok()
            {
                return Err(invalid("image changed during reading; retry the upload"));
            }
            Some(STANDARD.encode(bytes))
        };
        return Ok(StoredUserPart::Image {
            path: normalized.to_string_lossy().replace('\\', "/"),
            mime_type: mime_type.to_owned(),
            size,
            data,
        });
    }
    Err(invalid("image path is empty"))
}

fn mime(header: &[u8; 12]) -> Option<&'static str> {
    if header.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if header.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if header.starts_with(b"RIFF") && &header[8..] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn invalid(message: &str) -> CoreError {
    CoreError::Media(message.to_owned())
}

pub(crate) fn wire_parts(message: &StoredMessage, protocol: ModelProtocol) -> Option<Vec<Value>> {
    if message.role != "user" {
        return None;
    }
    Some(message.user_input.as_ref()?.parts.iter().map(|part| match part {
        StoredUserPart::Text { text } => text_part(text, protocol),
        StoredUserPart::Image { mime_type, data: Some(data), .. } => match protocol {
            ModelProtocol::OpenAIChat => json!({"type": "image_url", "image_url": {"url": format!("data:{mime_type};base64,{data}")}}),
            ModelProtocol::OpenAIResponses => json!({"type": "input_image", "image_url": format!("data:{mime_type};base64,{data}")}),
            ModelProtocol::AnthropicMessages => json!({"type": "image", "source": {"type": "base64", "media_type": mime_type, "data": data}}),
            ModelProtocol::GeminiGenerateContent => json!({"inlineData": {"mimeType": mime_type, "data": data}}),
        },
        StoredUserPart::Image { size, data: None, .. } => {
            let kind = if protocol == ModelProtocol::GeminiGenerateContent { "media" } else { "image" };
            text_part(&format!("[{kind} omitted from model context: local file is {size} bytes, exceeds inline limit of {MAX_INLINE_IMAGE_BYTES} bytes]"), protocol)
        }
    }).collect())
}

fn text_part(text: &str, protocol: ModelProtocol) -> Value {
    if protocol == ModelProtocol::GeminiGenerateContent {
        json!({"text": text})
    } else if protocol == ModelProtocol::OpenAIResponses {
        json!({"type": "input_text", "text": text})
    } else {
        json!({"type": "text", "text": text})
    }
}

pub(crate) fn encoded_bytes(message: &StoredMessage) -> usize {
    message.user_input.as_ref().map_or(0, |input| {
        input
            .parts
            .iter()
            .map(|part| match part {
                StoredUserPart::Image {
                    data: Some(data), ..
                } => data.len(),
                _ => 0,
            })
            .sum()
    })
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
