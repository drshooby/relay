// Task 12: winit event loop integration.
// TrayIcon is created inside `resumed()` (macOS requirement — must be on main thread after
// the event loop has started running).
// State updates arrive via EventLoopProxy<UserEvent> from the Tokio background thread.

use std::time::{Duration, Instant};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuItem},
    TrayIcon, TrayIconBuilder,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
};

use crate::tray::TrayState;

#[derive(Debug, Clone)]
pub enum UserEvent {
    StateUpdate(TrayState),
    TrayIconEvent(tray_icon::TrayIconEvent),
    MenuEvent(tray_icon::menu::MenuEvent),
}

pub fn build_event_loop() -> EventLoop<UserEvent> {
    EventLoop::<UserEvent>::with_user_event()
        .build()
        // App startup — panic is acceptable; there is no way to proceed without an event loop.
        .expect("failed to create winit event loop")
}

// ---------------------------------------------------------------------------
// App state held on the main thread
// ---------------------------------------------------------------------------

pub struct RelayApp {
    /// Command sender for the Tokio pipeline.
    app_cmd_tx: tokio::sync::mpsc::Sender<crate::AppCommand>,

    // Tray icon — None until `resumed` is called the first time.
    tray_icon: Option<TrayIcon>,

    // Menu items we need to update after construction.
    status_item: Option<MenuItem>,
    enabled_item: Option<CheckMenuItem>,
    quit_item: Option<MenuItem>,
}

impl RelayApp {
    pub fn new(app_cmd_tx: tokio::sync::mpsc::Sender<crate::AppCommand>) -> Self {
        Self {
            app_cmd_tx,
            tray_icon: None,
            status_item: None,
            enabled_item: None,
            quit_item: None,
        }
    }

    fn build_tray(&mut self) {
        // Build menu items.
        let status_item = MenuItem::new(TrayState::Idle.label(), false, None);
        let enabled_item = CheckMenuItem::new("Enabled", true, true, None);
        let quit_item = MenuItem::new("Quit Relay", true, None);

        let menu = Menu::with_items(&[&status_item, &enabled_item, &quit_item])
            // Fatal at startup — menu is required.
            .expect("failed to build tray menu");

        // Minimal 1×1 transparent icon — a real icon can be added as a PNG asset later.
        let icon = tray_icon::Icon::from_rgba(vec![0u8, 0, 0, 0], 1, 1)
            .expect("failed to create tray icon");

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .with_tooltip("Relay")
            .build()
            // Fatal at startup — without a tray icon the app has no UI.
            .expect("failed to build tray icon");

        self.status_item = Some(status_item);
        self.enabled_item = Some(enabled_item);
        self.quit_item = Some(quit_item);
        self.tray_icon = Some(tray);
    }

    fn apply_state(&self, state: &TrayState) {
        if let Some(item) = &self.status_item {
            item.set_text(state.label());
        }
    }

    fn handle_menu_event(&self, event: &tray_icon::menu::MenuEvent) {
        let enabled_id = self.enabled_item.as_ref().map(|i| i.id().clone());
        let quit_id = self.quit_item.as_ref().map(|i| i.id().clone());

        if Some(&event.id) == quit_id.as_ref() {
            tracing::info!("quit requested via menu");
            let _ = self.app_cmd_tx.blocking_send(crate::AppCommand::Quit);
        } else if Some(&event.id) == enabled_id.as_ref() {
            // CheckMenuItem toggles itself — read the *new* state after the click.
            let now_checked = self
                .enabled_item
                .as_ref()
                .map(|i| i.is_checked())
                .unwrap_or(false);
            tracing::info!("enabled toggled to {now_checked}");
            let _ = self
                .app_cmd_tx
                .blocking_send(crate::AppCommand::SetEnabled(now_checked));
        }
    }
}

impl ApplicationHandler<UserEvent> for RelayApp {
    /// Called once the event loop is ready — create the tray icon here so it is on the main thread
    /// and after the platform event loop has initialised (required on macOS).
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.tray_icon.is_none() {
            self.build_tray();
        }
    }

    /// Required by the trait — we create no windows, so this is a no-op.
    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        _event: WindowEvent,
    ) {
    }

    /// Receives cross-thread events sent via `EventLoopProxy::send_event`.
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::StateUpdate(state) => {
                self.apply_state(&state);
            }
            UserEvent::TrayIconEvent(_tray_event) => {
                // Left-click on the tray icon — the menu opens automatically; nothing else needed.
            }
            UserEvent::MenuEvent(menu_event) => {
                self.handle_menu_event(&menu_event);

                // If we received a Quit command from the menu, exit the event loop.
                // We check by comparing with the quit item id again.
                let quit_id = self.quit_item.as_ref().map(|i| i.id().clone());
                if Some(&menu_event.id) == quit_id.as_ref() {
                    event_loop.exit();
                }
            }
        }
    }

    /// Called each time the event loop is about to block — poll tray/menu channel events here
    /// because tray-icon 0.19 does not integrate directly with winit's wakeup mechanism.
    /// We reschedule a ~16 ms wakeup (~60 fps) to avoid busy-spinning at 100 % CPU.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Drain any pending tray icon events (left-click; menu opens automatically).
        while tray_icon::TrayIconEvent::receiver().try_recv().is_ok() {}

        // Drain any pending menu events.
        while let Ok(ev) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            self.handle_menu_event(&ev);
            let quit_id = self.quit_item.as_ref().map(|i| i.id().clone());
            if Some(&ev.id) == quit_id.as_ref() {
                event_loop.exit();
            }
        }

        // Rate-limit polling to ~60 fps instead of spinning at 100 % CPU.
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
    }
}
