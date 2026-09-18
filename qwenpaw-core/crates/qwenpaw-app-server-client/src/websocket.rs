//! Explicit connections own only the local transport, never the remote host.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{Error, Message};
use tokio_tungstenite::{Connector, WebSocketStream};
use url::{Host, Url};

use super::{AppServerClient, ClientCommand, ClientError, ServerNotification};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_MESSAGE_BYTES: usize = 1_048_576;

/// Authentication and explicit certificate trust for an existing host.
/// Deliberately does not implement Debug: bearer tokens must not be logged.
#[derive(Default)]
pub struct WebSocketOptions {
    pub bearer_token: Option<String>,
    /// Replaces default public roots; certificate and hostname checks remain on.
    pub tls_roots: Option<rustls::RootCertStore>,
}

/// Owns a connection to an already running Core, not a server process.
pub struct WebSocketConnection {
    client: AppServerClient,
}

impl WebSocketConnection {
    /// Connects without initializing or changing the host configuration.
    /// WSS verifies certificates; WS accepts only literal loopback IP addresses.
    ///
    /// # Errors
    /// Returns a redacted endpoint, TLS, authentication or handshake error.
    pub async fn connect(endpoint: &str, options: WebSocketOptions) -> Result<Self, ClientError> {
        let request = request(endpoint, options.bearer_token.as_deref())?;
        let connector = options.tls_roots.map(|roots| {
            Connector::Rustls(Arc::new(
                rustls::ClientConfig::builder()
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            ))
        });
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_MESSAGE_BYTES))
            .max_write_buffer_size(MAX_MESSAGE_BYTES * 2);
        let (socket, _) = tokio::time::timeout(
            CONNECT_TIMEOUT,
            tokio_tungstenite::connect_async_tls_with_config(
                request,
                Some(config),
                false,
                connector,
            ),
        )
        .await
        .map_err(|_| ClientError::WebSocket("handshake timed out"))?
        .map_err(transport_error)?;
        Ok(Self {
            client: AppServerClient::start_worker(move |commands, notifications| {
                run(socket, commands, notifications)
            }),
        })
    }

    /// Returns the typed client; initialize it before sending other requests.
    #[must_use]
    pub fn client(&self) -> &AppServerClient {
        &self.client
    }

    /// Disconnects only this client, without stopping the host or accepted turns.
    /// This is not a persistence acknowledgement for background work.
    ///
    /// # Errors
    /// Returns a transport error or a bounded-disconnect timeout.
    pub async fn disconnect(self) -> Result<(), ClientError> {
        tokio::time::timeout(DISCONNECT_TIMEOUT, self.client.shutdown_transport(false))
            .await
            .map_err(|_| ClientError::WebSocket("disconnect timed out"))?
    }
}

impl Drop for WebSocketConnection {
    fn drop(&mut self) {
        self.client.abort_worker();
    }
}

fn request(
    endpoint: &str,
    token: Option<&str>,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, ClientError> {
    let invalid = || {
        ClientError::WebSocket(
            "invalid endpoint; use WSS or literal loopback WS without URL credentials, query or fragment",
        )
    };
    let url = Url::parse(endpoint).map_err(|_| invalid())?;
    let loopback = match url.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if !matches!(url.scheme(), "ws" | "wss")
        || (url.scheme() == "ws" && !loopback)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    let mut request = url.as_str().into_client_request().map_err(|_| invalid())?;
    if let Some(token) = token {
        if !(32..=4096).contains(&token.len()) || !token.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(ClientError::WebSocket("invalid bearer token"));
        }
        let mut value = format!("Bearer {token}")
            .parse::<tokio_tungstenite::tungstenite::http::HeaderValue>()
            .map_err(|_| ClientError::WebSocket("invalid bearer token"))?;
        value.set_sensitive(true);
        request.headers_mut().insert(AUTHORIZATION, value);
    }
    Ok(request)
}

fn transport_error(error: Error) -> ClientError {
    // Handshake responses and server close reasons can reflect a credential.
    // Do not embed library errors, response headers, URLs or peer text.
    match error {
        Error::Http(response) => ClientError::WebSocketHandshake(response.status().as_u16()),
        Error::Tls(_) => ClientError::WebSocket("TLS verification or negotiation failed"),
        Error::Io(_) => ClientError::WebSocket("I/O failed"),
        Error::Capacity(_) => ClientError::WebSocket("message or write buffer limit exceeded"),
        _ => ClientError::WebSocket("transport or frame protocol failed"),
    }
}

async fn run<S: AsyncRead + AsyncWrite + Unpin>(
    mut socket: WebSocketStream<S>,
    mut commands: mpsc::Receiver<ClientCommand>,
    notifications: broadcast::Sender<ServerNotification>,
) -> Result<(), ClientError> {
    let mut pending = HashMap::<u64, oneshot::Sender<Result<Value, ClientError>>>::new();
    let mut next_id = 1_u64;
    let result = loop {
        tokio::select! {
            command = commands.recv() => {
                pending.retain(|_, response| !response.is_closed());
                let message = match command {
                    Some(ClientCommand::Request { method, params, response }) => {
                        let id = next_id;
                        next_id = next_id.saturating_add(1);
                        pending.insert(id, response);
                        json!({"id":id,"method":method,"params":params})
                    }
                    Some(ClientCommand::Notify { method, params }) => json!({"method":method,"params":params}),
                    Some(ClientCommand::Shutdown { .. }) | None => {
                        break socket.close(None).await.map_err(transport_error);
                    }
                }.to_string();
                if message.len() > MAX_MESSAGE_BYTES {
                    break Err(ClientError::WebSocket("outgoing message exceeds 1 MiB"));
                }
                if let Err(error) = socket.send(Message::Text(message.into())).await {
                    break Err(transport_error(error));
                }
            }
            message = socket.next() => match message {
                Some(Ok(Message::Text(text))) => match serde_json::from_str(&text) {
                    Ok(value) => super::handle_server_message(&value, &mut pending, &notifications),
                    Err(_) => break Err(ClientError::WebSocket("invalid JSON message")),
                },
                Some(Ok(Message::Ping(_))) => {
                    if let Err(error) = socket.flush().await { break Err(transport_error(error)); }
                }
                Some(Ok(Message::Pong(_))) => {}
                Some(Ok(Message::Close(_))) | None => break Ok(()),
                Some(Ok(Message::Binary(_) | Message::Frame(_))) => break Err(ClientError::WebSocket("expected a text message")),
                Some(Err(error)) => break Err(transport_error(error)),
            }
        }
    };
    drop(commands);
    for (_, response) in pending {
        let _ = response.send(Err(ClientError::TransportClosed));
    }
    result
}

#[cfg(test)]
mod tests;
