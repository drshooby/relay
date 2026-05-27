// TrayIcon is created inside `resumed()` (macOS requirement — must be on main thread after
// the event loop has started running).
// State updates arrive via EventLoopProxy<UserEvent> from the Tokio background thread.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tray_icon::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
};

use crate::config::Config;
use crate::constants::{
    SYSTEM_SETTINGS_AUTOMATION_URL, TRAY_OPEN_SETTINGS_LABEL, TRAY_PREFERENCES_LABEL,
};
use crate::tray::icons::{self, TrayIconVariant};
use crate::tray::{HelperHealth, TrayStatus};

#[derive(Debug, Clone)]
pub enum UserEvent {
    StatusUpdate(TrayStatus),
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

    // Status row items.
    playback_item: Option<MenuItem>,
    discord_item: Option<MenuItem>,
    helper_item: Option<MenuItem>,
    /// "Open System Settings…" — present only when helper is PermissionDenied.
    /// When the health state transitions to/from PermissionDenied the entire
    /// menu is rebuilt because tray-icon 0.19 does not expose set_visible on
    /// MenuItem.
    settings_item: Option<MenuItem>,
    quit_item: Option<MenuItem>,

    last_icon_variant: Option<TrayIconVariant>,

    prefs_item: Option<MenuItem>,

    /// Tracks whether the settings item is currently present in the menu.
    settings_visible: bool,
}

impl RelayApp {
    pub fn new(
        app_cmd_tx: tokio::sync::mpsc::Sender<crate::AppCommand>,
        _cfg: Arc<RwLock<Config>>,
    ) -> Self {
        Self {
            app_cmd_tx,
            tray_icon: None,
            playback_item: None,
            discord_item: None,
            helper_item: None,
            settings_item: None,
            quit_item: None,
            last_icon_variant: None,
            prefs_item: None,
            settings_visible: false,
        }
    }

    fn build_tray(&mut self) {
        self.rebuild_menu(false);
    }

    /// Rebuild the tray menu. `with_settings` controls whether the
    /// "Open System Settings…" item is included.
    ///
    /// tray-icon 0.19 does not expose `set_visible` on `MenuItem`, so we
    /// rebuild the entire menu whenever the PermissionDenied state changes.
    fn rebuild_menu(&mut self, with_settings: bool) {
        // Status rows — all start disabled (cosmetic display only).
        let initial_status = TrayStatus::new();
        let playback_text = self
            .playback_item
            .as_ref()
            .map(|i| i.text())
            .unwrap_or_else(|| initial_status.playback.row_text());
        let discord_text = self
            .discord_item
            .as_ref()
            .map(|i| i.text())
            .unwrap_or_else(|| initial_status.discord.row_text());
        let helper_text = self
            .helper_item
            .as_ref()
            .map(|i| i.text())
            .unwrap_or_else(|| initial_status.helper.row_text());

        let playback_item = MenuItem::new(playback_text, false, None);
        let discord_item = MenuItem::new(discord_text, false, None);
        let helper_item = MenuItem::new(helper_text, false, None);

        let prefs_item = MenuItem::new(TRAY_PREFERENCES_LABEL, true, None);
        let quit_item = MenuItem::new("Quit Relay", true, None);

        let sep = || PredefinedMenuItem::separator();

        let menu = if with_settings {
            let settings_item = MenuItem::new(TRAY_OPEN_SETTINGS_LABEL, true, None);
            let m = Menu::with_items(&[
                &playback_item,
                &sep(),
                &discord_item,
                &helper_item,
                &sep(),
                &settings_item,
                &sep(),
                &prefs_item,
                &sep(),
                &quit_item,
            ])
            .expect("failed to build tray menu");
            self.settings_item = Some(settings_item);
            m
        } else {
            let m = Menu::with_items(&[
                &playback_item,
                &sep(),
                &discord_item,
                &helper_item,
                &sep(),
                &prefs_item,
                &sep(),
                &quit_item,
            ])
            .expect("failed to build tray menu");
            self.settings_item = None;
            m
        };

        self.quit_item = Some(quit_item);
        self.prefs_item = Some(prefs_item);

        if let Some(tray) = &self.tray_icon {
            tray.set_menu(Some(Box::new(menu)));
        } else {
            // Initial build — create the tray icon.
            let icon = icons::default_icon();
            let tray = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_icon(icon)
                .with_tooltip("Relay")
                .build()
                .expect("failed to build tray icon");
            tray.set_icon_as_template(true);
            self.tray_icon = Some(tray);
            self.last_icon_variant = Some(TrayIconVariant::Normal);
        }

        self.playback_item = Some(playback_item);
        self.discord_item = Some(discord_item);
        self.helper_item = Some(helper_item);
        self.settings_visible = with_settings;
    }

    fn apply_status(&mut self, status: &TrayStatus) {
        let permission_denied = status.helper == HelperHealth::PermissionDenied;

        // Rebuild menu if PermissionDenied state has changed so that the
        // "Open System Settings…" item appears/disappears cleanly.
        // tray-icon 0.19 has no set_visible, so we rebuild the whole menu.
        if permission_denied != self.settings_visible {
            self.rebuild_menu(permission_denied);
        }

        if let Some(item) = &self.playback_item {
            item.set_text(status.playback.row_text());
        }

        if let Some(item) = &self.discord_item {
            item.set_text(status.discord.row_text());
        }

        if let Some(item) = &self.helper_item {
            item.set_text(status.helper.row_text());
        }

        // Update icon variant.
        let variant = status.icon_variant();
        if self.last_icon_variant != Some(variant) {
            if let Some(tray) = &self.tray_icon {
                let icon = status.icon();
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

        // "Preferences…" click — open the bundled SwiftUI prefs app.
        if self.prefs_item.as_ref().is_some_and(|i| event.id == i.id()) {
            use crate::media::prefs_path::resolve_prefs_path;
            let prefs_path = resolve_prefs_path();
            if let Err(e) = std::process::Command::new("open").arg(&prefs_path).status() {
                tracing::warn!("failed to open preferences app: {e}");
            }
            return;
        }

        // "Open System Settings…" click.
        if self
            .settings_item
            .as_ref()
            .is_some_and(|i| event.id == i.id())
        {
            if let Err(e) = std::process::Command::new("open")
                .arg(SYSTEM_SETTINGS_AUTOMATION_URL)
                .status()
            {
                tracing::warn!("failed to open system settings: {e}");
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
            UserEvent::StatusUpdate(status) => {
                self.apply_status(&status);
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
