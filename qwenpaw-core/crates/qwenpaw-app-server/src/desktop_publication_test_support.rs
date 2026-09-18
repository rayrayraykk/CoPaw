//! Explicit in-memory recovery support for isolated host test credential adapters.

use crate::{AgentPublicationSecret, AgentPublicationSecretScope};
use std::sync::Mutex;

#[derive(Default)]
pub(crate) struct RecoverySecrets(
    Mutex<Vec<(AgentPublicationSecretScope, AgentPublicationSecret)>>,
);

impl RecoverySecrets {
    pub(crate) fn load(
        &self,
        scope: &AgentPublicationSecretScope,
    ) -> anyhow::Result<Option<AgentPublicationSecret>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("fixture lock poisoned"))?
            .iter()
            .find(|(key, _)| key == scope)
            .map(|(_, value)| value.clone()))
    }
    pub(crate) fn prepare(
        &self,
        scope: &AgentPublicationSecretScope,
        secret: &AgentPublicationSecret,
    ) -> anyhow::Result<()> {
        let mut values = self.0.lock().unwrap();
        anyhow::ensure!(
            !values.iter().any(|(key, _)| key == scope),
            "fixture recovery occupied"
        );
        values.push((scope.clone(), secret.clone()));
        Ok(())
    }
    pub(crate) fn finish(
        &self,
        scope: &AgentPublicationSecretScope,
        expected: &AgentPublicationSecret,
    ) -> anyhow::Result<()> {
        let mut values = self.0.lock().unwrap();
        if let Some(index) = values.iter().position(|(key, _)| key == scope) {
            anyhow::ensure!(&values[index].1 == expected, "fixture recovery changed");
            values.remove(index);
        }
        Ok(())
    }
}
