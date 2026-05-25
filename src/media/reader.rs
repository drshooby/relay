use crate::media::event::{parse_event_line, HelperCommand, HelperStatus, MediaEvent};
use crate::media::resolve_helper_path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Spawn the Swift helper, read events from its stdout, and forward commands to its stdin.
/// Sends parsed events on `event_tx`, drains `helper_cmd_rx` into stdin as JSON lines,
/// emits HelperStatus when the process exits or fails to spawn.
pub async fn run_helper(
    event_tx: mpsc::Sender<MediaEvent>,
    status_tx: mpsc::Sender<HelperStatus>,
    mut helper_cmd_rx: mpsc::Receiver<HelperCommand>,
) {
    let helper_path = resolve_helper_path();

    let mut child = match Command::new(&helper_path)
        .stdin(std::process::Stdio::piped())
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
    let stdin = match child.stdin.take() {
        Some(s) => s,
        None => {
            tracing::error!("helper stdin not available");
            let _ = status_tx.send(HelperStatus::IoError).await;
            return;
        }
    };

    // Writer task: drains HelperCommands into helper stdin as JSON lines.
    // Aborted when the reader exits so the helper sees EOF on stdin.
    let writer_handle = tokio::spawn(async move {
        let mut stdin = stdin;
        while let Some(cmd) = helper_cmd_rx.recv().await {
            let line = cmd.to_json_line();
            if let Err(e) = stdin.write_all(line.as_bytes()).await {
                tracing::warn!("failed to write to helper stdin: {e}");
                break;
            }
            if let Err(e) = stdin.flush().await {
                tracing::warn!("failed to flush helper stdin: {e}");
                break;
            }
        }
    });

    let mut reader = BufReader::new(stdout).lines();
    loop {
        match reader.next_line().await {
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
                writer_handle.abort();
                let _ = status_tx.send(HelperStatus::IoError).await;
                return;
            }
        }
    }

    writer_handle.abort();

    let code = child.wait().await.ok().and_then(|s| s.code());
    tracing::info!("helper exited with code: {code:?}");
    let _ = status_tx.send(HelperStatus::Exited { code }).await;
}
