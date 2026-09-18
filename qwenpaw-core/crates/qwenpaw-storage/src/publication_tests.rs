//! Installation-local commit decisions must not travel in logical backups.

use super::*;
use pretty_assertions::assert_eq;
use uuid::Uuid;

#[test]
fn publication_journal_installation_identity_survives_reopen_and_never_moves_with_backup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("core.sqlite");
    let store = ThreadStore::open(&path).unwrap();
    let before = store.backup_snapshot(1024).unwrap();
    let identity = store.installation_id().unwrap();
    assert_eq!(store.installation_id().unwrap(), identity);
    assert_eq!(store.backup_snapshot(1024).unwrap(), before);
    store.replace_from_backup(&before).unwrap();
    drop(store);
    assert_eq!(
        ThreadStore::open(&path).unwrap().installation_id().unwrap(),
        identity
    );
    let other = ThreadStore::in_memory().unwrap();
    other.replace_from_backup(&before).unwrap();
    assert_ne!(other.installation_id().unwrap(), identity);
    other
        .lock()
        .unwrap()
        .execute(
            "UPDATE core_installation SET id = ?1",
            [Uuid::nil().to_string()],
        )
        .unwrap();
    assert!(other.installation_id().is_err());
    let raw: String = other
        .lock()
        .unwrap()
        .query_row("SELECT id FROM core_installation", [], |row| row.get(0))
        .unwrap();
    assert_eq!(raw, Uuid::nil().to_string());
}

#[test]
fn publication_journal_digest_and_reservation_are_atomic_on_write_failure() {
    let store = ThreadStore::in_memory().unwrap();
    let before = store.backup_snapshot(1024).unwrap();
    store.lock().unwrap().execute_batch("CREATE TRIGGER fail_digest BEFORE INSERT ON agent_publication_journal BEGIN SELECT RAISE(ABORT, 'fixture journal write failure'); END;").unwrap();
    assert!(
        store
            .prepare_agent_publication_with_journal(Uuid::now_v7(), [7; 32])
            .is_err()
    );
    assert_eq!(store.read_agent_publication().unwrap(), None);
    assert_eq!(store.backup_snapshot(1024).unwrap(), before);
    let count: i64 = store
        .lock()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM agent_publication_journal",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn publication_journal_digest_reopens_and_is_deleted_only_with_its_exact_decision() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("core.sqlite");
    let id = Uuid::now_v7();
    let store = ThreadStore::open(&path).unwrap();
    let before = store.backup_snapshot(1024).unwrap();
    store
        .prepare_agent_publication_with_journal(id, [9; 32])
        .unwrap();
    assert_eq!(store.backup_snapshot(1024).unwrap(), before);
    assert!(
        store
            .prepare_agent_publication_with_journal(Uuid::now_v7(), [8; 32])
            .is_err()
    );
    assert!(
        store
            .agent_publication_journal_digest(Uuid::now_v7())
            .is_err()
    );
    store
        .commit_agent_publication(id, Some("channels"))
        .unwrap();
    drop(store);
    let store = ThreadStore::open(&path).unwrap();
    assert_eq!(
        store.agent_publication_journal_digest(id).unwrap(),
        Some([9; 32])
    );
    store.lock().unwrap().execute_batch("CREATE TRIGGER fail_cleanup BEFORE DELETE ON agent_publication_journal BEGIN SELECT RAISE(ABORT, 'fixture cleanup failure'); END;").unwrap();
    assert!(
        store
            .finish_agent_publication(id, AgentPublicationState::Committed)
            .is_err()
    );
    assert_eq!(
        store.agent_publication_journal_digest(id).unwrap(),
        Some([9; 32])
    );
    assert_eq!(
        store.read_agent_publication().unwrap(),
        Some(decision(id, AgentPublicationState::Committed))
    );
    store
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_cleanup;")
        .unwrap();
    store
        .finish_agent_publication(id, AgentPublicationState::Committed)
        .unwrap();
    assert_eq!(store.read_agent_publication().unwrap(), None);
    store.prepare_agent_publication(id).unwrap();
    assert_eq!(store.agent_publication_journal_digest(id).unwrap(), None);
}

