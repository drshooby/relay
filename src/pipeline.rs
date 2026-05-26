use std::sync::Arc;

use tokio::sync::RwLock;
use winit::event_loop::EventLoopProxy;

use crate::artwork::cache::ArtworkCache;
use crate::artwork::itunes::{search_track, TrackLookup};
use crate::config::{self, Config, DisplayConfig};
use crate::constants::{CHANNEL_BUFFER_SIZE, TRACK_DEBOUNCE_MS, TRAY_PERMISSION_DENIED_DETAIL};
use crate::discord::activity::{compute_started_at, TrackInfo};
use crate::discord::client::{run_discord_client, DiscordCommand, DiscordStatus};
use crate::media::debounce::Debouncer;
use crate::media::event::{HelperCommand, HelperStatus, MediaEvent};
use crate::media::reader;
use crate::tray::event_loop::UserEvent;
use crate::tray::{DiscordHealth, HelperHealth, PlaybackStatus, TrayStatus};

/// Which display field to toggle.
#[derive(Debug, Clone, Copy)]
pub enum DisplayField {
    Title,
    Artist,
    Album,
    Artwork,
}

/// Commands sent from the main (winit) thread to the Tokio pipeline.
#[derive(Debug)]
pub enum AppCommand {
    Quit,
    SetDisplayField { field: DisplayField, enabled: bool },
}

/// Cached Discord activity fields reused for position-only updates.
#[derive(Debug, Clone)]
struct ActiveTrack {
    track: TrackInfo,
    artwork_url: Option<String>,
    track_url: Option<String>,
    duration_secs: Option<u64>,
    paused: bool,
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
    display: DisplayConfig,
) {
    let started_at = compute_started_at(now_unix_secs(), elapsed_secs, debounce_ms);
    let _ = discord_tx
        .send(DiscordCommand::SetActivity {
            track: active.track.clone(),
            artwork_url: active.artwork_url.clone(),
            track_url: active.track_url.clone(),
            started_at,
            duration_secs: active.duration_secs,
            display,
        })
        .await;
}

