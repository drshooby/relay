// TrayIcon is created inside `resumed()` (macOS requirement — must be on main thread after
// the event loop has started running).
// State updates arrive via EventLoopProxy<UserEvent> from the Tokio background thread.

use std::time::{Duration, Instant};
use tray_icon::{
    menu::{Menu, MenuItem},
    TrayIcon, TrayIconBuilder,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
};

use crate::tray::icons::{self, TrayIconVariant};
use crate::tray::TrayState;

#[derive(Debug, Clone)]
pub enum UserEvent {
    StateUpdate(TrayState),
    TrayIconEvent(tray_icon::TrayIconEvent),
    MenuEvent(tray_icon::menu::MenuEvent),
}

pub fn build_event_loop() -> EventLoop<UserEvent> {
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory);
    }
    builder
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

    tray_icon: Option<TrayIcon>,
    status_item: Option<MenuItem>,
    details_item: Option<MenuItem>,
    quit_item: Option<MenuItem>,
    last_icon_variant: Option<TrayIconVariant>,
}

impl RelayApp {
    pub fn new(app_cmd_tx: tokio::sync::mpsc::Sender<crate::AppCommand>) -> Self {
        Self {
            app_cmd_tx,
            tray_icon: None,
            status_item: None,
            details_item: None,
            quit_item: None,
            last_icon_variant: None,
        }
    }

    fn build_tray(&mut self) {
        let status_item = MenuItem::new(TrayState::Idle.label(), true, None);
        let details_item = MenuItem::new("", false, None);
        let quit_item = MenuItem::new("Quit Relay", true, None);

        let menu = Menu::with_items(&[&status_item, &details_item, &quit_item])
            .expect("failed to build tray menu");

        let icon = icons::default_icon();

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .with_tooltip("Relay")
            .build()
            .expect("failed to build tray icon");

        tray.set_icon_as_template(true);

        self.status_item = Some(status_item);
        self.details_item = Some(details_item);
        self.quit_item = Some(quit_item);
        self.tray_icon = Some(tray);
        self.last_icon_variant = Some(TrayIconVariant::Normal);
    }

    fn apply_state(&mut self, state: &TrayState) {
        if let Some(item) = &self.status_item {
            item.set_text(state.label());
        }

        if let Some(item) = &self.details_item {
            if let Some(detail) = state.error_detail() {
                item.set_text(detail);
                item.set_enabled(true);
            } else {
                item.set_text("");
                item.set_enabled(false);
            }
        }

        let variant = state.icon_variant();
        if self.last_icon_variant != Some(variant) {
            if let Some(tray) = &self.tray_icon {
                let icon = state.icon();
                if let Err(e) = tray.set_icon_with_as_template(Some(icon), true) {
                    tracing::warn!("failed to update tray icon: {e}");
                } else {
                    self.last_icon_variant = Some(variant);
                }
            }
        }
    }

    fn handle_menu_event(&self, event: &tray_icon::menu::MenuEvent) {
        let quit_id = self.quit_item.as_ref().map(|i| i.id().clone());

        if Some(&event.id) == quit_id.as_ref() {
            tracing::info!("quit requested via menu");
            let _ = self.app_cmd_tx.blocking_send(crate::AppCommand::Quit);
        }
    }

    fn is_quit_menu_event(&self, event: &tray_icon::menu::MenuEvent) -> bool {
        self.quit_item
            .as_ref()
            .is_some_and(|item| event.id == item.id())
    }

    fn dispatch_menu_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        event: &tray_icon::menu::MenuEvent,
    ) {
        self.handle_menu_event(event);
        if self.is_quit_menu_event(event) {
            event_loop.exit();
        }
    }
}

impl ApplicationHandler<UserEvent> for RelayApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.tray_icon.is_none() {
            self.build_tray();
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        _event: WindowEvent,
    ) {
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::StateUpdate(state) => {
                self.apply_state(&state);
            }
            // tray-icon 0.19 does not integrate with winit; left-click opens the menu automatically.
            UserEvent::TrayIconEvent(_tray_event) => {}
            UserEvent::MenuEvent(menu_event) => {
                self.dispatch_menu_event(event_loop, &menu_event);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        while tray_icon::TrayIconEvent::receiver().try_recv().is_ok() {}

        while let Ok(ev) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            self.dispatch_menu_event(event_loop, &ev);
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(crate::constants::TRAY_POLL_INTERVAL_MS),
        ));
    }
}
