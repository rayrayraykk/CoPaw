//! Profile submissions cannot bypass the dedicated channel validation gate.

use super::*;
use pretty_assertions::assert_eq;

const PROFILE: &str = "/api/agents/writer";
const SINGLE: &str = "/api/config/channels/console";

#[path = "desktop_channel_publication_tests.rs"]
mod publication;

async fn setup() -> (Fixture, std::path::PathBuf) {
    let fixture = Fixture::new().await;
    let created = fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
    let root = created["workspace_dir"].as_str().unwrap().into();
    (fixture, root)
}

fn snapshot(fixture: &Fixture, root: &std::path::Path) -> (Vec<u8>, Vec<u8>, Option<String>) {
    (
        std::fs::read(root.join("agent.json")).unwrap(),
        std::fs::read(fixture.directory.path().join("data/agents/catalog.json")).unwrap(),
        fixture
            .server
            .inner
            .core
            .read_channel_config_data()
            .unwrap(),
    )
}

#[tokio::test]
async fn channel_profile_invalid_console_uses_dedicated_errors_without_writes() {
    let (fixture, root) = setup().await;
    let before = snapshot(&fixture, &root);
    for console in [
        Value::Null,
        json!([]),
        json!({"bot_prefix":123}),
        json!({"unexpected_field":true}),
        json!({"enabled":"true"}),
        json!({"dm_policy":"unknown"}),
        json!({"allow_from":[123]}),
        json!({"tool_call_max_length":-1}),
        json!({"bot_prefix":"x".repeat(4097)}),
    ] {
        let expected = scoped(&fixture, "writer", "PUT", SINGLE, console.clone()).await;
        assert_eq!(expected.0, StatusCode::BAD_REQUEST, "{console}");
        let actual = scoped(
            &fixture,
            "writer",
            "PUT",
            PROFILE,
            json!({"id":"writer","name":"Do not save","channels":{"console":console}}),
        )
        .await;
        assert_eq!(actual, expected);
        assert_eq!(snapshot(&fixture, &root), before);
    }
}

#[tokio::test]
async fn channel_profile_invalid_container_and_payload_limit_do_not_write() {
    let (fixture, root) = setup().await;
    let before = snapshot(&fixture, &root);
    for (channels, detail) in [
        (json!([]), "Channel configuration must be an object or null"),
        (
            json!(123),
            "Channel configuration must be an object or null",
        ),
        (
            json!("invalid"),
            "Channel configuration must be an object or null",
        ),
        (
            json!({"console":{"bot_prefix":"x".repeat(262_144)}}),
            "Channel configuration is too large",
        ),
    ] {
        assert_eq!(
            scoped(
                &fixture,
                "writer",
                "PUT",
                PROFILE,
                json!({"id":"writer","name":"Do not save","channels":channels})
            )
            .await,
            (StatusCode::BAD_REQUEST, json!({"detail":detail}))
        );
        assert_eq!(snapshot(&fixture, &root), before);
    }
}

#[tokio::test]
async fn channel_profile_browser_number_round_trip_is_not_an_external_change() {
    let (fixture, root) = setup().await;
    let (_, expected) = scoped(&fixture, "writer", "GET", PROFILE, Value::Null).await;
    let mut submitted = expected.clone();
    // JavaScript JSON.stringify emits integral floats without a decimal point.
    submitted["channels"] = json!({"imessage":{"poll_sec":1},"sip":{"call_timeout":120}});
    assert_eq!(
        scoped(&fixture, "writer", "PUT", PROFILE, submitted).await,
        (StatusCode::OK, expected)
    );
    let before = snapshot(&fixture, &root);
    for config in [json!({"call_timeout":120.5}), json!({"call_timeout":"120"})] {
        assert_eq!(
            scoped(
                &fixture,
                "writer",
                "PUT",
                PROFILE,
                json!({"id":"writer","name":"Writer","channels":{"sip":config}})
            )
            .await,
            (
                StatusCode::NOT_IMPLEMENTED,
                json!({"detail":"Rust runtime for channel 'sip' is not implemented"})
            )
        );
        assert_eq!(snapshot(&fixture, &root), before);
    }
}

