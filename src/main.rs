use tracing_subscriber::EnvFilter;

use relay::tray::event_loop::{build_event_loop, RelayApp, UserEvent};
use relay::AppCommand;

fn main() -> anyhow::Result<()> {
    // 1. Initialise tracing — reads RUST_LOG from environment.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // 2. Load config — fall back to defaults on error (app should always start).
    let config = relay::config::load().unwrap_or_default();

    // 3. Cross-thread channel: main → Tokio (commands).
    //    tokio::sync::mpsc works here: the main (winit) thread uses blocking_send,
    //    the Tokio pipeline uses .recv().await.
    let (app_cmd_tx, app_cmd_rx) = tokio::sync::mpsc::channel::<AppCommand>(8);

    // 4. Build winit event loop on the main thread (macOS requirement).
    let event_loop = build_event_loop();
    let proxy = event_loop.create_proxy();

    // 5. Spawn the Tokio runtime on a dedicated OS thread so it never blocks the main thread.
    let _tokio_thread = std::thread::spawn(move || {
        // multi_thread scheduler: work-stealing pool.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            // Panic is acceptable at startup if the runtime cannot be created.
            .expect("failed to create tokio runtime");

        rt.block_on(async move {
            run_pipeline(proxy, app_cmd_rx, config.enabled).await;
        });
    });

    // 6. Run the winit event loop on the main thread (blocks until exit).
    run_event_loop(event_loop, app_cmd_tx)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tokio pipeline (background thread)
// ---------------------------------------------------------------------------

async fn run_pipeline(
    proxy: winit::event_loop::EventLoopProxy<UserEvent>,
    mut app_cmd_rx: tokio::sync::mpsc::Receiver<AppCommand>,
    initial_enabled: bool,
) {
    use tokio::sync::mpsc;

    use relay::artwork::cache::ArtworkCache;
    use relay::artwork::itunes::search_artwork;
    use relay::constants::TRACK_DEBOUNCE_MS;
    use relay::discord::activity::TrackInfo;
    use relay::discord::client::{run_discord_client, DiscordCommand};
    use relay::media::debounce::Debouncer;
    use relay::media::event::MediaEvent;
    use relay::media::reader;
    use relay::tray::TrayState;

    let (event_tx, mut event_rx) = mpsc::channel::<MediaEvent>(32);
    let (status_tx, mut status_rx) = mpsc::channel(4);
    let (discord_tx, discord_rx) = mpsc::channel::<DiscordCommand>(32);

    // Spawn the Swift helper reader.
    tokio::spawn(async move {
        reader::run_helper(event_tx, status_tx).await;
    });

    // Spawn the Discord RPC client.
    tokio::spawn(async move {
        run_discord_client(discord_rx).await;
    });

    // Pipeline state.
    let mut debouncer = Debouncer::new(std::time::Duration::from_millis(TRACK_DEBOUNCE_MS));
    let (debounced_tx, mut debounced_rx) = mpsc::channel::<MediaEvent>(32);
    let mut artwork_cache = ArtworkCache::load().unwrap_or_default();
    let http_client = reqwest::Client::new();
    let mut enabled = initial_enabled;

    loop {
        tokio::select! {
            // Helper process status changes (exit / IO error).
            Some(status) = status_rx.recv() => {
                if let Some(error_state) = TrayState::from_helper_status(&status) {
                    tracing::error!("helper status: {error_state:?}");
                    let _ = proxy.send_event(UserEvent::StateUpdate(error_state));
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

                        // Update tray label immediately.
                        let _ = proxy.send_event(UserEvent::StateUpdate(
                            TrayState::Playing { title: title.clone(), artist: artist.clone() },
                        ));

                        // Artwork: cache-first, then iTunes search.
                        let artwork_url = if let Some(url) = artwork_cache.get(&artist, &title) {
                            Some(url)
                        } else {
                            match search_artwork(&http_client, &artist, &title).await {
                                Ok(Some(url)) => {
                                    artwork_cache.insert(&artist, &title, url.clone());
                                    let _ = artwork_cache.save();
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
                        let _ = proxy.send_event(UserEvent::StateUpdate(TrayState::Idle));
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
                            let _ = proxy.send_event(UserEvent::StateUpdate(TrayState::Disabled));
                        } else {
                            let _ = proxy.send_event(UserEvent::StateUpdate(TrayState::Idle));
                        }
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

// ---------------------------------------------------------------------------
// Winit event loop (main thread)
// ---------------------------------------------------------------------------

fn run_event_loop(
    event_loop: winit::event_loop::EventLoop<UserEvent>,
    app_cmd_tx: tokio::sync::mpsc::Sender<AppCommand>,
) -> anyhow::Result<()> {
    use winit::event_loop::ControlFlow;

    // WaitUntil so about_to_wait is called at ~60 fps without busy-spinning.
    event_loop.set_control_flow(ControlFlow::WaitUntil(
        std::time::Instant::now() + std::time::Duration::from_millis(16),
    ));

    let mut app = RelayApp::new(app_cmd_tx);

    event_loop
        .run_app(&mut app)
        .map_err(|e| anyhow::anyhow!("winit event loop error: {e}"))?;

    Ok(())
}
