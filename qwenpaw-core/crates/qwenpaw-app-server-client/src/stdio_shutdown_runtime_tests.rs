use super::*;
use pretty_assertions::assert_eq;

#[test]
fn independent_current_thread_runtimes_drain_owned_children_while_peers_restart() {
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|worker| {
                scope.spawn(move || {
                    for round in 0..16 {
                        let runtime = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .unwrap();
                        runtime.block_on(async {
                            let directory = tempfile::tempdir().unwrap();
                            let mode = if (worker + round) % 2 == 0 { "0" } else { "7" };
                            let server = start(directory.path(), mode).await;
                            let client = server.client().clone();
                            let mut notifications = client.subscribe();
                            let pending = tokio::spawn(async move {
                                client.request::<_, Value>("fixture/wait", json!({})).await
                            });
                            assert_eq!(
                                tokio::time::timeout(Duration::from_secs(2), notifications.recv())
                                    .await.unwrap().unwrap().method,
                                "fixture/received"
                            );
                            let result = tokio::time::timeout(
                                Duration::from_secs(5), server.shutdown()
                            ).await;
                            let phase = std::fs::read_to_string(
                                directory.path().join("finished.json.phase")
                            );
                            assert!(
                                matches!(&result, Ok(Ok(()))) && mode == "0"
                                    || matches!(&result, Ok(Err(ClientError::ProcessExit(status)))
                                        if status.code() == Some(7)) && mode == "7",
                                "worker {worker}, round {round}, mode {mode}: {result:?}; phase: {phase:?}"
                            );
                            assert!(matches!(pending.await.unwrap(), Err(ClientError::TransportClosed)));
                            assert_eq!(phase.unwrap(), "saved");
                            let saved: Value = serde_json::from_slice(
                                &std::fs::read(directory.path().join("finished.json")).unwrap()
                            ).unwrap();
                            assert_eq!(saved, json!({"methods":["initialize","initialized","fixture/wait"]}));
                        });
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
    });
}
