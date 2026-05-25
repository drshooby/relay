use winit::event_loop::EventLoopProxy;

use crate::artwork::cache::ArtworkCache;
use crate::artwork::itunes::{search_track, TrackLookup};
use crate::constants::{
    CHANNEL_BUFFER_SIZE, TRACK_DEBOUNCE_MS, TRAY_ERROR_DISCORD_DISCONNECTED_DETAIL,
    TRAY_ERROR_DISCORD_MESSAGE,
};
use crate::discord::activity::{compute_started_at, TrackInfo};
use crate::discord::client::{run_discord_client, DiscordCommand, DiscordStatus};
use crate::media::debounce::Debouncer;
use crate::media::event::{HelperCommand, MediaEvent};
use crate::media::reader;
use crate::tray::event_loop::UserEvent;
use crate::tray::TrayState;

/// Commands sent from the main (winit) thread to the Tokio pipeline.
#[derive(Debug)]
pub enum AppCommand {
    Quit,
}

/// Cached Discord activity fields reused for position-only updates.
#[derive(Debug, Clone)]
struct ActiveTrack {
    track: TrackInfo,
    artwork_url: Option<String>,
    track_url: Option<String>,
    duration_secs: Option<u64>,
}

/// Tracks tray-affecting health so we can restore content state after recovery.
struct TrayHealth {
    helper_error: Option<TrayState>,
    discord_connected: bool,
    discord_was_connected: bool,
    discord_error_detail: Option<String>,
    content: TrayState,
}

impl TrayHealth {
    fn new() -> Self {
        Self {
            helper_error: None,
            discord_connected: false,
            discord_was_connected: false,
            discord_error_detail: None,
            content: TrayState::Idle,
        }
    }

    fn set_content(&mut self, state: TrayState) {
        self.content = state;
    }

