use tracing_subscriber::EnvFilter;

use relay::pipeline::{run_pipeline, AppCommand};
use relay::tray::event_loop::{build_event_loop, RelayApp, UserEvent};

fn main() -> anyhow::Result<()> {
    // 1. Initialise tracing — reads RUST_LOG from environment.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // 2. Cross-thread channel: main → Tokio (commands).
    //    tokio::sync::mpsc works here: the main (winit) thread uses blocking_send,
    //    the Tokio pipeline uses .recv().await.
    let (app_cmd_tx, app_cmd_rx) = tokio::sync::mpsc::channel::<AppCommand>(8);

    // 3. Build winit event loop on the main thread (macOS requirement).
    let event_loop = build_event_loop();
    let proxy = event_loop.create_proxy();

    // 4. Spawn the Tokio runtime on a dedicated OS thread so it never blocks the main thread.
    let _tokio_thread = std::thread::spawn(move || {
        // multi_thread scheduler: work-stealing pool.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            // Panic is acceptable at startup if the runtime cannot be created.
            .expect("failed to create tokio runtime");

        rt.block_on(async move {
            run_pipeline(proxy, app_cmd_rx).await;
        });
    });

    // 5. Run the winit event loop on the main thread (blocks until exit).
    run_event_loop(event_loop, app_cmd_tx)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Winit event loop (main thread)
// ---------------------------------------------------------------------------

fn run_event_loop(
    event_loop: winit::event_loop::EventLoop<UserEvent>,
    app_cmd_tx: tokio::sync::mpsc::Sender<AppCommand>,
) -> anyhow::Result<()> {
    use relay::constants::TRAY_POLL_INTERVAL_MS;
    use winit::event_loop::ControlFlow;

    // WaitUntil so about_to_wait is called at ~60 fps without busy-spinning.
    event_loop.set_control_flow(ControlFlow::WaitUntil(
        std::time::Instant::now() + std::time::Duration::from_millis(TRAY_POLL_INTERVAL_MS),
    ));

    let mut app = RelayApp::new(app_cmd_tx);

    event_loop
        .run_app(&mut app)
        .map_err(|e| anyhow::anyhow!("winit event loop error: {e}"))?;

    Ok(())
}
