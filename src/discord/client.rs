use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use tokio::sync::mpsc;

use crate::constants::{
    DISCORD_CLIENT_ID, DISCORD_HEARTBEAT_INTERVAL_MS, DISCORD_POST_CONNECT_DELAY_MS,
};
use crate::discord::activity::{build_activity, TrackInfo};
use crate::discord::reconnect::{initial_backoff_ms, next_backoff_ms};

#[derive(Debug)]
pub enum DiscordCommand {
    SetActivity {
        track: TrackInfo,
        artwork_url: Option<String>,
        started_at: i64,
    },
    ClearActivity,
    Shutdown,
}

/// Run the Discord RPC client task.
/// Reconnects with exponential backoff on failure.
/// The last known activity is re-published after reconnect.
pub async fn run_discord_client(mut cmd_rx: mpsc::Receiver<DiscordCommand>) {
    let mut backoff_ms = initial_backoff_ms();
    let mut last_activity: Option<(TrackInfo, Option<String>, i64)> = None;

    loop {
        match connect_and_run(&mut cmd_rx, &mut last_activity).await {
            Ok(()) => break, // clean shutdown (Shutdown command received)
            Err(e) => {
                tracing::error!("Discord RPC disconnected: {e}");
                tracing::info!("reconnecting in {backoff_ms}ms");
                tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;
                backoff_ms = next_backoff_ms(backoff_ms);
            }
        }
    }
}

async fn connect_and_run(
    cmd_rx: &mut mpsc::Receiver<DiscordCommand>,
    last_activity: &mut Option<(TrackInfo, Option<String>, i64)>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Connect inside spawn_blocking — connect() is a blocking IPC call.
    // DiscordIpcClient::new() is infallible; only connect() can fail.
    let client = tokio::task::spawn_blocking(|| {
        let mut client = DiscordIpcClient::new(DISCORD_CLIENT_ID);
        client.connect()?;
        Ok::<_, discord_rich_presence::error::Error>(client)
    })
    .await
    // JoinError means the blocking task panicked; treat as a connection failure.
    .unwrap_or(Err(
        discord_rich_presence::error::Error::IPCConnectionFailed,
    ))?;

    // Arc<Mutex<_>> lets each spawn_blocking closure briefly acquire the guard
    // without the runtime thread pool holding it across await points.
    let client = std::sync::Arc::new(std::sync::Mutex::new(client));

    tracing::info!("Discord RPC connected");

    // Re-publish last known activity after reconnect.
    if let Some((track, artwork_url, started_at)) = last_activity.clone() {
        // Discord's daemon needs a moment after the handshake before it will
        // actually surface a set_activity; the wire write succeeds immediately
        // but the activity is silently dropped if sent too soon.
        tokio::time::sleep(tokio::time::Duration::from_millis(
            DISCORD_POST_CONNECT_DELAY_MS,
        ))
        .await;
        let client_for_publish = client.clone();
        let publish_result = tokio::task::spawn_blocking(move || {
            let activity = build_activity(&track, artwork_url.as_deref(), started_at);
            if let Ok(mut c) = client_for_publish.lock() {
                c.set_activity(activity)
            } else {
                Ok(())
            }
        })
        .await
        .unwrap_or(Ok(()));
        if let Err(e) = publish_result {
            tracing::warn!("failed to re-publish Discord activity after reconnect: {e}");
        }
    }

    let mut heartbeat = tokio::time::interval(tokio::time::Duration::from_millis(
        DISCORD_HEARTBEAT_INTERVAL_MS,
    ));
    // Skip the immediate first tick; we already wrote (or had nothing to write)
    // during the re-publish phase above.
    heartbeat.tick().await;

    loop {
        let cmd = tokio::select! {
            cmd = cmd_rx.recv() => match cmd {
                Some(c) => c,
                None => return Ok(()), // channel closed
            },
            _ = heartbeat.tick() => {
                // Re-send last_activity to detect a dead IPC socket. Idempotent on
                // Discord's side — same data means no UI change.
                if let Some((track, artwork_url, started_at)) = last_activity.clone() {
                    let client_hb = client.clone();
                    let hb_result = tokio::task::spawn_blocking(move || {
                        let activity = build_activity(&track, artwork_url.as_deref(), started_at);
                        if let Ok(mut c) = client_hb.lock() {
                            c.set_activity(activity)
                        } else {
                            Ok(())
                        }
                    })
                    .await
                    .unwrap_or(Ok(()));
                    if let Err(e) = hb_result {
                        tracing::error!("Discord heartbeat failed, triggering reconnect: {e}");
                        return Err(e.into());
                    }
                }
                continue;
            }
        };

        match cmd {
            DiscordCommand::SetActivity {
                track,
                artwork_url,
                started_at,
            } => {
                // Update last_activity state.
                *last_activity = Some((track.clone(), artwork_url.clone(), started_at));
                let client = client.clone();
                let ipc_result = tokio::task::spawn_blocking(move || {
                    // build_activity borrows from track/artwork_url, so we call it
                    // inside this closure where those values are owned and alive.
                    let activity = build_activity(&track, artwork_url.as_deref(), started_at);
                    if let Ok(mut c) = client.lock() {
                        c.set_activity(activity)
                    } else {
                        Ok(())
                    }
                })
                .await
                .unwrap_or(Ok(()));
                if let Err(e) = ipc_result {
                    tracing::error!("Discord IPC write failed, triggering reconnect: {e}");
                    return Err(e.into());
                }
            }
            DiscordCommand::ClearActivity => {
                *last_activity = None;
                let client = client.clone();
                let ipc_result = tokio::task::spawn_blocking(move || {
                    if let Ok(mut c) = client.lock() {
                        c.clear_activity()
                    } else {
                        Ok(())
                    }
                })
                .await
                .unwrap_or(Ok(()));
                if let Err(e) = ipc_result {
                    tracing::error!("Discord IPC write failed, triggering reconnect: {e}");
                    return Err(e.into());
                }
            }
            DiscordCommand::Shutdown => {
                let client = client.clone();
                tokio::task::spawn_blocking(move || {
                    if let Ok(mut c) = client.lock() {
                        let _ = c.close();
                    }
                })
                .await
                .ok();
                break;
            }
        }
    }

    Ok(())
}
