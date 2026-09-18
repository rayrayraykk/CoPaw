//! The production stdio transport with injectable streams for lifecycle tests.

use std::time::Duration;

use anyhow::Context;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use super::{AppServer, ConnectionSession, OUTBOUND_CHANNEL_CAPACITY};

const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(test)]
#[path = "stdio_tests.rs"]
mod tests;

pub(super) async fn run<R, W>(server: AppServer, input: R, mut output: W) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<String>(OUTBOUND_CHANNEL_CAPACITY);
    let mut writer = tokio::spawn(async move {
        while let Some(message) = outbound_rx.recv().await {
            output
                .write_all(message.as_bytes())
                .await
                .context("failed to write app-server message")?;
            output
                .write_all(b"\n")
                .await
                .context("failed to terminate app-server message")?;
            output
                .flush()
                .await
                .context("failed to flush app-server message")?;
        }
        Ok::<(), anyhow::Error>(())
    });
    let mut lines = BufReader::new(input).lines();
    let mut session = ConnectionSession::default();
    let mut written = None;
    let read = loop {
        let next = tokio::select! {
            biased;
            result = &mut writer => {
                written = Some(result);
                break Ok(());
            }
            () = server.inner.shutdown.cancelled() => break Ok(()),
            next = lines.next_line() => next,
        };
        let line = match next {
            Ok(Some(line)) => line,
            Ok(None) => break Ok(()),
            Err(error) => {
                break Err(anyhow::Error::new(error).context("failed to read app-server input"));
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        tokio::select! {
            biased;
            result = &mut writer => {
                written = Some(result);
                break Ok(());
            }
            () = server.inner.shutdown.cancelled() => break Ok(()),
            () = server.process_line(&mut session, &line, &outbound_tx) => {}
        }
    };
    server.shutdown_services().await;
    drop(outbound_tx);
    let written = if let Some(result) = written {
        result.context("app-server writer task failed")?
    } else if let Ok(result) = tokio::time::timeout(OUTPUT_DRAIN_TIMEOUT, &mut writer).await {
        result.context("app-server writer task failed")?
    } else {
        // Tracked services have drained; only an unresponsive output pipe remains.
        writer.abort();
        let _ = writer.await;
        Err(anyhow::anyhow!(
            "app-server output did not drain after shutdown"
        ))
    };
    read?;
    written?;
    server.inner.core.check_final_persistence()?;
    Ok(())
}
