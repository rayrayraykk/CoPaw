//! File recovery must not repeat an already successful credential inverse.

use super::*;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Default)]
struct Secrets {
    writes: Mutex<Vec<Option<String>>>,
    current: Mutex<Option<String>>,
    recovery: crate::desktop_publication_test_support::RecoverySecrets,
}

impl crate::DesktopCredentialStore for Secrets {
    fn load_agent_publication_secret(
        &self,
        scope: &AgentPublicationSecretScope,
    ) -> anyhow::Result<Option<AgentPublicationSecret>> {
        self.recovery.load(scope)
    }
    fn prepare_agent_publication_secret(
        &self,
        scope: &AgentPublicationSecretScope,
        secret: &AgentPublicationSecret,
    ) -> anyhow::Result<()> {
        self.recovery.prepare(scope, secret)
    }
    fn finish_agent_publication_secret(
        &self,
        scope: &AgentPublicationSecretScope,
        expected: &AgentPublicationSecret,
    ) -> anyhow::Result<()> {
        self.recovery.finish(scope, expected)
    }
    fn load_agent_setting_secret(&self, _: &str) -> anyhow::Result<Option<String>> {
        Ok(self.current.lock().unwrap().clone())
    }
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("no model secrets")
    }
    fn save_agent_setting_secret(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        assert_eq!(key, "agent.writer.mail-auth-code");
        *self.current.lock().unwrap() = value.map(str::to_owned);
        self.writes.lock().unwrap().push(value.map(str::to_owned));
        Ok(())
    }
}

#[tokio::test]
async fn publication_recovery_retains_unavailable_files_without_repeating_restored_secret() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let parent = root.join("workspace");
    fs::create_dir(&parent).unwrap();
    let target = parent.join("agent.json");
    fs::write(&target, b"original").unwrap();
    let mut server = AppServer::new(Core::new(qwenpaw_core::ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("fixture"),
    }));
    let secrets = Arc::new(Secrets::default());
    Arc::get_mut(&mut server.inner).unwrap().desktop_credentials = Some(secrets.clone());
    fs::create_dir(root.join("agents")).unwrap();
    let catalog = root.join("agents/catalog.json");
    fs::write(&catalog, b"original catalog").unwrap();
    Arc::get_mut(&mut server.inner).unwrap().desktop_workspace = Some(DesktopWorkspace {
        data_dir: root.clone(),
        initial: parent.clone(),
        selected: tokio::sync::RwLock::new(parent.clone()),
    });
    let core = &server.inner.core;
    let installation = core.installation_id().unwrap();
    let id = Uuid::now_v7();
    let staged = AgentPublicationFiles::prepare(
        installation,
        id,
        "writer",
        &target,
        b"replacement",
        &catalog,
        b"replacement catalog",
    )
    .unwrap();
    let bytes = staged.encode().unwrap();
    let digest = Sha256::digest(&bytes).into();
    core.prepare_agent_publication_recovery(id, digest, &bytes, true)
        .unwrap();
    let scope = AgentPublicationSecretScope::new(installation, id, "writer".into()).unwrap();
    secrets
        .prepare_agent_publication_secret(
            &scope,
            &AgentPublicationSecret::new(
                Some("before-fixture".into()),
                Some("after-fixture".into()),
            ),
        )
        .unwrap();
    *secrets.current.lock().unwrap() = Some("after-fixture".into());
    core.start_agent_publication(id).unwrap();
    AgentPublicationFiles::from_persisted(&bytes, digest, installation, id, &catalog)
        .unwrap()
        .apply()
        .unwrap();
    let displaced = directory.path().join("temporarily-unavailable");
    fs::rename(&parent, &displaced).unwrap();
    for _ in 0..3 {
        let _guard = server.inner.desktop_agents_lock.lock().await;
        assert!(server.recover_agent_publication().is_err());
        assert!(server.ensure_agent_publication_available().is_err());
        assert_eq!(
            *secrets.writes.lock().unwrap(),
            vec![Some(String::from("before-fixture"))]
        );
        assert_eq!(
            fs::read(displaced.join("agent.json")).unwrap(),
            b"replacement"
        );
    }
    fs::rename(&displaced, &parent).unwrap();
    let _guard = server.inner.desktop_agents_lock.lock().await;
    server.recover_agent_publication().unwrap();
    server.ensure_agent_publication_available().unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"original");
    assert_eq!(
        *secrets.writes.lock().unwrap(),
        vec![Some(String::from("before-fixture"))]
    );
    assert_eq!(fs::read_dir(&parent).unwrap().count(), 1);
}
