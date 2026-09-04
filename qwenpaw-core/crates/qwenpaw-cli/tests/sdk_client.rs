use pretty_assertions::assert_eq;
use qwenpaw_app_server_client::ClientIdentity;
use qwenpaw_app_server_client::StdioAppServer;
use qwenpaw_protocol::ThreadStartParams;
use qwenpaw_protocol::ThreadStartResponse;
use tokio::process::Command;

#[tokio::test]
async fn rust_sdk_connects_to_the_real_app_server() {
    let home = tempfile::tempdir().expect("temporary Core home should be created");
    let mut command = Command::new(env!("CARGO_BIN_EXE_qwenpaw-core"));
    command
        .args(["app-server", "--stdio"])
        .env("QWENPAW_HOME", home.path());
    let server = StdioAppServer::spawn_command(&mut command)
        .expect("Rust SDK should start the real App Server");
    let initialized = server
        .client()
        .initialize(
            ClientIdentity::new("qwenpaw_rust_sdk_test", "0.2.0")
                .with_title("QwenPaw Rust SDK Test"),
        )
        .await
        .expect("Rust SDK should initialize");
    assert_eq!(initialized.server_info.name, "qwenpaw-core");

    let response = server
        .client()
        .request::<_, ThreadStartResponse>(
            "thread/start",
            ThreadStartParams {
                model: None,
                workspace_root: None,
            },
        )
        .await
        .expect("Rust SDK should create a thread");
    assert_eq!(response.thread.status, qwenpaw_protocol::ThreadStatus::Idle);
    server
        .shutdown()
        .await
        .expect("Rust SDK should stop the App Server");
}
