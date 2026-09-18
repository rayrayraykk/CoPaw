//! Provider OAuth transport for the unchanged Console, independent of MCP OAuth.

use std::time::{Duration, Instant};

use axum::response::{Html, IntoResponse, Response};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore as _;
use sha2::{Digest, Sha256};

use super::*;

#[cfg(test)]
#[path = "desktop_provider_oauth_tests.rs"]
mod tests;

const PROVIDER: &str = "openrouter";
const TTL: Duration = Duration::from_secs(600);
const MAX_SESSIONS: usize = 32;
const EXCHANGE_LIMIT: usize = 16 * 1024;

pub(crate) struct StateStore {
    sessions: tokio::sync::Mutex<BTreeMap<String, Session>>,
    authorize_url: String,
    exchange_url: String,
}

impl Default for StateStore {
    fn default() -> Self {
        Self {
            sessions: tokio::sync::Mutex::new(BTreeMap::new()),
            authorize_url: String::from("https://openrouter.ai/auth"),
            exchange_url: String::from("https://openrouter.ai/api/v1/auth/keys"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Pending,
    Exchanging,
    Discovering,
    Completed,
    Failed,
}

struct Session {
    created: Instant,
    origin: String,
    verifier: String,
    expected: CredentialRevision,
    phase: Phase,
    error: Option<&'static str>,
}

impl Session {
    fn expired(&self) -> bool {
        self.created.elapsed() >= TTL
    }
    fn fail(&mut self, message: &'static str) {
        self.phase = Phase::Failed;
        self.error = Some(message);
        self.verifier.clear();
    }
}

#[derive(Deserialize)]
struct StatusQuery {
    state: String,
}

#[derive(Deserialize)]
struct CallbackQuery {
    #[serde(default)]
    state: String,
    code: Option<String>,
    error: Option<String>,
}

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/providers/{provider_id}/oauth/start", post(start))
        .route("/api/providers/{provider_id}/oauth/status", get(status))
        .route("/api/providers/{provider_id}/oauth/callback", get(callback))
}

pub(super) fn secret_hash(secret: Option<&str>) -> Option<String> {
    secret.map(|secret| format!("{:x}", Sha256::digest(secret.as_bytes())))
}

fn random_token() -> String {
    let mut bytes = [0; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn callback_base(server: &AppServer, headers: &HeaderMap) -> Result<url::Url, ApiError> {
    let host = headers
        .get("host")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| bad_request("OAuth callback Host is missing"))?;
    let authority = host
        .parse::<axum::http::uri::Authority>()
        .map_err(|_| bad_request("OAuth callback Host is invalid"))?;
    let base = url::Url::parse(&format!("http://{host}/"))
        .map_err(|_| bad_request("OAuth callback Host is invalid"))?;
    if !base.username().is_empty()
        || base.password().is_some()
        || base.path() != "/"
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(bad_request("OAuth callback Host is invalid"));
    }
    let trusted = server
        .inner
        .allowed_origins
        .iter()
        .filter_map(|origin| url::Url::parse(origin).ok())
        .find(|origin| {
            origin.scheme() == "https"
                && origin.host_str() == base.host_str()
                && origin.port_or_known_default() == Some(authority.port_u16().unwrap_or(443))
                && origin.username().is_empty()
                && origin.password().is_none()
                && origin.path() == "/"
                && origin.query().is_none()
                && origin.fragment().is_none()
        });
    trusted
        .or_else(|| crate::host_is_loopback(host).then_some(base))
        .ok_or_else(|| {
            bad_request("OAuth requires a loopback callback or an explicitly allowed HTTPS origin")
        })
}

async fn start(
    State(server): State<AppServer>,
    Path(provider): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    if provider != PROVIDER {
        return Err(not_found(&format!(
            "Provider '{provider}' does not support OAuth"
        )));
    }
    if !server.origin_allowed(&headers) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"detail":"Origin is not allowed"})),
        ));
    }
    let mut callback = callback_base(&server, &headers)?;
    let origin = callback.origin().ascii_serialization();
    let expected = {
        let _models = server.inner.desktop_models_lock.lock().await;
        let registry = read_registry(&server)?;
        let record = registry
            .providers
            .get(PROVIDER)
            .ok_or_else(|| not_found("Provider not found"))?;
        if record.is_custom {
            return Err(not_found("Provider does not support OAuth"));
        }
        CredentialRevision {
            revision: registry.revision,
            secret_hash: secret_hash(load_provider_secret(&server, PROVIDER).await?.as_deref()),
        }
    };
    let state = random_token();
    let verifier = random_token();
    callback.set_path("/api/providers/openrouter/oauth/callback");
    callback.query_pairs_mut().append_pair("state", &state);
    let mut authorize = url::Url::parse(&server.inner.desktop_provider_oauth.authorize_url)
        .map_err(|_| internal("OAuth authorization endpoint is invalid"))?;
    authorize
        .query_pairs_mut()
        .append_pair("callback_url", callback.as_str())
        .append_pair(
            "code_challenge",
            &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
        )
        .append_pair("code_challenge_method", "S256");
    let mut sessions = server.inner.desktop_provider_oauth.sessions.lock().await;
    sessions.retain(|_, session| !session.expired());
    for session in sessions.values_mut() {
        if matches!(session.phase, Phase::Pending | Phase::Exchanging) {
            session.fail("Superseded by a newer authorization");
        }
    }
    if sessions.len() >= MAX_SESSIONS {
        let oldest = sessions
            .iter()
            .filter(|(_, session)| matches!(session.phase, Phase::Completed | Phase::Failed))
            .min_by_key(|(_, session)| session.created)
            .map(|(state, _)| state.clone());
        if let Some(oldest) = oldest {
            sessions.remove(&oldest);
        } else {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({"detail":"Too many active OAuth sessions"})),
            ));
        }
    }
    sessions.insert(
        state.clone(),
        Session {
            created: Instant::now(),
            origin,
            verifier,
            expected,
            phase: Phase::Pending,
            error: None,
        },
    );
    Ok(Json(
        json!({"authorize_url":authorize.as_str(),"state":state,"flow_type":"browser_redirect"}),
    ))
}