#[tokio::test]
async fn channel_profile_external_changes_and_unknown_channels_are_rejected() {
    let (mut fixture, root) = setup().await;
    let (_, defaults) = scoped(
        &fixture,
        "writer",
        "GET",
        "/api/config/channels",
        Value::Null,
    )
    .await;
    let before = snapshot(&fixture, &root);
    for channel in defaults
        .as_object()
        .unwrap()
        .keys()
        .filter(|name| *name != "console")
    {
        for config in [
            json!({"enabled":true}),
            json!({"bot_prefix":"changed"}),
            json!({"extra":"fixture"}),
        ] {
            let expected = scoped(
                &fixture,
                "writer",
                "PUT",
                &format!("/api/config/channels/{channel}"),
                config.clone(),
            )
            .await;
            assert_eq!(expected.0, StatusCode::NOT_IMPLEMENTED);
            assert_eq!(
                scoped(
                    &fixture,
                    "writer",
                    "PUT",
                    PROFILE,
                    json!({"id":"writer","name":"Do not save","channels":{channel:config}})
                )
                .await,
                expected
            );
            assert_eq!(snapshot(&fixture, &root), before);
        }
    }
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "PUT",
            PROFILE,
            json!({"id":"writer","name":"Writer","channels":{"custom-fixture":{}}})
        )
        .await,
        (
            StatusCode::NOT_FOUND,
            json!({"detail":"Channel 'custom-fixture' not found"})
        )
    );
    fixture.reopen().await;
    assert_eq!(snapshot(&fixture, &root), before);
}

#[tokio::test]
async fn channel_profile_rejection_precedes_mail_secret_publication() {
    let (fixture, root) = setup().await;
    let before = snapshot(&fixture, &root);
    // The fixture panics on any attempt to save a nonempty Agent secret.
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "PUT",
            PROFILE,
            json!({"id":"writer","name":"Do not save",
                "channels":{"telegram":{"enabled":true,"bot_token":"fixture-only-not-real"}},
                "mail":{"credential":{"auth_code":"fixture-only-not-real"}}})
        )
        .await,
        (
            StatusCode::NOT_IMPLEMENTED,
            json!({"detail":"Rust runtime for channel 'telegram' is not implemented"})
        )
    );
    assert_eq!(snapshot(&fixture, &root), before);
}

#[tokio::test]
async fn channel_profile_validation_preserves_default_and_optional_round_trips() {
    let (fixture, _) = setup().await;
    let (_, mut defaults) = scoped(
        &fixture,
        "writer",
        "GET",
        "/api/config/channels",
        Value::Null,
    )
    .await;
    for value in defaults.as_object_mut().unwrap().values_mut() {
        value.as_object_mut().unwrap().remove("isBuiltin");
    }
    for channels in [
        defaults.clone(),
        json!({"telegram":{"enabled":false}}),
        json!({"console":{"bot_prefix":"valid"}}),
        json!({}),
        Value::Null,
    ] {
        let (_, mut expected) = scoped(&fixture, "writer", "GET", PROFILE, Value::Null).await;
        let mut submitted = expected.clone();
        submitted["channels"] = channels.clone();
        expected["channels"] = if channels.is_null() {
            Value::Null
        } else {
            let mut normalized = defaults.clone();
            if let Some(prefix) = channels.pointer("/console/bot_prefix") {
                normalized["console"]["bot_prefix"] = prefix.clone();
            }
            normalized
        };
        assert_eq!(
            scoped(&fixture, "writer", "PUT", PROFILE, submitted).await,
            (StatusCode::OK, expected.clone())
        );
        assert_eq!(
            scoped(
                &fixture,
                "writer",
                "PUT",
                PROFILE,
                json!({"id":"writer","name":"Writer"})
            )
            .await,
            (StatusCode::OK, expected)
        );
    }
}
