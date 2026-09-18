use super::*;
use pretty_assertions::assert_eq;

const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

fn fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("红 色.png"),
        STANDARD.decode(PNG).unwrap(),
    )
    .unwrap();
    directory
}

fn input() -> Vec<UserInput> {
    vec![
        UserInput::Text {
            text: String::from("first"),
        },
        UserInput::Image {
            path: String::from("红 色.png"),
        },
        UserInput::Text {
            text: String::from("last"),
        },
    ]
}

#[test]
fn snapshots_ordered_images_and_encodes_three_native_protocols() {
    let directory = fixture();
    let (public, stored) = prepare(
        input(),
        directory.path().to_str(),
        &FileGuardConfig::default(),
        String::from("item-1"),
    )
    .unwrap();
    assert_eq!(public, input());
    assert_eq!(
        stored,
        Some(StoredUserInput {
            item_id: String::from("item-1"),
            parts: vec![
                StoredUserPart::Text {
                    text: String::from("first")
                },
                StoredUserPart::Image {
                    path: String::from("红 色.png"),
                    mime_type: String::from("image/png"),
                    size: 69,
                    data: Some(PNG.to_owned())
                },
                StoredUserPart::Text {
                    text: String::from("last")
                },
            ]
        })
    );
    let mut message = StoredMessage::text("user", "first\nlast");
    message.user_input = stored;
    std::fs::write(directory.path().join("红 色.png"), b"changed").unwrap();
    for (protocol, expected) in [
        (
            ModelProtocol::OpenAIChat,
            json!([{"type":"text","text":"first"},{"type":"image_url","image_url":{"url":format!("data:image/png;base64,{PNG}")}},{"type":"text","text":"last"}]),
        ),
        (
            ModelProtocol::AnthropicMessages,
            json!([{"type":"text","text":"first"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":PNG}},{"type":"text","text":"last"}]),
        ),
        (
            ModelProtocol::GeminiGenerateContent,
            json!([{"text":"first"},{"inlineData":{"mimeType":"image/png","data":PNG}},{"text":"last"}]),
        ),
    ] {
        assert_eq!(json!(wire_parts(&message, protocol).unwrap()), expected);
    }
    assert_eq!(encoded_bytes(&message), PNG.len());
    let legacy: StoredMessage =
        serde_json::from_value(json!({"role":"user","content":"old"})).unwrap();
    assert_eq!(
        serde_json::to_value(legacy).unwrap(),
        json!({"role":"user","content":"old"})
    );
}

#[test]
fn caps_large_images_without_reading_or_truncating_their_payload() {
    let directory = fixture();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(directory.path().join("红 色.png"))
        .unwrap();
    file.set_len(MAX_INLINE_IMAGE_BYTES + 1).unwrap();
    let (_, stored) = prepare(
        input(),
        directory.path().to_str(),
        &FileGuardConfig::default(),
        String::from("item"),
    )
    .unwrap();
    let mut message = StoredMessage::text("user", "first\nlast");
    message.user_input = stored;
    assert_eq!(
        message.user_input.as_ref().unwrap().parts[1],
        StoredUserPart::Image {
            path: String::from("红 色.png"),
            mime_type: String::from("image/png"),
            size: MAX_INLINE_IMAGE_BYTES + 1,
            data: None
        }
    );
    for protocol in [
        ModelProtocol::OpenAIChat,
        ModelProtocol::AnthropicMessages,
        ModelProtocol::GeminiGenerateContent,
    ] {
        let parts = wire_parts(&message, protocol).unwrap();
        let kind = if protocol == ModelProtocol::GeminiGenerateContent {
            "media"
        } else {
            "image"
        };
        let expected = format!(
            "[{kind} omitted from model context: local file is 2097153 bytes, exceeds inline limit of 2097152 bytes]"
        );
        assert_eq!(parts[1], text_part(&expected, protocol));
    }
    assert_eq!(encoded_bytes(&message), 0);
}

#[test]
fn rejects_untrusted_paths_non_images_and_resource_overflow() {
    let directory = fixture();
    let root = directory.path().canonicalize().unwrap();
    assert!(snapshot(&root, "红 色.png", &FileGuardConfig::default()).is_ok());
    std::fs::write(directory.path().join("fake.png"), b"this is not an image").unwrap();
    for path in [
        "../outside.png",
        "/tmp/other.png",
        "C:/other.png",
        "sub\\file.png",
        "fake.png",
        "missing.png",
        "",
        ".",
        "red\n.png",
    ] {
        assert!(
            snapshot(&root, path, &FileGuardConfig::default()).is_err(),
            "accepted {path}"
        );
    }
    let mut guard = FileGuardConfig::default();
    guard.paths.push(String::from("红 色.png"));
    assert_eq!(
        snapshot(&root, "红 色.png", &guard).unwrap_err(),
        invalid("image path is protected by File Guard")
    );
    guard.paths = vec![
        directory
            .path()
            .join("红 色.png")
            .to_string_lossy()
            .into_owned(),
    ];
    assert!(snapshot(&root, "红 色.png", &guard).is_err());
    assert!(prepare(input(), None, &guard, String::new()).is_err());
    let many = vec![
        UserInput::Image {
            path: String::from("红 色.png")
        };
        33
    ];
    assert!(prepare(many, directory.path().to_str(), &guard, String::new()).is_err());
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(directory.path().join("红 色.png"))
        .unwrap();
    file.set_len(MAX_INLINE_IMAGE_BYTES).unwrap();
    let many = vec![
        UserInput::Image {
            path: String::from("红 色.png")
        };
        9
    ];
    assert_eq!(
        prepare(
            many,
            directory.path().to_str(),
            &FileGuardConfig::default(),
            String::new()
        )
        .unwrap_err(),
        invalid("image snapshots exceed the 16 MiB turn limit")
    );
}

#[cfg(unix)]
#[test]
fn refuses_file_and_directory_symlinks_even_inside_the_workspace() {
    use std::os::unix::fs::symlink;
    let directory = fixture();
    symlink(
        directory.path().join("红 色.png"),
        directory.path().join("link.png"),
    )
    .unwrap();
    symlink(directory.path(), directory.path().join("link-dir")).unwrap();
    let root = directory.path().canonicalize().unwrap();
    for path in ["link.png", "link-dir/红 色.png"] {
        assert!(snapshot(&root, path, &FileGuardConfig::default()).is_err());
    }
}
