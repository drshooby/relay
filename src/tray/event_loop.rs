// TrayIcon is created inside `resumed()` (macOS requirement — must be on main thread after
// the event loop has started running).
// State updates arrive via EventLoopProxy<UserEvent> from the Tokio background thread.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuItem, Submenu},
    TrayIcon, TrayIconBuilder,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
};

use crate::config::Config;
use crate::constants::{
    TRAY_DISPLAY_ALBUM_LABEL, TRAY_DISPLAY_ARTIST_LABEL, TRAY_DISPLAY_ARTWORK_LABEL,
    TRAY_DISPLAY_SUBMENU_LABEL, TRAY_DISPLAY_TITLE_LABEL,
};
use crate::pipeline::DisplayField;
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

    /// Shared config — used to read initial display state when building the tray.
    cfg: Arc<RwLock<Config>>,

    tray_icon: Option<TrayIcon>,
    status_item: Option<MenuItem>,
    details_item: Option<MenuItem>,
    quit_item: Option<MenuItem>,
    last_icon_variant: Option<TrayIconVariant>,

    // Display submenu toggles.
    display_title_item: Option<CheckMenuItem>,
    display_artist_item: Option<CheckMenuItem>,
    display_album_item: Option<CheckMenuItem>,
    display_artwork_item: Option<CheckMenuItem>,
}

impl RelayApp {
    pub fn new(
        app_cmd_tx: tokio::sync::mpsc::Sender<crate::AppCommand>,
        cfg: Arc<RwLock<Config>>,
    ) -> Self {
        Self {
            app_cmd_tx,
            cfg,
            tray_icon: None,
            status_item: None,
            details_item: None,
            quit_item: None,
            last_icon_variant: None,
            display_title_item: None,
            display_artist_item: None,
            display_album_item: None,
            display_artwork_item: None,
        }
    }

    fn build_tray(&mut self) {
        // Read the current display config (blocking — main thread, pre-loop-start).
        let display = self.cfg.blocking_read().display.clone();

        let status_item = MenuItem::new(TrayState::Idle.label(), true, None);
        let details_item = MenuItem::new("", false, None);

        // Display submenu with 4 checkable toggles.
        let display_title_item =
            CheckMenuItem::new(TRAY_DISPLAY_TITLE_LABEL, true, display.show_title, None);
        let display_artist_item =
            CheckMenuItem::new(TRAY_DISPLAY_ARTIST_LABEL, true, display.show_artist, None);
        let display_album_item =
            CheckMenuItem::new(TRAY_DISPLAY_ALBUM_LABEL, true, display.show_album, None);
        let display_artwork_item =
            CheckMenuItem::new(TRAY_DISPLAY_ARTWORK_LABEL, true, display.show_artwork, None);
        let display_submenu = Submenu::with_items(
            TRAY_DISPLAY_SUBMENU_LABEL,
            true,
            &[
                &display_title_item,
                &display_artist_item,
                &display_album_item,
                &display_artwork_item,
            ],
        )
        .expect("failed to build display submenu");

        let quit_item = MenuItem::new("Quit Relay", true, None);

        let menu = Menu::with_items(&[&status_item, &details_item, &display_submenu, &quit_item])
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
        self.display_title_item = Some(display_title_item);
        self.display_artist_item = Some(display_artist_item);
        self.display_album_item = Some(display_album_item);
        self.display_artwork_item = Some(display_artwork_item);
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
        if self.quit_item.as_ref().is_some_and(|i| event.id == i.id()) {
            tracing::info!("quit requested via menu");
            let _ = self.app_cmd_tx.blocking_send(crate::AppCommand::Quit);
            return;
        }

        // Display toggle handlers: read the new checked state and forward to pipeline.
        let display_toggles: &[(Option<&CheckMenuItem>, DisplayField)] = &[
            (self.display_title_item.as_ref(), DisplayField::Title),
            (self.display_artist_item.as_ref(), DisplayField::Artist),
            (self.display_album_item.as_ref(), DisplayField::Album),
            (self.display_artwork_item.as_ref(), DisplayField::Artwork),
        ];
        for (item_opt, field) in display_toggles {
            if let Some(item) = item_opt {
                if event.id == item.id() {
                    let enabled = item.is_checked();
                    tracing::debug!("display toggle {:?} -> {enabled}", field);
                    let _ = self
                        .app_cmd_tx
                        .blocking_send(crate::AppCommand::SetDisplayField {
                            field: *field,
                            enabled,
                        });
                    return;
                }
            }
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
