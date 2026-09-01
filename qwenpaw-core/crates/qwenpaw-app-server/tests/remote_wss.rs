use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::SinkExt;
use futures_util::StreamExt;
use qwenpaw_app_server::AppServer;
use qwenpaw_core::Core;
use qwenpaw_core::ModelConfig;
use rcgen::CertifiedKey;
use rustls::ClientConfig;
use rustls::RootCertStore;
use serde_json::Value;
use serde_json::json;
use tokio_tungstenite::Connector;
use tokio_tungstenite::connect_async_tls_with_config;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

const INITIAL_TOKEN: &str = "initial-remote-token-0123456789abcdef";
const ROTATED_TOKEN: &str = "rotated-remote-token-0123456789abcdef";

#[tokio::test]
async fn requires_tls_authentication_and_reloads_the_remote_token() {
    let directory = tempfile::tempdir().expect("temporary WSS directory should be created");
    let certificate_path = directory.path().join("certificate.pem");
    let private_key_path = directory.path().join("private-key.pem");
    let token_path = directory.path().join("auth-token");
    let CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec![String::from("localhost")])
            .expect("test TLS certificate should generate");
    std::fs::write(&certificate_path, cert.pem()).expect("test certificate should write");
    write_private_file(&private_key_path, signing_key.serialize_pem());
    write_private_token(&token_path, INITIAL_TOKEN);

    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("remote WSS listener should bind");
    let address = listener
        .local_addr()
        .expect("remote WSS listener address should resolve");
    let server = AppServer::new(Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    }))
    .with_remote_auth_token_file(&token_path)
    .expect("remote authentication should configure");
    let certificate = certificate_path.clone();
    let private_key = private_key_path.clone();
    let server_task =
        tokio::spawn(async move { server.run_wss(listener, &certificate, &private_key).await });
    let connector = tls_connector(cert.der().clone());
    let url = format!("wss://localhost:{}/app-protocol", address.port());

    wait_for_server(&url, &connector).await;
    let unauthorized = connect(&url, None, &connector)
        .await
        .expect_err("missing bearer token should reject the WSS handshake");
    match unauthorized {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), 401);
            assert_eq!(
                response.headers()["www-authenticate"],
                "Bearer realm=\"qwenpaw-app-protocol\""
            );
        }
        other => panic!("unexpected unauthenticated WSS error: {other}"),
    }

    let (mut socket, _) = connect(&url, Some(INITIAL_TOKEN), &connector)
        .await
        .expect("valid bearer token should establish WSS");
    socket
        .send(Message::Text(
            json!({
                "id": 1,
                "method": "initialize",
                "params": {"clientInfo": {"name": "remote_test", "version": "0.1.0"}}
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("remote initialize should send");
    let initialized = socket
        .next()
        .await
        .expect("remote initialize response should arrive")
        .expect("remote initialize response should be valid");
    let initialized: Value = serde_json::from_str(
        initialized
            .to_text()
            .expect("remote initialize response should be text"),
    )
    .expect("remote initialize response should be JSON");
    assert_eq!(initialized["result"]["protocolVersion"], 3);
    socket.close(None).await.expect("remote WSS should close");

    write_private_token(&token_path, ROTATED_TOKEN);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o644))
            .expect("test token should become intentionally insecure");
        let insecure = connect(&url, Some(ROTATED_TOKEN), &connector)
            .await
            .expect_err("an insecure rotated token file should fail closed");
        assert!(matches!(
            insecure,
            tokio_tungstenite::tungstenite::Error::Http(response)
                if response.status() == 503
        ));
        std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600))
            .expect("test token should become private again");
    }
    assert!(
        connect(&url, Some(INITIAL_TOKEN), &connector)
            .await
            .is_err()
    );
    let (mut rotated, _) = connect(&url, Some(ROTATED_TOKEN), &connector)
        .await
        .expect("rotated bearer token should establish WSS");
    rotated.close(None).await.expect("rotated WSS should close");

    server_task.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_a_tls_private_key_readable_by_other_users() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().expect("temporary WSS directory should be created");
    let certificate_path = directory.path().join("certificate.pem");
    let private_key_path = directory.path().join("private-key.pem");
    let token_path = directory.path().join("auth-token");
    let CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec![String::from("localhost")])
            .expect("test TLS certificate should generate");
    std::fs::write(&certificate_path, cert.pem()).expect("test certificate should write");
    write_private_file(&private_key_path, signing_key.serialize_pem());
    std::fs::set_permissions(&private_key_path, std::fs::Permissions::from_mode(0o644))
        .expect("test private key should become intentionally insecure");
    write_private_token(&token_path, INITIAL_TOKEN);
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("remote WSS listener should bind");
    let server = AppServer::new(Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    }))
    .with_remote_auth_token_file(&token_path)
    .expect("remote authentication should configure");

    let error = server
        .run_wss(listener, &certificate_path, &private_key_path)
        .await
        .expect_err("an insecure TLS private key should fail closed");
    assert!(
        error
            .to_string()
            .contains("must not be accessible by group or other users")
    );
}

fn tls_connector(certificate: rustls::pki_types::CertificateDer<'static>) -> Connector {
    let mut roots = RootCertStore::empty();
    roots
        .add(certificate)
        .expect("test certificate should be a valid root");
    Connector::Rustls(Arc::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ))
}

async fn wait_for_server(url: &str, connector: &Connector) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match connect(url, None, connector).await {
                Err(tokio_tungstenite::tungstenite::Error::Http(response))
                    if response.status() == 401 =>
                {
                    return;
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
                Ok(_) => panic!("remote WSS must never accept an unauthenticated handshake"),
            }
        }
    })
    .await
    .expect("remote WSS should become ready");
}

async fn connect(
    url: &str,
    token: Option<&str>,
    connector: &Connector,
) -> Result<
    (
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::handshake::client::Response,
    ),
    tokio_tungstenite::tungstenite::Error,
> {
    let mut request = url.into_client_request()?;
    if let Some(token) = token {
        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))
                .expect("test bearer header should be valid"),
        );
    }
    connect_async_tls_with_config(request, None, false, Some(connector.clone())).await
}

fn write_private_token(path: &Path, token: &str) {
    write_private_file(path, format!("{token}\n"));
}

fn write_private_file(path: &Path, contents: impl AsRef<[u8]>) {
    std::fs::write(path, contents).expect("private test file should write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("private test file permissions should update");
    }
}