async fn status(
    State(server): State<AppServer>,
    Path(provider): Path<String>,
    Query(query): Query<StatusQuery>,
) -> Json<Value> {
    let mut sessions = server.inner.desktop_provider_oauth.sessions.lock().await;
    sessions.retain(|_, session| !session.expired());
    let Some(session) = sessions.get(&query.state) else {
        return Json(json!({"status":"failed","error":"Session expired"}));
    };
    if provider != PROVIDER {
        return Json(json!({"status":"failed","error":"Provider mismatch"}));
    }
    Json(json!({"status":match session.phase {
        Phase::Completed => "completed", Phase::Failed => "failed", _ => "pending"
    },"error":session.error}))
}

async fn callback(
    State(server): State<AppServer>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    if provider != PROVIDER {
        return page(StatusCode::NOT_FOUND, "Provider does not support OAuth.");
    }
    let origin = match callback_base(&server, &headers) {
        Ok(base) => base.origin().ascii_serialization(),
        Err(_) => return page(StatusCode::BAD_REQUEST, "Invalid callback origin."),
    };
    let (verifier, expected) = {
        let mut sessions = server.inner.desktop_provider_oauth.sessions.lock().await;
        let Some(session) = sessions
            .get_mut(&query.state)
            .filter(|session| !session.expired())
        else {
            return page(StatusCode::BAD_REQUEST, "Session expired or invalid.");
        };
        if session.origin != origin || session.phase != Phase::Pending {
            return page(
                StatusCode::BAD_REQUEST,
                "Authorization callback is invalid or already used.",
            );
        }
        if query.error.is_some() {
            session.fail("Authorization was denied");
            return page(
                StatusCode::BAD_REQUEST,
                "Authorization was denied. Please retry.",
            );
        }
        if query
            .code
            .as_ref()
            .is_none_or(|code| code.is_empty() || code.len() > 8192)
        {
            session.fail("Authorization code is missing or invalid");
            return page(
                StatusCode::BAD_REQUEST,
                "Authorization code is missing or invalid.",
            );
        }
        session.phase = Phase::Exchanging;
        (
            std::mem::take(&mut session.verifier),
            session.expected.clone(),
        )
    };
    let exchanged = tokio::select! {
        () = server.inner.shutdown.cancelled() => Err("Application is shutting down"),
        result = exchange(&server, query.code.as_deref().expect("validated code"), &verifier) => result,
    };
    let key = match exchanged {
        Ok(key) => key,
        Err(message) => {
            fail(&server, &query.state, message).await;
            return page(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Authorization failed. Please retry.",
            );
        }
    };
    {
        // Serialize the final save with supersession; network I/O ran unlocked.
        let mut sessions = server.inner.desktop_provider_oauth.sessions.lock().await;
        let Some(session) = sessions
            .get_mut(&query.state)
            .filter(|session| !session.expired() && session.phase == Phase::Exchanging)
        else {
            return page(StatusCode::BAD_REQUEST, "Session expired or superseded.");
        };
        if let Err(error) =
            configure_provider_checked(&server, PROVIDER, &json!({"api_key":key}), Some(&expected))
                .await
        {
            session.fail(if error.0 == StatusCode::CONFLICT {
                "Provider configuration changed during authorization"
            } else {
                "Provider credentials could not be saved"
            });
            return page(error.0, "Authorization could not be saved. Please retry.");
        }
        session.phase = Phase::Discovering;
    }
    finish_discovery(&server, &query.state).await;
    page(StatusCode::OK, "")
}