pub async fn run_pipeline(
    proxy: EventLoopProxy<UserEvent>,
    mut app_cmd_rx: tokio::sync::mpsc::Receiver<AppCommand>,
    cfg: Arc<RwLock<Config>>,
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
    let mut status = TrayStatus::new();
    let mut last_sent: Option<TrayStatus> = None;
    let mut active_track: Option<ActiveTrack> = None;

    let mut send_status = |s: TrayStatus| {
        if last_sent.as_ref() == Some(&s) {
            return;
        }
        last_sent = Some(s.clone());
        let _ = proxy.send_event(UserEvent::StatusUpdate(s));
    };

    loop {
        tokio::select! {
            // Helper process status changes (exit / IO error).
            Some(helper_status) = status_rx.recv() => {
                match helper_status {
                    HelperStatus::Running => {}
                    HelperStatus::Exited { code } => {
                        let detail = match code {
                            Some(c) => format!("helper exited with code {}", c),
                            None => "helper exited".to_string(),
                        };
                        tracing::error!("helper exited: {detail}");
                        status.helper = HelperHealth::Unavailable { detail: detail.clone() };
                        status.last_error = Some(detail);
                        send_status(status.clone());
                    }
                    HelperStatus::IoError => {
                        let detail = "helper io error".to_string();
                        tracing::error!("{detail}");
                        status.helper = HelperHealth::Unavailable { detail: detail.clone() };
                        status.last_error = Some(detail);
                        send_status(status.clone());
                    }
                }
            }

            Some(discord_status) = discord_status_rx.recv() => {
                match discord_status {
                    DiscordStatus::Connected => {
                        tracing::info!("discord connected");
                        status.discord = DiscordHealth::Connected;
                        status.discord_was_connected = true;
                        send_status(status.clone());
                    }
                    DiscordStatus::Disconnected { detail } => {
                        status.discord = DiscordHealth::Disconnected { detail };
                        if status.discord_was_connected {
                            send_status(status.clone());
                        }
                    }
                    DiscordStatus::Reconnecting { backoff_ms } => {
                        status.discord = DiscordHealth::Reconnecting { backoff_ms };
                        if status.discord_was_connected {
                            send_status(status.clone());
                        }
                    }
                }
            }

            // Raw media events from the helper.
            Some(event) = event_rx.recv() => {
                match &event {
                    MediaEvent::PermissionDenied => {
                        tracing::warn!("apple music permission denied");
                        status.helper = HelperHealth::PermissionDenied;
                        status.last_error = Some(TRAY_PERMISSION_DENIED_DETAIL.to_string());
                        send_status(status.clone());
                    }
                    MediaEvent::PositionChanged { .. } => {
                        if let MediaEvent::PositionChanged { elapsed_secs } = event {
                            if let Some(active) = active_track.as_ref() {
                                // Skip position updates while paused — adding timestamps to
                                // a paused card would incorrectly re-enable the progress bar.
                                if !active.paused {
                                    let display = cfg.read().await.display.clone();
                                    send_set_activity(
                                        &discord_tx,
                                        active,
                                        Some(elapsed_secs),
                                        0,
                                        display,
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                    MediaEvent::TrackChanged { .. } => {
                        // Clear permission-denied state on recovery.
                        if matches!(status.helper, HelperHealth::PermissionDenied) {
                            status.helper = HelperHealth::Running;
                            status.last_error = None;
                        }
                        // Drop stale metadata until debounce completes so position_changed
                        // cannot advance the progress bar for the previous track.
                        active_track = None;
                        debouncer.submit(event, debounced_tx.clone());
                    }
                    MediaEvent::PlaybackPaused => {
                        // Clear permission-denied state on recovery.
                        if matches!(status.helper, HelperHealth::PermissionDenied) {
                            status.helper = HelperHealth::Running;
                            status.last_error = None;
                        }
                        // Do NOT clear active_track here — let the debounced handler
                        // mutate the paused flag so rapid pause→resume doesn't lose
                        // track context.
                        debouncer.submit(event, debounced_tx.clone());
                    }
                    MediaEvent::PlaybackStopped => {
                        // Clear permission-denied state on recovery.
                        if matches!(status.helper, HelperHealth::PermissionDenied) {
                            status.helper = HelperHealth::Running;
                            status.last_error = None;
                        }
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

                        status.playback = PlaybackStatus::Playing {
                            title: title.clone(),
                            artist: artist.clone(),
                        };
                        send_status(status.clone());

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
                            paused: false,
                        };
                        active_track = Some(active.clone());
                        let display = cfg.read().await.display.clone();
                        send_set_activity(
                            &discord_tx,
                            &active,
                            elapsed_secs,
                            TRACK_DEBOUNCE_MS,
                            display,
                        )
                        .await;
                    }

                    MediaEvent::PlaybackPaused => {
                        if let Some(active) = active_track.as_mut() {
                            active.paused = true;
                            status.playback = PlaybackStatus::Paused {
                                title: active.track.title.clone(),
                                artist: active.track.artist.clone(),
                            };
                            send_status(status.clone());
                            let display = cfg.read().await.display.clone();
                            let _ = discord_tx
                                .send(DiscordCommand::SetPausedActivity {
                                    track: active.track.clone(),
                                    artwork_url: active.artwork_url.clone(),
                                    track_url: active.track_url.clone(),
                                    display,
                                })
                                .await;
                        } else {
                            // No active track established yet — safe fallback.
                            status.playback = PlaybackStatus::Idle;
                            send_status(status.clone());
                            let _ = discord_tx.send(DiscordCommand::ClearActivity).await;
                        }
                    }

                    MediaEvent::PlaybackStopped => {
                        active_track = None;
                        status.playback = PlaybackStatus::Idle;
                        send_status(status.clone());
                        let _ = discord_tx.send(DiscordCommand::ClearActivity).await;
                    }

                    MediaEvent::PositionChanged { .. } => {
                        // Position updates are handled on the raw event path.
                    }

                    MediaEvent::PermissionDenied => {
                        // Already handled on raw event path.
                    }
                }
            }

            // App commands from the main thread (quit / display toggles).
            cmd = app_cmd_rx.recv() => {
                match cmd {
                    Some(AppCommand::Quit) => {
                        let _ = discord_tx.send(DiscordCommand::Shutdown).await;
                        break;
                    }
                    Some(AppCommand::SetDisplayField { field, enabled }) => {
                        // Mutate the shared config.
                        {
                            let mut guard = cfg.write().await;
                            match field {
                                DisplayField::Title => guard.display.show_title = enabled,
                                DisplayField::Artist => guard.display.show_artist = enabled,
                                DisplayField::Album => guard.display.show_album = enabled,
                                DisplayField::Artwork => guard.display.show_artwork = enabled,
                            }
                        }
                        tracing::info!(
                            "display field {:?} set to {enabled}",
                            field
                        );

                        // Persist asynchronously — failure is non-fatal.
                        let snapshot = cfg.read().await.clone();
                        tokio::task::spawn_blocking(move || {
                            if let Err(e) = config::save(&snapshot) {
                                tracing::warn!("failed to persist config after display toggle: {e}");
                            }
                        });

                        // Force-republish with the new display snapshot.
                        let display = cfg.read().await.display.clone();
                        if let Some(active) = active_track.as_ref() {
                            if active.paused {
                                let _ = discord_tx
                                    .send(DiscordCommand::SetPausedActivity {
                                        track: active.track.clone(),
                                        artwork_url: active.artwork_url.clone(),
                                        track_url: active.track_url.clone(),
                                        display,
                                    })
                                    .await;
                            } else {
                                send_set_activity(
                                    &discord_tx,
                                    active,
                                    None,
                                    0,
                                    display,
                                )
                                .await;
                            }
                        }
                    }
                    None => break, // sender dropped — treat as quit
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::constants::TRAY_PERMISSION_DENIED_DETAIL;
    use crate::tray::icons::TrayIconVariant;
    use crate::tray::{DiscordHealth, HelperHealth, TrayStatus};

    #[test]
    fn status_shows_discord_error_after_disconnect() {
        let mut status = TrayStatus::new();
        status.discord_was_connected = true;
        status.discord = DiscordHealth::Disconnected {
            detail: "discord ipc: pipe closed".to_string(),
        };
        assert_eq!(status.discord.row_text(), "Discord: Disconnected");
        assert_eq!(status.icon_variant(), TrayIconVariant::Error);
    }

    #[test]
    fn status_no_discord_error_before_first_connection() {
        let mut s = TrayStatus::new();
        s.discord = DiscordHealth::Disconnected {
            detail: "not yet connected".into(),
        };
        // discord_was_connected is still false
        assert_eq!(s.icon_variant(), TrayIconVariant::Normal);
    }

    #[test]
    fn status_returns_to_normal_after_discord_reconnect() {
        let mut status = TrayStatus::new();
        status.discord_was_connected = true;
        status.discord = DiscordHealth::Disconnected {
            detail: "lost".into(),
        };
        assert_eq!(status.icon_variant(), TrayIconVariant::Error);

        status.discord = DiscordHealth::Connected;
        assert_eq!(status.icon_variant(), TrayIconVariant::Normal);
    }

    #[test]
    fn status_helper_unavailable_sets_error() {
        let mut status = TrayStatus::new();
        let detail = "helper exited with code 1".to_string();
        status.helper = HelperHealth::Unavailable {
            detail: detail.clone(),
        };
        status.last_error = Some(detail);
        assert_eq!(status.icon_variant(), TrayIconVariant::Error);
    }

    #[test]
    fn permission_denied_then_track_changed_clears_helper_state() {
        let mut status = TrayStatus::new();

        // Simulate permission denied
        status.helper = HelperHealth::PermissionDenied;
        status.last_error = Some(TRAY_PERMISSION_DENIED_DETAIL.to_string());

        assert_eq!(status.helper, HelperHealth::PermissionDenied);
        assert!(status.last_error.is_some());

        // Simulate a successful track event arriving — should clear the permission state
        if matches!(status.helper, HelperHealth::PermissionDenied) {
            status.helper = HelperHealth::Running;
            status.last_error = None;
        }

        assert_eq!(status.helper, HelperHealth::Running);
        assert!(status.last_error.is_none());
    }
}