#[test]
fn publication_journal_orphan_digest_blocks_reservation_and_backup_replacement() {
    let store = ThreadStore::in_memory().unwrap();
    let before = store.backup_snapshot(1024).unwrap();
    store
        .lock()
        .unwrap()
        .execute_batch("PRAGMA foreign_keys = OFF;")
        .unwrap();
    store
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO agent_publication_journal VALUES (1, ?1)",
            [[3_u8; 32].as_slice()],
        )
        .unwrap();
    assert!(store.prepare_agent_publication(Uuid::now_v7()).is_err());
    assert!(
        store
            .prepare_agent_publication_with_journal(Uuid::now_v7(), [9; 32])
            .is_err()
    );
    assert!(store.replace_from_backup(&before).is_err());
    assert!(store.read_agent_publication().is_err());
    assert_eq!(store.backup_snapshot(1024).unwrap(), before);
}

fn decision(id: Uuid, state: AgentPublicationState) -> AgentPublication {
    AgentPublication { id, state }
}

#[test]
fn publication_recovery_metadata_insert_failure_rolls_back_all_control_rows() {
    let store = ThreadStore::in_memory().unwrap();
    let before = store.backup_snapshot(1024).unwrap();
    store.lock().unwrap().execute_batch("CREATE TRIGGER fail_metadata BEFORE INSERT ON agent_publication_recovery BEGIN SELECT RAISE(ABORT, 'fixture metadata failure'); END;").unwrap();
    assert!(
        store
            .prepare_agent_publication_recovery(Uuid::now_v7(), [1; 32], b"receipt", true)
            .is_err()
    );
    assert_eq!(store.read_agent_publication().unwrap(), None);
    assert_eq!(store.backup_snapshot(1024).unwrap(), before);
    for table in [
        "agent_publication",
        "agent_publication_journal",
        "agent_publication_recovery",
    ] {
        let count: i64 = store
            .lock()
            .unwrap()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn publication_recovery_phases_reopen_and_prevent_early_commit_or_cleanup() {
    for committed in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("core.sqlite");
        let store = ThreadStore::open(&path).unwrap();
        let id = Uuid::now_v7();
        let before = store.backup_snapshot(1024).unwrap();
        store
            .prepare_agent_publication_recovery(id, [1; 32], b"receipt", true)
            .unwrap();
        assert_eq!(store.backup_snapshot(1024).unwrap(), before);
        assert!(store.replace_from_backup(&before).is_err());
        assert!(
            store
                .commit_agent_publication(id, Some("must not publish"))
                .is_err()
        );
        assert_eq!(store.read_setting(CHANNEL_CONFIG_DATA_KEY).unwrap(), None);
        assert!(
            store
                .finish_agent_publication(id, AgentPublicationState::Prepared)
                .is_err()
        );
        assert!(store.start_agent_publication(Uuid::now_v7()).is_err());
        assert!(
            store
                .clean_agent_publication(id, AgentPublicationState::Committed)
                .is_err()
        );
        drop(store);
        let store = ThreadStore::open(&path).unwrap();
        assert_eq!(
            store.read_agent_publication_recovery(id).unwrap(),
            Some(AgentPublicationRecovery {
                journal: b"receipt".to_vec(),
                phase: AgentPublicationPhase::Staging,
                has_secret: true
            })
        );
        store.start_agent_publication(id).unwrap();
        assert!(store.start_agent_publication(id).is_err());
        let state = if committed {
            store
                .commit_agent_publication(id, Some("published"))
                .unwrap();
            AgentPublicationState::Committed
        } else {
            AgentPublicationState::Prepared
        };
        assert!(store.finish_agent_publication(id, state).is_err());
        store.clean_agent_publication(id, state).unwrap();
        store.clean_agent_publication(id, state).unwrap();
        assert!(store.start_agent_publication(id).is_err());
        assert!(
            store
                .commit_agent_publication(id, Some("must not publish"))
                .is_err()
        );
        drop(store);
        let store = ThreadStore::open(&path).unwrap();
        assert_eq!(
            store.read_agent_publication_recovery(id).unwrap(),
            Some(AgentPublicationRecovery {
                journal: b"receipt".to_vec(),
                phase: AgentPublicationPhase::Cleaning,
                has_secret: true
            })
        );
        assert!(
            store
                .finish_agent_publication(Uuid::now_v7(), state)
                .is_err()
        );
        store.finish_agent_publication(id, state).unwrap();
        assert_eq!(store.read_agent_publication().unwrap(), None);
        assert_eq!(store.read_agent_publication_recovery(id).unwrap(), None);
        assert_eq!(
            store.read_setting(CHANNEL_CONFIG_DATA_KEY).unwrap(),
            committed.then(|| "published".into())
        );
    }
}

#[test]
fn publication_recovery_final_delete_is_atomic_when_metadata_cleanup_fails() {
    let store = ThreadStore::in_memory().unwrap();
    let id = Uuid::now_v7();
    store
        .prepare_agent_publication_recovery(id, [2; 32], b"receipt", false)
        .unwrap();
    store
        .clean_agent_publication(id, AgentPublicationState::Prepared)
        .unwrap();
    let before = store.read_agent_publication_recovery(id).unwrap();
    store.lock().unwrap().execute_batch("CREATE TRIGGER fail_metadata_cleanup BEFORE DELETE ON agent_publication_recovery BEGIN SELECT RAISE(ABORT, 'fixture metadata cleanup failure'); END;").unwrap();
    assert!(
        store
            .finish_agent_publication(id, AgentPublicationState::Prepared)
            .is_err()
    );
    assert_eq!(
        store.read_agent_publication().unwrap(),
        Some(decision(id, AgentPublicationState::Prepared))
    );
    assert_eq!(
        store.agent_publication_journal_digest(id).unwrap(),
        Some([2; 32])
    );
    assert_eq!(store.read_agent_publication_recovery(id).unwrap(), before);
    store
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_metadata_cleanup;")
        .unwrap();
    store
        .finish_agent_publication(id, AgentPublicationState::Prepared)
        .unwrap();
    assert_eq!(store.read_agent_publication().unwrap(), None);
}

#[test]
fn publication_recovery_invalid_metadata_is_not_absence() {
    let store = ThreadStore::in_memory().unwrap();
    for bytes in [&[][..], &vec![0; 131_073][..]] {
        assert!(
            store
                .prepare_agent_publication_recovery(Uuid::now_v7(), [0; 32], bytes, false)
                .is_err()
        );
        assert_eq!(store.read_agent_publication().unwrap(), None);
    }
    let id = Uuid::now_v7();
    store
        .prepare_agent_publication_recovery(id, [0; 32], b"receipt", false)
        .unwrap();
    store.lock().unwrap().execute_batch("PRAGMA ignore_check_constraints = ON; UPDATE agent_publication_recovery SET phase = 'unknown';").unwrap();
    assert!(store.read_agent_publication_recovery(id).is_err());
    assert!(
        store
            .finish_agent_publication(id, AgentPublicationState::Prepared)
            .is_err()
    );
    assert_eq!(
        store.read_agent_publication().unwrap(),
        Some(decision(id, AgentPublicationState::Prepared))
    );
}

#[test]
fn publication_decision_survives_reopen_at_each_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("core.sqlite");
    let id = Uuid::now_v7();
    {
        let store = ThreadStore::open(&path).unwrap();
        assert_eq!(store.read_agent_publication().unwrap(), None);
        store.prepare_agent_publication(id).unwrap();
    }
    {
        let store = ThreadStore::open(&path).unwrap();
        assert_eq!(
            store.read_agent_publication().unwrap(),
            Some(decision(id, AgentPublicationState::Prepared))
        );
        assert_eq!(store.read_setting(CHANNEL_CONFIG_DATA_KEY).unwrap(), None);
        store
            .commit_agent_publication(id, Some("new channels"))
            .unwrap();
    }
    {
        let store = ThreadStore::open(&path).unwrap();
        assert_eq!(
            store.read_agent_publication().unwrap(),
            Some(decision(id, AgentPublicationState::Committed))
        );
        assert_eq!(
            store.read_setting(CHANNEL_CONFIG_DATA_KEY).unwrap(),
            Some("new channels".into())
        );
        store
            .finish_agent_publication(id, AgentPublicationState::Committed)
            .unwrap();
    }
    let store = ThreadStore::open(&path).unwrap();
    assert_eq!(store.read_agent_publication().unwrap(), None);
    assert_eq!(
        store.read_setting(CHANNEL_CONFIG_DATA_KEY).unwrap(),
        Some("new channels".into())
    );
}