async fn finish_discovery(server: &AppServer, state: &str) {
    // Discovery uses the normal revision-checked catalog path. Failure does not
    // undo a successfully saved key or replace the original UI's login success.
    let _ = tokio::time::timeout(
        Duration::from_secs(30),
        discover_models(
            State(server.clone()),
            Path(String::from(PROVIDER)),
            Query(DiscoverModelsQuery { save: true }),
            Bytes::new(),
        ),
    )
    .await;
    if let Some(session) = server
        .inner
        .desktop_provider_oauth
        .sessions
        .lock()
        .await
        .get_mut(state)
        && session.phase == Phase::Discovering
    {
        session.phase = Phase::Completed;
    }
}

async fn fail(server: &AppServer, state: &str, message: &'static str) {
    if let Some(session) = server
        .inner
        .desktop_provider_oauth
        .sessions
        .lock()
        .await
        .get_mut(state)
        && session.phase == Phase::Exchanging
    {
        session.fail(message);
    }
}

async fn exchange(server: &AppServer, code: &str, verifier: &str) -> Result<String, &'static str> {
    let response = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "OAuth HTTP client could not start")?
        .post(&server.inner.desktop_provider_oauth.exchange_url)
        .json(&json!({"code":code,"code_verifier":verifier,"code_challenge_method":"S256"}))
        .send()
        .await
        .map_err(|_| "OAuth exchange request failed")?;
    if !response.status().is_success() {
        return Err("OAuth exchange was rejected");
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "OAuth exchange response could not be read")?;
        if bytes.len().saturating_add(chunk.len()) > EXCHANGE_LIMIT {
            return Err("OAuth exchange response is too large");
        }
        bytes.extend_from_slice(&chunk);
    }
    let body: Value =
        serde_json::from_slice(&bytes).map_err(|_| "OAuth exchange returned invalid JSON")?;
    let key = body
        .get("key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or("OAuth exchange returned no credentials")?;
    validate_api_key(key).map_err(|_| "OAuth exchange returned invalid credentials")?;
    Ok(key.to_owned())
}

fn page(status: StatusCode, error: &'static str) -> Response {
    let nonce = random_token();
    let content = if status == StatusCode::OK {
        format!(
            r#"<!DOCTYPE html><html><head><title>Authorization Successful</title></head><body><h2>Connected!</h2><p>You can close this window.</p><script nonce="{nonce}">history.replaceState(null, '', location.pathname);if(window.opener)window.opener.postMessage({{type:'oauth_complete',provider:'openrouter'}},location.origin);setTimeout(()=>window.close(),1500);</script></body></html>"#
        )
    } else {
        // Messages are static, never provider responses, codes, keys or query text.
        format!(
            "<!DOCTYPE html><html><head><title>Authorization Failed</title></head><body><h2>Authorization Failed</h2><p>{error}</p><p>You can close this window.</p></body></html>"
        )
    };
    let mut response = (status, Html(content)).into_response();
    response
        .headers_mut()
        .insert("referrer-policy", "no-referrer".parse().unwrap());
    response
        .headers_mut()
        .insert("x-content-type-options", "nosniff".parse().unwrap());
    response.headers_mut().insert("content-security-policy", format!("default-src 'none'; script-src 'nonce-{nonce}'; base-uri 'none'; frame-ancestors 'none'").parse().unwrap());
    response
}
