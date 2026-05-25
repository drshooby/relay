use winit::event_loop::EventLoopProxy;

use crate::artwork::cache::ArtworkCache;
use crate::artwork::itunes::search_artwork;
use crate::constants::{
    CHANNEL_BUFFER_SIZE, TRACK_DEBOUNCE_MS, TRAY_ERROR_DISCORD_MESSAGE,
};
use crate::discord::activity::TrackInfo;
use crate::discord::client::{run_discord_client, DiscordCommand, DiscordStatus};
use crate::media::debounce::Debouncer;
use crate::media::event::{HelperCommand, MediaEvent};
use crate::media::reader;
use crate::tray::event_loop::UserEvent;
use crate::tray::TrayState;

/// Commands sent from the main (winit) thread to the Tokio pipeline.
#[derive(Debug)]
pub enum AppCommand {
    SetEnabled(bool),
    Quit,
}

/// Tracks tray-affecting health so we can restore content state after recovery.
struct TrayHealth {
    helper_error: Option<TrayState>,
    discord_connected: bool,
    discord_was_connected: bool,
    content: TrayState,
}

impl TrayHealth {
    fn new(initial_enabled: bool) -> Self {
        Self {
            helper_error: None,
            discord_connected: false,
            discord_was_connected: false,
            content: if initial_enabled {
                TrayState::Idle
            } else {
                TrayState::Disabled
            },
        }
    }

    fn set_content(&mut self, state: TrayState) {
        self.content = state;
    }

    fn resolved(&self, enabled: bool) -> TrayState {
        if let Some(err) = &self.helper_error {
            return err.clone();
        }
        if enabled && self.discord_was_connected && !self.discord_connected {
            return TrayState::Error {
                message: TRAY_ERROR_DISCORD_MESSAGE.to_string(),
                detail: "discord ipc: disconnected".to_string(),
            };
        }
        if enabled {
            self.content.clone()
        } else {
            TrayState::Disabled
        }
    }

    fn discord_error(detail: String) -> TrayState {
        TrayState::Error {
            message: TRAY_ERROR_DISCORD_MESSAGE.to_string(),
            detail,
        }
    }
}

pub async fn run_pipeline(
    proxy: EventLoopProxy<UserEvent>,
    mut app_cmd_rx: tokio::sync::mpsc::Receiver<AppCommand>,
    initial_enabled: bool,
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
    let mut enabled = initial_enabled;
    let mut tray_health = TrayHealth::new(initial_enabled);
    let mut last_sent: Option<TrayState> = None;

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
                    send_tray(tray_health.resolved(enabled));
                }
            }

            Some(discord_status) = discord_status_rx.recv() => {
                match discord_status {
                    DiscordStatus::Connected => {
                        tray_health.discord_connected = true;
                        tray_health.discord_was_connected = true;
                        send_tray(tray_health.resolved(enabled));
                    }
                    DiscordStatus::Disconnected { detail } => {
                        tray_health.discord_connected = false;
                        if enabled && tray_health.discord_was_connected {
                            send_tray(TrayHealth::discord_error(detail));
                        }
                    }
                    DiscordStatus::Reconnecting { backoff_ms } => {
                        tray_health.discord_connected = false;
                        if enabled && tray_health.discord_was_connected {
                            send_tray(TrayHealth::discord_error(format!(
                                "reconnecting in {}ms",
                                backoff_ms
                            )));
                        }
                    }
                }
            }

            // Raw media events from the helper — debounce them.
            Some(event) = event_rx.recv() => {
                if enabled {
                    debouncer.submit(event, debounced_tx.clone());
                }
            }

            // Debounced events — look up artwork, push to Discord, refresh tray.
            Some(event) = debounced_rx.recv() => {
                match event {
                    MediaEvent::TrackChanged { title, artist, album } => {
                        let track = TrackInfo {
                            title: title.clone(),
                            artist: artist.clone(),
                            album,
                        };

                        tray_health.set_content(TrayState::Playing {
                            title: title.clone(),
                            artist: artist.clone(),
                        });
                        send_tray(tray_health.resolved(enabled));

                        // Artwork: cache-first, then iTunes search.
                        let artwork_url = if let Some(url) = artwork_cache.get(&artist, &title) {
                            Some(url)
                        } else {
                            match search_artwork(&http_client, &artist, &title).await {
                                Ok(Some(url)) => {
                                    artwork_cache.insert(&artist, &title, url.clone());
                                    let cache_snapshot = artwork_cache.clone();
                                    tokio::task::spawn_blocking(move || {
                                        if let Err(e) = cache_snapshot.save() {
                                            tracing::warn!("failed to persist artwork cache: {e}");
                                        }
                                    });
                                    Some(url)
                                }
                                Ok(None) => None,
                                Err(e) => {
                                    tracing::warn!("artwork lookup failed: {e}");
                                    None
                                }
                            }
                        };

                        let started_at = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64;

                        let _ = discord_tx
                            .send(DiscordCommand::SetActivity { track, artwork_url, started_at })
                            .await;
                    }

                    MediaEvent::PlaybackPaused | MediaEvent::PlaybackStopped => {
                        tray_health.set_content(TrayState::Idle);
                        send_tray(tray_health.resolved(enabled));
                        let _ = discord_tx.send(DiscordCommand::ClearActivity).await;
                    }
                }
            }

            // App commands from the main thread (toggle / quit).
            cmd = app_cmd_rx.recv() => {
                match cmd {
                    Some(AppCommand::SetEnabled(val)) => {
                        enabled = val;
                        if !val {
                            let _ = discord_tx.send(DiscordCommand::ClearActivity).await;
                            tray_health.set_content(TrayState::Disabled);
                        } else {
                            tray_health.set_content(TrayState::Idle);
                        }
                        send_tray(tray_health.resolved(enabled));
                    }
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
