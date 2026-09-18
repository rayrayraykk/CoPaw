//! External-to-host, memory-only credential fixture; no OS accounts or files.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{AgentPublicationSecret, AgentPublicationSecretScope, DesktopCredentialStore};

pub(super) const ENDPOINT: &str = "QWENPAW_PUBLICATION_TEST_CREDENTIAL_ENDPOINT";
pub(super) const BEFORE: &str = "publication-fixture-secret-before-1a785309";
pub(super) const AFTER: &str = "publication-fixture-secret-after-3145a731";
const LIVE_KEY: &str = "agent.default.mail-auth-code";

#[derive(Serialize, Deserialize)]
enum Request {
    LoadLive(String),
    SaveLive(String, Option<String>),
    LoadRecovery(AgentPublicationSecretScope),
    Prepare(AgentPublicationSecretScope, AgentPublicationSecret),
    Finish(AgentPublicationSecretScope, AgentPublicationSecret),
}

#[derive(Clone, Default)]
pub(super) struct State {
    pub(super) live: Option<String>,
    pub(super) writes: Vec<Option<String>>,
    pub(super) recovery: Option<(AgentPublicationSecretScope, AgentPublicationSecret)>,
}

impl State {
    fn request(&mut self, request: Request) -> anyhow::Result<Value> {
        match request {
            Request::LoadLive(key) => Ok(serde_json::to_value(
                (key == LIVE_KEY).then_some(self.live.as_ref()).flatten(),
            )?),
            Request::SaveLive(key, value) => {
                anyhow::ensure!(key == LIVE_KEY, "unexpected fixture credential key");
                self.live.clone_from(&value);
                self.writes.push(value);
                Ok(Value::Null)
            }
            Request::LoadRecovery(scope) => {
                if let Some((existing, value)) = &self.recovery {
                    if existing != &scope {
                        eprintln!(
                            "fixture recovery identity mismatch: stored={existing:?}, requested={scope:?}"
                        );
                    }
                    anyhow::ensure!(existing == &scope, "fixture scope mismatch");
                    Ok(serde_json::to_value(value)?)
                } else {
                    Ok(Value::Null)
                }
            }
            Request::Prepare(scope, secret) => {
                if let Some((existing, _)) = &self.recovery {
                    eprintln!(
                        "fixture recovery occupied: stored={existing:?}, requested={scope:?}"
                    );
                }
                anyhow::ensure!(self.recovery.is_none(), "fixture account occupied");
                self.recovery = Some((scope, secret));
                Ok(Value::Null)
            }
            Request::Finish(scope, expected) => {
                if let Some((existing, value)) = &self.recovery {
                    anyhow::ensure!(
                        existing == &scope && value == &expected,
                        "fixture recovery changed"
                    );
                    self.recovery = None;
                }
                Ok(Value::Null)
            }
        }
    }
}

pub(super) struct Service {
    pub(super) state: Arc<Mutex<State>>,
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    accepted: Arc<AtomicUsize>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Service {
    pub(super) fn new(previous: Option<&str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let state = Arc::new(Mutex::new(State {
            live: previous.map(str::to_owned),
            ..State::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_state = state.clone();
        let worker_stop = stop.clone();
        let accepted = Arc::new(AtomicUsize::new(0));
        let worker_accepted = accepted.clone();
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Accepted sockets inherit nonblocking mode on macOS.
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        worker_accepted.fetch_add(1, Ordering::Release);
                        let request = read_line(&stream)
                            .and_then(|bytes| Ok(serde_json::from_slice(&bytes)?));
                        let response = match request {
                            Ok(request) => worker_state
                                .lock()
                                .unwrap()
                                .request(request)
                                .map_err(|_| String::from("fixture-state-rejected")),
                            Err(error) => {
                                let io = error
                                    .downcast_ref::<std::io::Error>()
                                    .map(std::io::Error::kind);
                                let json = error
                                    .downcast_ref::<serde_json::Error>()
                                    .map(serde_json::Error::classify);
                                eprintln!("fixture frame rejected: io={io:?}, json={json:?}");
                                Err(String::from("fixture-frame-rejected"))
                            }
                        };
                        let mut bytes = serde_json::to_vec(&response).unwrap();
                        bytes.push(b'\n');
                        // A killed client may disappear before the reply.
                        let _ = stream.write_all(&bytes);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("fixture credential listener failed: {error}"),
                }
            }
        });
        Self {
            state,
            address,
            stop,
            accepted,
            worker: Some(worker),
        }
    }