#[test]
fn publication_decision_without_channels_preserves_missing_and_existing_values() {
    for initial in [None, Some("old channels")] {
        let store = ThreadStore::in_memory().unwrap();
        if let Some(value) = initial {
            store
                .write_settings(&[(CHANNEL_CONFIG_DATA_KEY, value)])
                .unwrap();
        }
        let before = store.backup_snapshot(1024).unwrap();
        let id = Uuid::now_v7();
        store.prepare_agent_publication(id).unwrap();
        store.commit_agent_publication(id, None).unwrap();
        assert_eq!(
            store.read_agent_publication().unwrap(),
            Some(decision(id, AgentPublicationState::Committed))
        );
        assert_eq!(store.backup_snapshot(1024).unwrap(), before);
    }
}

#[test]
fn publication_decision_sql_failure_rolls_back_both_values() {
    for initial in [None, Some("old channels")] {
        for target in ["agent_publication", "core_settings"] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("core.sqlite");
            let store = ThreadStore::open(&path).unwrap();
            if let Some(value) = initial {
                store
                    .write_settings(&[(CHANNEL_CONFIG_DATA_KEY, value)])
                    .unwrap();
            }
            let before = store.backup_snapshot(1024).unwrap();
            let id = Uuid::now_v7();
            store.prepare_agent_publication(id).unwrap();
            let operation = if target == "agent_publication" || initial.is_some() {
                "UPDATE"
            } else {
                "INSERT"
            };
            store
                .lock()
                .unwrap()
                .execute_batch(&format!(
                    "CREATE TRIGGER fail_publication BEFORE {operation} ON {target}
                 BEGIN SELECT RAISE(ABORT, 'injected publication failure'); END;"
                ))
                .unwrap();
            assert!(matches!(
                store.commit_agent_publication(id, Some("new channels")),
                Err(StorageError::Database(_))
            ));
            drop(store);
            let store = ThreadStore::open(&path).unwrap();
            assert_eq!(store.backup_snapshot(1024).unwrap(), before);
            assert_eq!(
                store.read_agent_publication().unwrap(),
                Some(decision(id, AgentPublicationState::Prepared))
            );
        }
    }
}