    fn resolved(&self) -> TrayState {
        if let Some(err) = &self.helper_error {
            return err.clone();
        }
        if self.discord_was_connected && !self.discord_connected {
            return TrayState::Error {
                message: TRAY_ERROR_DISCORD_MESSAGE.to_string(),
                detail: self
                    .discord_error_detail
                    .clone()
                    .unwrap_or_else(|| TRAY_ERROR_DISCORD_DISCONNECTED_DETAIL.to_string()),
            };
        }
        self.content.clone()
    }
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn send_set_activity(
    discord_tx: &tokio::sync::mpsc::Sender<DiscordCommand>,
    active: &ActiveTrack,
    elapsed_secs: Option<u64>,
    debounce_ms: u64,
) {
    let started_at = compute_started_at(now_unix_secs(), elapsed_secs, debounce_ms);
    let _ = discord_tx
        .send(DiscordCommand::SetActivity {
            track: active.track.clone(),
            artwork_url: active.artwork_url.clone(),
            track_url: active.track_url.clone(),
            started_at,
            duration_secs: active.duration_secs,
        })
        .await;
}

pub async fn run_pipeline(
    proxy: EventLoopProxy<UserEvent>,
    mut app_cmd_rx: tokio::sync::mpsc::Receiver<AppCommand>,
) {
    use tokio::sync::mpsc;

    let (event_tx, mut event_rx) = mpsc::channel::<MediaEvent>(CHANNEL_BUFFER_SIZE);
    let (status_tx, mut status_rx) = mpsc::channel(4);
    let (discord_tx, discord_rx) = mpsc::channel::<DiscordCommand>(CHANNEL_BUFFER_SIZE);
    let (discord_status_tx, mut discord_status_rx) = mpsc::channel::<DiscordStatus>(8);
    let (helper_cmd_tx, helper_cmd_rx) = mpsc::channel::<HelperCommand>(8);

    // Spawn the Swift helper reader/writer.
    tokio::spawn(async move {
        reader::run_helper(event_tx, status_tx, helper_cmd_rx).await;
    });

    // Spawn the Discord RPC client. Receives helper_cmd_tx so it can request
    // a fresh state snapshot from the helper after a reconnect.
    tokio::spawn(async move {
        run_discord_client(discord_rx, helper_cmd_tx, discord_status_tx).await;
    });

    // Pipeline state.
    let mut debouncer = Debouncer::new(std::time::Duration::from_millis(TRACK_DEBOUNCE_MS));
    let (debounced_tx, mut debounced_rx) = mpsc::channel::<MediaEvent>(CHANNEL_BUFFER_SIZE);
    let mut artwork_cache = tokio::task::spawn_blocking(ArtworkCache::load)
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
    let http_client = reqwest::Client::new();
    let mut tray_health = TrayHealth::new();
    let mut last_sent: Option<TrayState> = None;
    let mut active_track: Option<ActiveTrack> = None;

    let mut send_tray = |state: TrayState| {
        if last_sent.as_ref() == Some(&state) {
            return;
        }
        last_sent = Some(state.clone());
        let _ = proxy.send_event(UserEvent::StateUpdate(state));
    };

    loop {
        tokio::select! {
            // Helper process status changes (exit / IO error).
            Some(status) = status_rx.recv() => {
                if let Some(error_state) = TrayState::from_helper_status(&status) {
                    tracing::error!("helper status: {error_state:?}");
                    tray_health.helper_error = Some(error_state);
                    send_tray(tray_health.resolved());
                }
            }

            Some(discord_status) = discord_status_rx.recv() => {
                match discord_status {
                    DiscordStatus::Connected => {
                        tray_health.discord_connected = true;
                        tray_health.discord_was_connected = true;
                        tray_health.discord_error_detail = None;
                        send_tray(tray_health.resolved());
                    }
                    DiscordStatus::Disconnected { detail } => {
                        tray_health.discord_connected = false;
                        tray_health.discord_error_detail = Some(detail);
                        if tray_health.discord_was_connected {
                            send_tray(tray_health.resolved());
                        }
                    }
                    DiscordStatus::Reconnecting { backoff_ms } => {
                        tray_health.discord_connected = false;
                        tray_health.discord_error_detail =
                            Some(format!("reconnecting in {backoff_ms}ms"));
                        if tray_health.discord_was_connected {
                            send_tray(tray_health.resolved());
                        }
                    }
                }
            }

            // Raw media events from the helper.
            Some(event) = event_rx.recv() => {
                match &event {
                    MediaEvent::PositionChanged { .. } => {
                        if let MediaEvent::PositionChanged { elapsed_secs } = event {
                            if let Some(active) = active_track.as_ref() {
                                send_set_activity(
                                    &discord_tx,
                                    active,
                                    Some(elapsed_secs),
                                    0,
                                )
                                .await;
                            }
                        }
                    }
                    MediaEvent::TrackChanged { .. } => {
                        // Drop stale metadata until debounce completes so position_changed
                        // cannot advance the progress bar for the previous track.
                        active_track = None;
                        debouncer.submit(event, debounced_tx.clone());
                    }
                    MediaEvent::PlaybackPaused | MediaEvent::PlaybackStopped => {
                        active_track = None;
                        debouncer.submit(event, debounced_tx.clone());
                    }
                }
            }

            // Debounced events — look up artwork, push to Discord, refresh tray.
            Some(event) = debounced_rx.recv() => {
                match event {
                    MediaEvent::TrackChanged {
                        title,
                        artist,
                        album,
                        elapsed_secs,
                        duration_secs,
                    } => {
                        let track = TrackInfo {
                            title: title.clone(),
                            artist: artist.clone(),
                            album,
                        };

                        tray_health.set_content(TrayState::Playing {
                            title: title.clone(),
                            artist: artist.clone(),
                        });
                        send_tray(tray_health.resolved());

                        // Artwork + track URL: cache-first, then iTunes search.
                        let lookup = if let Some(cached) = artwork_cache.get(&artist, &title) {
                            cached
                        } else {
                            match search_track(&http_client, &artist, &title).await {
                                Ok(Some(found)) => {
                                    artwork_cache.insert(&artist, &title, found.clone());
                                    let cache_snapshot = artwork_cache.clone();
                                    tokio::task::spawn_blocking(move || {
                                        if let Err(e) = cache_snapshot.save() {
                                            tracing::warn!("failed to persist artwork cache: {e}");
                                        }
                                    });
                                    found
                                }
                                Ok(None) => TrackLookup {
                                    artwork_url: None,
                                    track_url: None,
                                    duration_secs: None,
                                },
                                Err(e) => {
                                    tracing::warn!("artwork lookup failed: {e}");
                                    TrackLookup {
                                        artwork_url: None,
                                        track_url: None,
                                        duration_secs: None,
                                    }
                                }
                            }
                        };

                        let resolved_duration = duration_secs.or(lookup.duration_secs);

                        let active = ActiveTrack {
                            track: track.clone(),
                            artwork_url: lookup.artwork_url.clone(),
                            track_url: lookup.track_url.clone(),
                            duration_secs: resolved_duration,
                        };
                        active_track = Some(active.clone());
                        send_set_activity(
                            &discord_tx,
                            &active,
                            elapsed_secs,
                            TRACK_DEBOUNCE_MS,
                        )
                        .await;
                    }

                    MediaEvent::PlaybackPaused | MediaEvent::PlaybackStopped => {
                        active_track = None;
                        tray_health.set_content(TrayState::Idle);
                        send_tray(tray_health.resolved());
                        let _ = discord_tx.send(DiscordCommand::ClearActivity).await;
                    }

                    MediaEvent::PositionChanged { .. } => {
                        // Position updates are handled on the raw event path.
                    }
                }
            }

            // App commands from the main thread (quit).
            cmd = app_cmd_rx.recv() => {
                match cmd {
                    Some(AppCommand::Quit) => {
                        let _ = discord_tx.send(DiscordCommand::Shutdown).await;
                        break;
                    }
                    None => break, // sender dropped — treat as quit
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{TRAY_ERROR_DISCORD_MESSAGE, TRAY_ERROR_HELPER_MESSAGE};

    #[test]
    fn resolved_prefers_helper_error_over_discord() {
        let mut health = TrayHealth::new();
        health.helper_error = Some(TrayState::Error {
            message: TRAY_ERROR_HELPER_MESSAGE.to_string(),
            detail: "helper exited".to_string(),
        });
        health.discord_connected = false;
        health.discord_was_connected = true;
        health.discord_error_detail = Some("discord ipc: lost".to_string());

        let state = health.resolved();
        assert_eq!(state.label(), format!("Relay: {TRAY_ERROR_HELPER_MESSAGE}"));
    }

    #[test]
    fn resolved_shows_discord_error_after_disconnect() {
        let mut health = TrayHealth::new();
        health.discord_connected = false;
        health.discord_was_connected = true;
        health.discord_error_detail = Some("discord ipc: pipe closed".to_string());

        let state = health.resolved();
        assert_eq!(
            state.label(),
            format!("Relay: {TRAY_ERROR_DISCORD_MESSAGE}")
        );
        assert_eq!(state.error_detail(), Some("discord ipc: pipe closed"));
    }

    #[test]
    fn resolved_no_discord_error_before_first_connection() {
        let health = TrayHealth::new();
        assert_eq!(health.resolved(), TrayState::Idle);
    }

    #[test]
    fn resolved_returns_content_when_healthy() {
        let mut health = TrayHealth::new();
        health.discord_connected = true;
        health.discord_was_connected = true;
        health.set_content(TrayState::Playing {
            title: "Song".into(),
            artist: "Artist".into(),
        });

        let state = health.resolved();
        assert!(matches!(state, TrayState::Playing { .. }));
    }

    #[test]
    fn resolved_recovers_content_after_discord_reconnect() {
        let mut health = TrayHealth::new();
        health.discord_was_connected = true;
        health.discord_connected = false;
        health.discord_error_detail = Some("reconnecting in 1000ms".to_string());
        health.set_content(TrayState::Idle);

        let err = health.resolved();
        assert!(matches!(err, TrayState::Error { .. }));

        health.discord_connected = true;
        health.discord_error_detail = None;
        assert_eq!(health.resolved(), TrayState::Idle);
    }
}
