use std::sync::Arc;

use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

use relay::config::{self, Config};
use relay::pipeline::{run_pipeline, AppCommand};
use relay::tray::event_loop::{build_event_loop, RelayApp, UserEvent};

fn main() -> anyhow::Result<()> {
    // 1. Initialise tracing — default to "off" so production builds are silent.
    //    Set RUST_LOG=info (or debug/trace) to enable logs.
    let filter = std::env::var("RUST_LOG")
        .map(EnvFilter::new)
        .unwrap_or_else(|_| EnvFilter::new("off"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    // 2. First-run onboarding: fire a notification before the helper spawns so the
    //    user sees Relay's copy *before* the OS Automation permission prompt.
    relay::onboarding::maybe_show_first_run_notification();

    // 3. Load config — fall back to default on any error (e.g. first run).
    let initial_config = config::load().unwrap_or_else(|e| {
        tracing::warn!("failed to load config, using defaults: {e}");
        Config::default()
    });
    let cfg = Arc::new(RwLock::new(initial_config));

    // 4. Cross-thread channel: main → Tokio (commands).
    //    tokio::sync::mpsc works here: the main (winit) thread uses blocking_send,
    //    the Tokio pipeline uses .recv().await.
    let (app_cmd_tx, app_cmd_rx) = tokio::sync::mpsc::channel::<AppCommand>(8);

    // 5. Build winit event loop on the main thread (macOS requirement).
    let event_loop = build_event_loop();
    let proxy = event_loop.create_proxy();

    // 5b. Spawn a file-watcher thread to pick up config.toml changes written by the prefs app.
    // Uses notify's RecommendedWatcher (FSEvents on macOS). Sends AppCommand::ReloadConfig
    // to the pipeline so debounce_ms is updated without requiring a restart.
    let watcher_cmd_tx = app_cmd_tx.clone();
    let _watcher_thread = std::thread::spawn(move || {
        use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
        use std::sync::mpsc::channel;

        let config_path = match relay::config::config_path() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("cannot watch config: failed to resolve config path: {e}");
                return;
            }
        };

        let watch_dir = match config_path.parent() {
            Some(d) => d.to_owned(),
            None => {
                tracing::warn!("config path has no parent directory; watcher not started");
                return;
            }
        };

        let (tx, rx) = channel();
        let mut watcher = match RecommendedWatcher::new(tx, NotifyConfig::default()) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("failed to create config file watcher: {e}");
                return;
            }
        };

        if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
            tracing::warn!("failed to watch config directory: {e}");
            return;
        }

        tracing::info!("watching config directory for changes");

        for result in &rx {
            match result {
                Ok(event) => {
                    let is_config = event.paths.iter().any(|p| p == &config_path);
                    if !is_config {
                        continue;
                    }
                    let reloaded = match relay::config::load() {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::warn!("config reload failed: {e}");
                            continue;
                        }
                    };
                    tracing::info!("config reloaded from disk");
                    let debounce_ms = reloaded.playback.debounce_ms;
                    let _ = watcher_cmd_tx
                        .blocking_send(relay::pipeline::AppCommand::ReloadConfig { debounce_ms });
                }
                Err(e) => {
                    tracing::warn!("config watcher error: {e}");
                }
            }
        }
    });

    // 6. Spawn the Tokio runtime on a dedicated OS thread so it never blocks the main thread.
    let cfg_pipeline = cfg.clone();
    let _tokio_thread = std::thread::spawn(move || {
        // multi_thread scheduler: work-stealing pool.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            // Panic is acceptable at startup if the runtime cannot be created.
            .expect("failed to create tokio runtime");

        rt.block_on(async move {
            run_pipeline(proxy, app_cmd_rx, cfg_pipeline).await;
        });
    });

    // 7. Run the winit event loop on the main thread (blocks until exit).
    run_event_loop(event_loop, app_cmd_tx, cfg)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Winit event loop (main thread)
// ---------------------------------------------------------------------------

fn run_event_loop(
    event_loop: winit::event_loop::EventLoop<UserEvent>,
    app_cmd_tx: tokio::sync::mpsc::Sender<AppCommand>,
    cfg: Arc<RwLock<Config>>,
) -> anyhow::Result<()> {
    use relay::constants::TRAY_POLL_INTERVAL_MS;
    use winit::event_loop::ControlFlow;

    // WaitUntil so about_to_wait is called at ~60 fps without busy-spinning.
    event_loop.set_control_flow(ControlFlow::WaitUntil(
        std::time::Instant::now() + std::time::Duration::from_millis(TRAY_POLL_INTERVAL_MS),
    ));

    let mut app = RelayApp::new(app_cmd_tx, cfg);

    event_loop
        .run_app(&mut app)
        .map_err(|e| anyhow::anyhow!("winit event loop error: {e}"))?;

    Ok(())
}