#[test]
fn publication_decision_rejects_stale_duplicate_and_wrong_state_operations() {
    let store = ThreadStore::in_memory().unwrap();
    let first = Uuid::now_v7();
    let second = Uuid::now_v7();
    store.prepare_agent_publication(first).unwrap();
    for id in [first, second] {
        assert!(matches!(
            store.prepare_agent_publication(id),
            Err(StorageError::AgentPublicationConflict)
        ));
    }
    assert!(matches!(
        store.commit_agent_publication(second, Some("stale")),
        Err(StorageError::AgentPublicationConflict)
    ));
    assert!(matches!(
        store.finish_agent_publication(first, AgentPublicationState::Committed),
        Err(StorageError::AgentPublicationConflict)
    ));
    assert!(matches!(
        store.finish_agent_publication(second, AgentPublicationState::Prepared),
        Err(StorageError::AgentPublicationConflict)
    ));
    assert_eq!(store.read_setting(CHANNEL_CONFIG_DATA_KEY).unwrap(), None);
    assert_eq!(
        store.read_agent_publication().unwrap(),
        Some(decision(first, AgentPublicationState::Prepared))
    );
    store
        .commit_agent_publication(first, Some("committed"))
        .unwrap();
    assert!(matches!(
        store.commit_agent_publication(first, Some("duplicate")),
        Err(StorageError::AgentPublicationConflict)
    ));
    assert!(matches!(
        store.finish_agent_publication(first, AgentPublicationState::Prepared),
        Err(StorageError::AgentPublicationConflict)
    ));
    assert_eq!(
        store.read_setting(CHANNEL_CONFIG_DATA_KEY).unwrap(),
        Some("committed".into())
    );
    store
        .finish_agent_publication(first, AgentPublicationState::Committed)
        .unwrap();
    store.prepare_agent_publication(second).unwrap();
    assert!(matches!(
        store.finish_agent_publication(first, AgentPublicationState::Committed),
        Err(StorageError::AgentPublicationConflict)
    ));
    assert_eq!(
        store.read_agent_publication().unwrap(),
        Some(decision(second, AgentPublicationState::Prepared))
    );
    store
        .finish_agent_publication(second, AgentPublicationState::Prepared)
        .unwrap();
    assert_eq!(store.read_agent_publication().unwrap(), None);
}

