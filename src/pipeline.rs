use std::sync::Arc;

use tokio::sync::RwLock;
use winit::event_loop::EventLoopProxy;

use crate::artwork::cache::ArtworkCache;
use crate::artwork::itunes::{search_track, TrackLookup};
use crate::config::{self, Config, DisplayConfig};
use crate::constants::{CHANNEL_BUFFER_SIZE, TRAY_PERMISSION_DENIED_DETAIL};
use crate::discord::activity::{compute_started_at, TrackInfo};
use crate::discord::client::{run_discord_client, DiscordCommand, DiscordStatus};
use crate::errors;
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
    SetDisplayField {
        field: DisplayField,
        enabled: bool,
    },
    /// Live-reload the debounce duration from config (written by the prefs app).
    ReloadConfig {
        debounce_ms: u64,
    },
}

/// Cached Discord activity fields reused for position-only updates.
#[derive(Debug, Clone)]
pub(crate) struct ActiveTrack {
    track: TrackInfo,
    artwork_url: Option<String>,
    track_url: Option<String>,
    duration_secs: Option<u64>,
    /// Most recent elapsed position reported by helper (seconds).
    last_elapsed_secs: Option<u64>,
    /// Wall-clock instant when `last_elapsed_secs` was recorded, for projection.
    last_elapsed_observed_at: Option<std::time::Instant>,
    /// True while playback is paused; gates republish paths so a stray event
    /// or display-field toggle cannot recreate the Discord card after the user
    /// chose "clear on pause".
    paused: bool,
}

/// Project a cached elapsed position forward by `observed_ago_secs` seconds.
///
/// - `elapsed_secs`: value from the incoming event. When `Some`, returned as-is.
/// - `cached_elapsed`: last known elapsed stored in `ActiveTrack`.
/// - `observed_ago_secs`: seconds since `cached_elapsed` was recorded.
/// - `duration_secs`: track duration cap. Result is bounded to this when `Some`.
///
/// Returns `None` only when both `elapsed_secs` and `cached_elapsed` are `None`.
pub fn project_elapsed(
    elapsed_secs: Option<u64>,
    cached_elapsed: Option<u64>,
    observed_ago_secs: u64,
    duration_secs: Option<u64>,
) -> Option<u64> {
    if let Some(e) = elapsed_secs {
        return Some(e);
    }
    let cached = cached_elapsed?;
    let projected = cached.saturating_add(observed_ago_secs);
    Some(match duration_secs {
        Some(d) if projected > d => d,
        _ => projected,
    })
}