    pub(super) fn client(&self) -> Arc<Client> {
        Arc::new(Client(self.address))
    }

    pub(super) fn endpoint(&self) -> String {
        self.address.to_string()
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.worker.take().unwrap().join();
    }
}

fn read_line(stream: &TcpStream) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    BufReader::new(stream.take(65_537)).read_until(b'\n', &mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= 65_536 && bytes.last() == Some(&b'\n'),
        "invalid fixture frame"
    );
    Ok(bytes)
}

pub(super) struct Client(SocketAddr);

impl Client {
    pub(super) fn from_env() -> Self {
        let address: SocketAddr = std::env::var(ENDPOINT).unwrap().parse().unwrap();
        assert!(address.ip().is_loopback());
        Self(address)
    }

    fn request(&self, request: &Request) -> anyhow::Result<Value> {
        let operation = match request {
            Request::LoadLive(_) => "load-live",
            Request::SaveLive(_, _) => "save-live",
            Request::LoadRecovery(_) => "load-recovery",
            Request::Prepare(_, _) => "prepare",
            Request::Finish(_, _) => "finish",
        };
        let started = std::time::Instant::now();
        self.request_inner(request).inspect_err(|error| {
            let kind = error
                .downcast_ref::<std::io::Error>()
                .map(std::io::Error::kind);
            let json = error
                .downcast_ref::<serde_json::Error>()
                .map(serde_json::Error::classify);
            eprintln!(
                "fixture credential {operation} failed: io={kind:?}, json={json:?}, elapsed={:?}",
                started.elapsed()
            );
        })
    }

    fn request_inner(&self, request: &Request) -> anyhow::Result<Value> {
        let timeout = Duration::from_secs(2);
        let mut stream = TcpStream::connect_timeout(&self.0, timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let mut bytes = serde_json::to_vec(request)?;
        bytes.push(b'\n');
        stream.write_all(&bytes)?;
        let response: Result<Value, String> = serde_json::from_slice(&read_line(&stream)?)?;
        response.map_err(|code| {
            eprintln!(
                "fixture response rejection class: {}",
                if code == "fixture-state-rejected" {
                    "state"
                } else {
                    "frame"
                }
            );
            anyhow::anyhow!("fixture credential service rejected request")
        })
    }
}

impl DesktopCredentialStore for Client {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        anyhow::bail!("fixture must not use model credentials")
    }
    fn load_agent_setting_secret(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(serde_json::from_value(
            self.request(&Request::LoadLive(key.into()))?,
        )?)
    }
    fn save_agent_setting_secret(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        self.request(&Request::SaveLive(key.into(), value.map(str::to_owned)))?;
        super::super::pause(if value == Some(AFTER) {
            "live-write-before-return"
        } else {
            "inverse-write-before-return"
        });
        Ok(())
    }
    fn load_agent_publication_secret(
        &self,
        scope: &AgentPublicationSecretScope,
    ) -> anyhow::Result<Option<AgentPublicationSecret>> {
        Ok(serde_json::from_value(
            self.request(&Request::LoadRecovery(scope.clone()))?,
        )?)
    }
    fn prepare_agent_publication_secret(
        &self,
        scope: &AgentPublicationSecretScope,
        secret: &AgentPublicationSecret,
    ) -> anyhow::Result<()> {
        self.request(&Request::Prepare(scope.clone(), secret.clone()))?;
        super::super::pause("private-write-before-return");
        Ok(())
    }
    fn finish_agent_publication_secret(
        &self,
        scope: &AgentPublicationSecretScope,
        expected: &AgentPublicationSecret,
    ) -> anyhow::Result<()> {
        self.request(&Request::Finish(scope.clone(), expected.clone()))?;
        super::super::pause("private-delete-before-return");
        Ok(())
    }
}

#[test]
fn publication_credential_service_accepts_a_delayed_request_without_a_readiness_race() {
    let service = Service::new(None);
    let mut stream = TcpStream::connect(service.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while service.accepted.load(Ordering::Acquire) == 0 {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    std::thread::sleep(Duration::from_millis(20));
    let mut request = serde_json::to_vec(&Request::LoadLive(LIVE_KEY.into())).unwrap();
    request.push(b'\n');
    stream.write_all(&request).unwrap();
    let response: Result<Value, String> =
        serde_json::from_slice(&read_line(&stream).unwrap()).unwrap();
    assert_eq!(response, Ok(Value::Null));
}