#[test]
fn publication_decision_separate_connections_cannot_steal_reservation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("core.sqlite");
    let stores = [
        ThreadStore::open(&path).unwrap(),
        ThreadStore::open(&path).unwrap(),
    ];
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = stores
            .into_iter()
            .map(|store| {
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    let id = Uuid::now_v7();
                    barrier.wait();
                    (id, store.prepare_agent_publication(id))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        results.iter().filter(|(_, result)| result.is_ok()).count(),
        1
    );
    let winner = results.iter().find(|(_, result)| result.is_ok()).unwrap().0;
    let loser = results
        .iter()
        .find(|(_, result)| result.is_err())
        .unwrap()
        .0;
    let store = ThreadStore::open(&path).unwrap();
    assert!(
        store
            .commit_agent_publication(loser, Some("wrong"))
            .is_err()
    );
    assert!(
        store
            .finish_agent_publication(loser, AgentPublicationState::Prepared)
            .is_err()
    );
    assert_eq!(
        store.read_agent_publication().unwrap(),
        Some(decision(winner, AgentPublicationState::Prepared))
    );
    assert_eq!(store.read_setting(CHANNEL_CONFIG_DATA_KEY).unwrap(), None);
}

#[test]
fn publication_decision_stays_local_and_blocks_backup_replacement_until_finished() {
    let store = ThreadStore::in_memory().unwrap();
    store.write_settings(&[("business", "original")]).unwrap();
    let mut replacement = store.backup_snapshot(1024).unwrap();
    replacement
        .settings
        .insert("business".into(), "replacement".into());
    for state in [
        AgentPublicationState::Prepared,
        AgentPublicationState::Committed,
    ] {
        let id = Uuid::now_v7();
        store.prepare_agent_publication(id).unwrap();
        if state == AgentPublicationState::Committed {
            store
                .commit_agent_publication(id, Some("channels"))
                .unwrap();
        }
        let before = store.backup_snapshot(1024).unwrap();
        assert!(matches!(
            store.replace_from_backup(&replacement),
            Err(StorageError::AgentPublicationConflict)
        ));
        assert_eq!(store.backup_snapshot(1024).unwrap(), before);
        assert_eq!(
            store.read_agent_publication().unwrap(),
            Some(decision(id, state))
        );
        let other = ThreadStore::in_memory().unwrap();
        other.replace_from_backup(&before).unwrap();
        assert_eq!(other.backup_snapshot(1024).unwrap(), before);
        assert_eq!(other.read_agent_publication().unwrap(), None);
        store.finish_agent_publication(id, state).unwrap();
    }
    store.replace_from_backup(&replacement).unwrap();
    assert_eq!(store.backup_snapshot(1024).unwrap(), replacement);
}

#[test]
fn publication_decision_rejects_malformed_records_without_removing_them() {
    for (slot, id, state) in [
        (1, "not-a-uuid", "prepared"),
        (1, "AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA", "prepared"),
        (2, "00000000-0000-4000-8000-000000000001", "prepared"),
        (1, "00000000-0000-4000-8000-000000000001", "unknown"),
    ] {
        let store = ThreadStore::in_memory().unwrap();
        let backup = store.backup_snapshot(1024).unwrap();
        store
            .lock()
            .unwrap()
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();
        store
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO agent_publication VALUES (?1, ?2, ?3)",
                params![slot, id, state],
            )
            .unwrap();
        assert!(matches!(
            store.read_agent_publication(),
            Err(StorageError::InvalidAgentPublication)
        ));
        assert!(matches!(
            store.prepare_agent_publication(Uuid::now_v7()),
            Err(StorageError::AgentPublicationConflict)
        ));
        assert!(matches!(
            store.replace_from_backup(&backup),
            Err(StorageError::AgentPublicationConflict)
        ));
        let row: (i64, String, String) = store
            .lock()
            .unwrap()
            .query_row("SELECT slot, id, state FROM agent_publication", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap();
        assert_eq!(row, (slot, id.into(), state.into()));
    }
}

#[test]
fn publication_decision_has_an_installation_local_table_outside_backup() {
    let store = ThreadStore::in_memory().unwrap();
    let before = store.backup_snapshot(1024).unwrap();
    store
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO agent_publication(slot, id, state) VALUES (1, ?1, 'prepared')",
            ["00000000-0000-4000-8000-000000000001"],
        )
        .unwrap();
    assert_eq!(store.backup_snapshot(1024).unwrap(), before);
}