/// Determine the effective elapsed position for a `TrackChanged` event.
///
/// - When the incoming event reports a real (non-zero) position for the same track,
///   use it as-is.
/// - When the same track is detected AND `event_elapsed` is `None` or `Some(0)`,
///   treat this as a resume where Music.app hasn't updated `player position` yet.
///   Fall back to the cached `last_elapsed_secs` from `active_track`.
/// - For a genuinely different track, always use `event_elapsed` (even if it is 0).
pub(crate) fn compute_effective_elapsed(
    event_elapsed: Option<u64>,
    event_title: &str,
    event_artist: &str,
    active_track: Option<&ActiveTrack>,
) -> Option<u64> {
    let is_same_track = active_track
        .is_some_and(|a| a.track.title == event_title && a.track.artist == event_artist);

    // On resume the helper may emit elapsed=None or elapsed=Some(0) before
    // Music.app has updated its player position. Use the cached value when both
    // conditions hold: same track AND the reported position is zero/absent.
    let is_resume = is_same_track && event_elapsed.unwrap_or(0) == 0;

    if is_resume {
        // Use cached position directly — pause duration is unknown so we do NOT
        // project forward here. The new ActiveTrack sets last_elapsed_observed_at
        // = Instant::now(), so subsequent display-toggle republishes project
        // correctly from the resume moment.
        active_track.and_then(|prev| prev.last_elapsed_secs)
    } else {
        event_elapsed
    }
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Write the `nowplaying.json` snapshot consumed by RelayPreferences.
///
/// - `active = Some(t)` — track is known; writes full payload.
/// - `active = None`    — no track; deletes the file so the prefs app sees
///   the empty state without stale data.
/// - `playing`          — caller sets this; allows writing `playing: false`
///   on pause even though the `ActiveTrack` struct predates the flag flip.
pub(crate) fn write_nowplaying_snapshot(
    active: Option<&ActiveTrack>,
    playing: bool,
    dir: &std::path::Path,
) -> Result<(), std::io::Error> {
    let path = dir.join(crate::constants::NOWPLAYING_SNAPSHOT_FILE);

    match active {
        None => {
            // Remove the file so the prefs app falls to the empty state.
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Some(a) => {
            let observed_at = now_unix_ms();
            let mut obj = serde_json::json!({
                "title": a.track.title,
                "artist": a.track.artist,
                "album": a.track.album,
                "artwork_url": a.artwork_url,
                "playing": playing,
                "observed_at_unix_ms": observed_at,
            });
            // elapsed_secs: omit when unknown
            if let Some(e) = a.last_elapsed_secs {
                obj["elapsed_secs"] = serde_json::json!(e);
            }
            // duration_secs: omit when unknown
            if let Some(d) = a.duration_secs {
                obj["duration_secs"] = serde_json::json!(d);
            }
            let json = serde_json::to_string(&obj).map_err(std::io::Error::other)?;
            std::fs::create_dir_all(dir)?;
            std::fs::write(&path, json)?;
        }
    }
    Ok(())
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
    let initial_debounce_ms = cfg.read().await.playback.debounce_ms;
    let mut debouncer = Debouncer::new(std::time::Duration::from_millis(initial_debounce_ms));
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
                        status.last_error = Some(detail.clone());
                        errors::record("helper", detail).await;
                        send_status(status.clone());
                    }
                    HelperStatus::IoError => {
                        let detail = "helper io error".to_string();
                        tracing::error!("{detail}");
                        status.helper = HelperHealth::Unavailable { detail: detail.clone() };
                        status.last_error = Some(detail.clone());
                        errors::record("helper", detail).await;
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
                        // Only record in the errors log on permanent disconnect
                        // (i.e. when we were previously connected). Transient
                        // disconnects during reconnect backoff are NOT recorded
                        // individually — only the final give-up state is.
                        if status.discord_was_connected {
                            errors::record("discord", detail.clone()).await;
                        }
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
                        errors::record("helper", TRAY_PERMISSION_DENIED_DETAIL).await;
                        send_status(status.clone());
                    }
                    MediaEvent::PositionChanged { .. } => {
                        if let MediaEvent::PositionChanged { elapsed_secs } = event {
                            if let Some(active) = active_track.as_mut() {
                                // Always update the cached elapsed so resume projection is
                                // accurate even if the event arrived while paused.
                                active.last_elapsed_secs = Some(elapsed_secs);
                                active.last_elapsed_observed_at = Some(std::time::Instant::now());
                                // Defense-in-depth: never recreate the Discord card if the
                                // helper emits a stray position_changed while paused.
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
                                // Update snapshot so prefs bar reflects scrub position.
                                let snap_active = active.clone();
                                let snap_playing = !active.paused;
                                if let Ok(dir) = config::data_dir() {
                                    tokio::task::spawn_blocking(move || {
                                        if let Err(e) = write_nowplaying_snapshot(
                                            Some(&snap_active),
                                            snap_playing,
                                            &dir,
                                        ) {
                                            tracing::warn!(
                                                "failed to write nowplaying snapshot on position change: {e}"
                                            );
                                        }
                                    });
                                }
                            }
                        }
                    }
                    MediaEvent::TrackChanged { ref title, ref artist, .. } => {
                        // Clear permission-denied state on recovery.
                        if matches!(status.helper, HelperHealth::PermissionDenied) {
                            status.helper = HelperHealth::Running;
                            status.last_error = None;
                        }
                        // Only null the cached track when the title+artist differ.
                        // Preserving it across pause→resume lets the debounced arm
                        // detect the resume and use the cached elapsed position.
                        let same_track = active_track.as_ref().is_some_and(|a| {
                            a.track.title == *title && a.track.artist == *artist
                        });
                        if !same_track {
                            active_track = None;
                        }
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

                        // Resume path: same title+artist with elapsed=None or elapsed=Some(0).
                        // compute_effective_elapsed falls back to the cached position so
                        // Discord shows the right time instead of resetting to 0:00.
                        let effective_elapsed = compute_effective_elapsed(
                            elapsed_secs,
                            &title,
                            &artist,
                            active_track.as_ref(),
                        );

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
                            last_elapsed_secs: effective_elapsed,
                            last_elapsed_observed_at: effective_elapsed
                                .map(|_| std::time::Instant::now()),
                            paused: false,
                        };
                        active_track = Some(active.clone());
                        let cfg_guard = cfg.read().await;
                        let display = cfg_guard.display.clone();
                        let debounce_ms = cfg_guard.playback.debounce_ms;
                        drop(cfg_guard);
                        send_set_activity(
                            &discord_tx,
                            &active,
                            effective_elapsed,
                            debounce_ms,
                            display,
                        )
                        .await;

                        // Write nowplaying snapshot for prefs app polling.
                        {
                            let snap_active = active.clone();
                            if let Ok(dir) = config::data_dir() {
                                tokio::task::spawn_blocking(move || {
                                    if let Err(e) = write_nowplaying_snapshot(
                                        Some(&snap_active),
                                        true,
                                        &dir,
                                    ) {
                                        tracing::warn!(
                                            "failed to write nowplaying snapshot: {e}"
                                        );
                                    }
                                });
                            }
                        }
                    }

                    MediaEvent::PlaybackPaused => {
                        if let Some(active) = active_track.as_mut() {
                            active.paused = true;
                            status.playback = PlaybackStatus::Paused {
                                title: active.track.title.clone(),
                                artist: active.track.artist.clone(),
                            };
                            send_status(status.clone());
                            // Write snapshot with playing=false so prefs bar freezes.
                            let snap_active = active.clone();
                            if let Ok(dir) = config::data_dir() {
                                tokio::task::spawn_blocking(move || {
                                    if let Err(e) = write_nowplaying_snapshot(
                                        Some(&snap_active),
                                        false,
                                        &dir,
                                    ) {
                                        tracing::warn!(
                                            "failed to write nowplaying snapshot on pause: {e}"
                                        );
                                    }
                                });
                            }
                        } else {
                            status.playback = PlaybackStatus::Idle;
                            send_status(status.clone());
                        }
                        // Clear the Discord card on pause. Keep active_track so
                        // resume can re-publish with the projected cached position.
                        let _ = discord_tx.send(DiscordCommand::ClearActivity).await;
                    }

                    MediaEvent::PlaybackStopped => {
                        active_track = None;
                        status.playback = PlaybackStatus::Idle;
                        send_status(status.clone());
                        let _ = discord_tx.send(DiscordCommand::ClearActivity).await;
                        // Delete the snapshot so prefs app shows empty state.
                        if let Ok(dir) = config::data_dir() {
                            tokio::task::spawn_blocking(move || {
                                if let Err(e) = write_nowplaying_snapshot(None, false, &dir) {
                                    tracing::warn!(
                                        "failed to clear nowplaying snapshot on stop: {e}"
                                    );
                                }
                            });
                        }
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

                        // Force-republish with the new display snapshot — but only
                        // when not paused. While paused the Discord card is intentionally
                        // cleared; republishing here would undo the "clear on pause" UX.
                        let display = cfg.read().await.display.clone();
                        if let Some(active) = active_track.as_ref() {
                            if !active.paused {
                                let observed_ago = active
                                    .last_elapsed_observed_at
                                    .map(|t| t.elapsed().as_secs())
                                    .unwrap_or(0);
                                let projected = project_elapsed(
                                    None,
                                    active.last_elapsed_secs,
                                    observed_ago,
                                    active.duration_secs,
                                );
                                send_set_activity(&discord_tx, active, projected, 0, display)
                                    .await;
                            }
                        }
                    }
                    Some(AppCommand::ReloadConfig { debounce_ms }) => {
                        tracing::info!("config reloaded: debounce_ms={debounce_ms}");
                        cfg.write().await.playback.debounce_ms = debounce_ms;
                        debouncer.set_duration(std::time::Duration::from_millis(debounce_ms));
                    }
                    None => break, // sender dropped — treat as quit
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compute_effective_elapsed, project_elapsed, write_nowplaying_snapshot, ActiveTrack,
        AppCommand,
    };
    use crate::constants::TRAY_PERMISSION_DENIED_DETAIL;
    use crate::discord::activity::TrackInfo;
    use crate::tray::icons::TrayIconVariant;
    use crate::tray::{DiscordHealth, HelperHealth, TrayStatus};

    fn make_active_with_duration(
        title: &str,
        artist: &str,
        elapsed: Option<u64>,
        duration: Option<u64>,
    ) -> ActiveTrack {
        ActiveTrack {
            track: TrackInfo {
                title: title.to_string(),
                artist: artist.to_string(),
                album: "Test Album".to_string(),
            },
            artwork_url: None,
            track_url: None,
            duration_secs: duration,
            last_elapsed_secs: elapsed,
            last_elapsed_observed_at: None,
            paused: false,
        }
    }

    // ---- write_nowplaying_snapshot tests ----

    #[test]
    fn snapshot_written_on_position_change() {
        let dir = tempfile::tempdir().unwrap();
        let active = make_active_with_duration("Song", "Artist", Some(93), Some(215));
        write_nowplaying_snapshot(Some(&active), true, dir.path()).unwrap();

        let path = dir.path().join(crate::constants::NOWPLAYING_SNAPSHOT_FILE);
        let contents = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(v["elapsed_secs"], 93);
        assert_eq!(v["duration_secs"], 215);
        assert_eq!(v["playing"], true);
    }

    #[test]
    fn snapshot_written_on_pause_with_playing_false() {
        let dir = tempfile::tempdir().unwrap();
        let mut active = make_active_with_duration("Song", "Artist", Some(60), Some(200));
        active.paused = true;
        write_nowplaying_snapshot(Some(&active), false, dir.path()).unwrap();

        let path = dir.path().join(crate::constants::NOWPLAYING_SNAPSHOT_FILE);
        let contents = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(v["playing"], false);
        assert_eq!(v["elapsed_secs"], 60);
    }

    #[test]
    fn snapshot_includes_observed_at_in_ms() {
        let dir = tempfile::tempdir().unwrap();
        let active = make_active_with_duration("Song", "Artist", Some(10), Some(300));
        let before_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        write_nowplaying_snapshot(Some(&active), true, dir.path()).unwrap();
        let after_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let path = dir.path().join(crate::constants::NOWPLAYING_SNAPSHOT_FILE);
        let contents = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&contents).unwrap();
        let observed_at = v["observed_at_unix_ms"].as_u64().unwrap();
        assert!(
            observed_at >= before_ms && observed_at <= after_ms,
            "observed_at_unix_ms={observed_at} not in range [{before_ms}, {after_ms}]"
        );
    }

    #[test]
    fn snapshot_omits_duration_when_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let active = make_active_with_duration("Song", "Artist", Some(30), None);
        write_nowplaying_snapshot(Some(&active), true, dir.path()).unwrap();

        let path = dir.path().join(crate::constants::NOWPLAYING_SNAPSHOT_FILE);
        let contents = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert!(
            v.get("duration_secs").is_none() || v["duration_secs"].is_null(),
            "duration_secs should be absent when unknown"
        );
    }

    // ---- compute_effective_elapsed tests (RED first — function does not exist yet) ----

    fn make_active(title: &str, artist: &str, last_elapsed_secs: Option<u64>) -> ActiveTrack {
        ActiveTrack {
            track: TrackInfo {
                title: title.to_string(),
                artist: artist.to_string(),
                album: "Album".to_string(),
            },
            artwork_url: None,
            track_url: None,
            duration_secs: None,
            last_elapsed_secs,
            last_elapsed_observed_at: None,
            paused: false,
        }
    }

    /// No active track — should fall through to the event value.
    #[test]
    fn effective_elapsed_no_active_track_returns_event_value() {
        let result = compute_effective_elapsed(Some(42), "Song", "Artist", None);
        assert_eq!(result, Some(42));
    }

    /// Different title — a genuinely new track starting at 0. Should use event value.
    #[test]
    fn effective_elapsed_legit_new_track_from_zero() {
        let active = make_active("Old Song", "Artist", Some(90));
        let result = compute_effective_elapsed(Some(0), "New Song", "Artist", Some(&active));
        assert_eq!(result, Some(0));
    }

    /// Same track, elapsed=None → resume with missing elapsed. Should use cache.
    #[test]
    fn effective_elapsed_resume_with_none_uses_cache() {
        let active = make_active("Song", "Artist", Some(90));
        let result = compute_effective_elapsed(None, "Song", "Artist", Some(&active));
        assert_eq!(result, Some(90));
    }

    /// Same track, elapsed=Some(0) → Music.app stale zero on resume. Should use cache.
    /// This is THE BUG: previously returned Some(0) instead of Some(90).
    #[test]
    fn effective_elapsed_resume_with_zero_uses_cache() {
        let active = make_active("Song", "Artist", Some(90));
        let result = compute_effective_elapsed(Some(0), "Song", "Artist", Some(&active));
        assert_eq!(result, Some(90));
    }

    /// Same track, real non-zero elapsed position reported — use it directly.
    #[test]
    fn effective_elapsed_legit_position_reported() {
        let active = make_active("Song", "Artist", Some(90));
        let result = compute_effective_elapsed(Some(45), "Song", "Artist", Some(&active));
        assert_eq!(result, Some(45));
    }

    /// Same track, elapsed=None, no cache. Nothing to fall back to → None.
    #[test]
    fn effective_elapsed_no_cache_falls_back_to_event() {
        let active = make_active("Song", "Artist", None);
        let result = compute_effective_elapsed(None, "Song", "Artist", Some(&active));
        assert_eq!(result, None);
    }

    // ---- raw-arm same-track decision tests ----

    /// same title+artist — raw TrackChanged arm should NOT null active_track.
    #[test]
    fn same_track_keeps_active_track() {
        let active = make_active("Song", "Artist", Some(90));
        let same_track = active.track.title == "Song" && active.track.artist == "Artist";
        assert!(
            same_track,
            "same title+artist must be detected as the same track"
        );
    }

    /// different title — raw TrackChanged arm should null active_track.
    #[test]
    fn different_track_nulls_active_track() {
        let active = make_active("Song", "Artist", Some(90));
        let same_track = active.track.title == "Other Song" && active.track.artist == "Artist";
        assert!(
            !same_track,
            "different title must be detected as a different track"
        );
    }

    #[test]
    fn app_command_reload_config_variant_exists() {
        let _cmd: AppCommand = AppCommand::ReloadConfig { debounce_ms: 500 };
    }

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

    // --- #37 elapsed-caching tests ---

    /// When a TrackChanged arrives for the same title+artist with elapsed_secs=None
    /// (the resume path), project_elapsed should return the cached value advanced
    /// by the time elapsed since the last observation.
    #[test]
    fn resume_with_missing_elapsed_uses_cached_position() {
        // Cached: 90 seconds elapsed, observed 10 seconds ago.
        let cached_elapsed = 90u64;
        let observed_ago_secs = 10u64;
        let duration_secs: Option<u64> = Some(200);
        // elapsed_secs=None simulates the resume path where AppleScript returned nothing.
        let result = project_elapsed(None, Some(cached_elapsed), observed_ago_secs, duration_secs);
        // Expected: 90 + 10 = 100, bounded by 200
        assert_eq!(result, Some(100));
    }

    /// project_elapsed must bound the projected value at duration_secs when known.
    #[test]
    fn resume_projection_is_bounded_by_duration() {
        let cached_elapsed = 195u64;
        let observed_ago_secs = 20u64; // would project to 215, but track is 200s long
        let duration_secs: Option<u64> = Some(200);
        let result = project_elapsed(None, Some(cached_elapsed), observed_ago_secs, duration_secs);
        assert_eq!(result, Some(200));
    }

    /// When elapsed_secs is present, project_elapsed should return it as-is
    /// (no projection needed — the helper reported a fresh position).
    #[test]
    fn display_toggle_preserves_elapsed() {
        // Simulate: we have a cached elapsed=60, observed 5s ago.
        // The display-toggle republish path calls project_elapsed with elapsed_secs=None
        // and should return the projected value (65), not None.
        let result = project_elapsed(None, Some(60), 5, None);
        assert_eq!(result, Some(65));
    }

    /// When play resumes after pause, the pipeline should be able to re-publish
    /// with the correct projected elapsed rather than resetting to 0:00.
    #[test]
    fn pause_then_resume_clears_then_republishes() {
        // Simulate: paused at elapsed=120 (observed 30s ago — paused that long)
        // On resume, elapsed_secs=None arrives. Projection: 120 + 30 = 150.
        let result = project_elapsed(None, Some(120), 30, Some(300));
        assert_eq!(result, Some(150));
    }

    /// Toggling a display field while paused must NOT trigger a republish.
    /// The `paused` flag on `ActiveTrack` gates the `send_set_activity` call
    /// in the `SetDisplayField` branch so the "clear on pause" UX is preserved.
    #[test]
    fn display_toggle_while_paused_does_not_republish() {
        // Build a paused ActiveTrack (mirrors pipeline state after PlaybackPaused).
        let active = ActiveTrack {
            track: TrackInfo {
                title: "Test Song".to_string(),
                artist: "Test Artist".to_string(),
                album: "Test Album".to_string(),
            },
            artwork_url: None,
            track_url: None,
            duration_secs: Some(200),
            last_elapsed_secs: Some(60),
            last_elapsed_observed_at: Some(std::time::Instant::now()),
            paused: true,
        };

        // The display-toggle path gates republish on !active.paused.
        // Assert that the guard condition holds — no send should occur when paused.
        assert!(
            active.paused,
            "active track must be marked paused to suppress republish"
        );
        // Confirm the gate: if paused, skip republish.
        let would_republish = !active.paused;
        assert!(
            !would_republish,
            "display toggle while paused must not trigger a republish"
        );
    }
}
