use crate::media::event::{parse_event_line, HelperStatus, MediaEvent};
use crate::media::resolve_helper_path;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Spawn the Swift helper and read events from its stdout.
/// Sends parsed events on `event_tx`, sends HelperStatus when process exits.
pub async fn run_helper(event_tx: mpsc::Sender<MediaEvent>, status_tx: mpsc::Sender<HelperStatus>) {
    let helper_path = resolve_helper_path();

    let mut child = match Command::new(&helper_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            tracing::error!("failed to spawn helper at {}: {e}", helper_path.display());
            let _ = status_tx.send(HelperStatus::IoError).await;
            return;
        }
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            tracing::error!("helper stdout not available");
            let _ = status_tx.send(HelperStatus::IoError).await;
            return;
        }
    };

    let mut reader = BufReader::new(stdout).lines();

    loop {
        tokio::select! {
            line = reader.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        if let Some(event) = parse_event_line(&line) {
                            if event_tx.send(event).await.is_err() {
                                break; // receiver dropped
                            }
                        }
                    }
                    Ok(None) => break, // EOF
                    Err(e) => {
                        tracing::error!("error reading helper stdout: {e}");
                        let _ = status_tx.send(HelperStatus::IoError).await;
                        return;
                    }
                }
            }
        }
    }

    // Process ended — wait for exit code
    let code = child.wait().await.ok().and_then(|s| s.code());
    tracing::info!("helper exited with code: {code:?}");
    let _ = status_tx.send(HelperStatus::Exited { code }).await;
}
