use std::time::Duration;

use pretty_assertions::assert_eq;

use super::*;
use crate::ModelConfig;

#[tokio::test]
async fn publication_journal_core_identity_and_reservation_obey_restore_barrier() {
    let core = Core::new(config());
    let identity = core.installation_id().unwrap();
    let id = Uuid::now_v7();
    let lease = core.begin_restore(Duration::from_secs(1)).await.unwrap();
    assert_eq!(core.installation_id(), Err(CoreError::RestoreBusy));
    assert_eq!(
        core.prepare_agent_publication_with_journal(id, [3; 32]),
        Err(CoreError::RestoreBusy)
    );
    assert_eq!(core.read_agent_publication().unwrap(), None);
    drop(lease);
    assert_eq!(core.installation_id().unwrap(), identity);
    core.prepare_agent_publication_with_journal(id, [3; 32])
        .unwrap();
    assert_eq!(
        core.agent_publication_journal_digest(id).unwrap(),
        Some([3; 32])
    );
}

fn config() -> ModelConfig {
    ModelConfig {
        base_url: "http://127.0.0.1:1/v1".into(),
        api_key: None,
        default_model: "publication-fixture".into(),
    }
}

#[tokio::test]
async fn publication_recovery_phase_mutations_obey_restore_barrier() {
    let core = Core::new(config());
    let id = Uuid::now_v7();
    let lease = core.begin_restore(Duration::from_secs(1)).await.unwrap();
    assert_eq!(
        core.prepare_agent_publication_recovery(id, [1; 32], b"receipt", false),
        Err(CoreError::RestoreBusy)
    );
    drop(lease);
    core.prepare_agent_publication_recovery(id, [1; 32], b"receipt", false)
        .unwrap();
    let before = core.read_agent_publication_recovery(id).unwrap();
    let lease = core.begin_restore(Duration::from_secs(1)).await.unwrap();
    assert_eq!(
        core.start_agent_publication(id),
        Err(CoreError::RestoreBusy)
    );
    assert_eq!(
        core.clean_agent_publication(id, AgentPublicationState::Prepared),
        Err(CoreError::RestoreBusy)
    );
    assert_eq!(core.read_agent_publication_recovery(id).unwrap(), before);
    drop(lease);
    core.start_agent_publication(id).unwrap();
    core.clean_agent_publication(id, AgentPublicationState::Prepared)
        .unwrap();
    core.finish_agent_publication(id, AgentPublicationState::Prepared)
        .unwrap();
}

#[tokio::test]
async fn publication_decision_mutations_obey_restore_barrier() {
    let core = Core::new(config());
    let id = Uuid::now_v7();
    let lease = core.begin_restore(Duration::from_secs(1)).await.unwrap();
    assert_eq!(
        core.prepare_agent_publication(id),
        Err(CoreError::RestoreBusy)
    );
    assert_eq!(core.read_agent_publication().unwrap(), None);
    drop(lease);
    core.prepare_agent_publication(id).unwrap();
    let before = core.backup_snapshot(1024 * 1024).unwrap();
    let lease = core.begin_restore(Duration::from_secs(1)).await.unwrap();
    assert_eq!(
        core.commit_agent_publication(id, Some("channels")),
        Err(CoreError::RestoreBusy)
    );
    assert_eq!(
        core.finish_agent_publication(id, AgentPublicationState::Prepared),
        Err(CoreError::RestoreBusy)
    );
    assert_eq!(
        core.read_agent_publication().unwrap(),
        Some(AgentPublication {
            id,
            state: AgentPublicationState::Prepared
        })
    );
    assert_eq!(core.read_channel_config_data().unwrap(), None);
    drop(lease);
    assert_eq!(core.backup_snapshot(1024 * 1024).unwrap(), before);
    core.commit_agent_publication(id, Some("channels")).unwrap();
    let lease = core.begin_restore(Duration::from_secs(1)).await.unwrap();
    assert_eq!(
        core.finish_agent_publication(id, AgentPublicationState::Committed),
        Err(CoreError::RestoreBusy)
    );
    assert_eq!(
        core.read_agent_publication().unwrap(),
        Some(AgentPublication {
            id,
            state: AgentPublicationState::Committed
        })
    );
    drop(lease);
    core.finish_agent_publication(id, AgentPublicationState::Committed)
        .unwrap();
    assert_eq!(core.read_agent_publication().unwrap(), None);
    assert_eq!(
        core.read_channel_config_data().unwrap(),
        Some("channels".into())
    );
}

#[tokio::test]
async fn publication_decision_core_reopen_uses_canonical_channels_key() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("core.sqlite");
    let id = Uuid::now_v7();
    {
        let core = Core::persistent(config(), &path).unwrap();
        core.write_channel_config_data("original").unwrap();
        core.prepare_agent_publication(id).unwrap();
    }
    {
        let core = Core::persistent(config(), &path).unwrap();
        assert_eq!(
            core.read_agent_publication().unwrap(),
            Some(AgentPublication {
                id,
                state: AgentPublicationState::Prepared
            })
        );
        assert_eq!(
            core.read_channel_config_data().unwrap(),
            Some("original".into())
        );
        core.commit_agent_publication(id, Some("replacement"))
            .unwrap();
    }
    let core = Core::persistent(config(), &path).unwrap();
    assert_eq!(
        core.read_agent_publication().unwrap(),
        Some(AgentPublication {
            id,
            state: AgentPublicationState::Committed
        })
    );
    assert_eq!(
        core.read_channel_config_data().unwrap(),
        Some("replacement".into())
    );
}
