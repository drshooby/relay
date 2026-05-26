use crate::constants::{
    TRAY_DISCORD_LABEL_CONNECTED, TRAY_DISCORD_LABEL_DISCONNECTED,
    TRAY_DISCORD_LABEL_PREFIX_RECONNECTING, TRAY_HELPER_LABEL_PERMISSION_DENIED,
    TRAY_HELPER_LABEL_RUNNING, TRAY_HELPER_LABEL_UNAVAILABLE_PREFIX, TRAY_LAST_ERROR_PREFIX,
    TRAY_PLAYBACK_IDLE_LABEL, TRAY_PLAYBACK_PAUSED_PREFIX, TRAY_PLAYBACK_PLAYING_PREFIX,
};

pub mod event_loop;
pub mod icons;

/// Current playback state shown in the top row of the tray menu.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PlaybackStatus {
    #[default]
    Idle,
    Playing {
        title: String,
        artist: String,
    },
    Paused {
        title: String,
        artist: String,
    },
}

impl PlaybackStatus {
    pub fn row_text(&self) -> String {
        match self {
            PlaybackStatus::Idle => TRAY_PLAYBACK_IDLE_LABEL.to_string(),
            PlaybackStatus::Playing { title, artist } => {
                format!("{}{} \u{2014} {}", TRAY_PLAYBACK_PLAYING_PREFIX, title, artist)
            }
            PlaybackStatus::Paused { title, artist } => {
                format!(
                    "{}{} \u{2014} {}",
                    TRAY_PLAYBACK_PAUSED_PREFIX, title, artist
                )
            }
        }
    }
}

/// Health of the Discord RPC connection.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum DiscordHealth {
    #[default]
    Connected,
    Reconnecting {
        backoff_ms: u64,
    },
    Disconnected {
        detail: String,
    },
}

impl DiscordHealth {
    pub fn row_text(&self) -> String {
        match self {
            DiscordHealth::Connected => TRAY_DISCORD_LABEL_CONNECTED.to_string(),
            DiscordHealth::Reconnecting { backoff_ms } => {
                format!("{}{}ms", TRAY_DISCORD_LABEL_PREFIX_RECONNECTING, backoff_ms)
            }
            DiscordHealth::Disconnected { .. } => TRAY_DISCORD_LABEL_DISCONNECTED.to_string(),
        }
    }

    pub fn is_healthy(&self) -> bool {
        matches!(self, DiscordHealth::Connected)
    }
}

/// Health of the Swift helper process and Apple Music access.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum HelperHealth {
    #[default]
    Running,
    Unavailable {
        detail: String,
    },
    PermissionDenied,
}

impl HelperHealth {
    pub fn row_text(&self) -> String {
        match self {
            HelperHealth::Running => TRAY_HELPER_LABEL_RUNNING.to_string(),
            HelperHealth::Unavailable { detail } => {
                format!("{}{}", TRAY_HELPER_LABEL_UNAVAILABLE_PREFIX, detail)
            }
            HelperHealth::PermissionDenied => TRAY_HELPER_LABEL_PERMISSION_DENIED.to_string(),
        }
    }
}

/// Complete tray status: all fields that drive the status dashboard menu rows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrayStatus {
    pub playback: PlaybackStatus,
    pub discord: DiscordHealth,
    pub helper: HelperHealth,
    /// Most recent transient error message. Shown in the "Last error" row when set.
    pub last_error: Option<String>,
    /// Whether Discord has ever successfully connected in this session.
    /// Used to suppress "Disconnected" noise before the first connection.
    pub discord_was_connected: bool,
}

impl TrayStatus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn last_error_row_text(&self) -> Option<String> {
        self.last_error
            .as_ref()
            .map(|e| format!("{}{}", TRAY_LAST_ERROR_PREFIX, e))
    }

    /// Returns `Error` when any health dimension is degraded; otherwise `Normal`.
    pub fn icon_variant(&self) -> icons::TrayIconVariant {
        if self.last_error.is_some() {
            return icons::TrayIconVariant::Error;
        }
        if self.helper == HelperHealth::PermissionDenied {
            return icons::TrayIconVariant::Error;
        }
        // Only flag Discord disconnect after first successful connection.
        if self.discord_was_connected && !self.discord.is_healthy() {
            return icons::TrayIconVariant::Error;
        }
        icons::TrayIconVariant::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{
        TRAY_DISCORD_LABEL_CONNECTED, TRAY_HELPER_LABEL_RUNNING, TRAY_PLAYBACK_IDLE_LABEL,
    };

    // --- PlaybackStatus row_text ---

    #[test]
    fn playback_idle_row_text() {
        assert_eq!(PlaybackStatus::Idle.row_text(), TRAY_PLAYBACK_IDLE_LABEL);
    }

    #[test]
    fn playback_playing_row_text() {
        let s = PlaybackStatus::Playing {
            title: "Song".into(),
            artist: "Artist".into(),
        };
        assert_eq!(s.row_text(), "Now Playing: Song \u{2014} Artist");
    }

    #[test]
    fn playback_paused_row_text() {
        let s = PlaybackStatus::Paused {
            title: "Song".into(),
            artist: "Artist".into(),
        };
        assert_eq!(s.row_text(), "Paused \u{2014} Song \u{2014} Artist");
    }

    // --- DiscordHealth row_text ---

    #[test]
    fn discord_connected_row_text() {
        assert_eq!(
            DiscordHealth::Connected.row_text(),
            TRAY_DISCORD_LABEL_CONNECTED
        );
    }

    #[test]
    fn discord_reconnecting_row_text() {
        let s = DiscordHealth::Reconnecting { backoff_ms: 1200 };
        assert_eq!(s.row_text(), "Discord: Reconnecting in 1200ms");
    }

    #[test]
    fn discord_disconnected_row_text() {
        let s = DiscordHealth::Disconnected {
            detail: "pipe closed".into(),
        };
        assert_eq!(s.row_text(), "Discord: Disconnected");
    }

    // --- HelperHealth row_text ---

    #[test]
    fn helper_running_row_text() {
        assert_eq!(HelperHealth::Running.row_text(), TRAY_HELPER_LABEL_RUNNING);
    }

    #[test]
    fn helper_unavailable_row_text() {
        let s = HelperHealth::Unavailable {
            detail: "exited with code 1".into(),
        };
        assert_eq!(s.row_text(), "Helper: Unavailable \u{2014} exited with code 1");
    }

    #[test]
    fn helper_permission_denied_row_text() {
        assert_eq!(
            HelperHealth::PermissionDenied.row_text(),
            "Helper: Apple Music access denied"
        );
    }

    // --- TrayStatus::icon_variant ---

    #[test]
    fn icon_variant_normal_when_healthy() {
        let status = TrayStatus {
            discord_was_connected: true,
            ..TrayStatus::default()
        };
        assert_eq!(status.icon_variant(), icons::TrayIconVariant::Normal);
    }

    #[test]
    fn icon_variant_error_when_last_error_set() {
        let status = TrayStatus {
            last_error: Some("something broke".into()),
            ..TrayStatus::default()
        };
        assert_eq!(status.icon_variant(), icons::TrayIconVariant::Error);
    }

    #[test]
    fn icon_variant_error_when_permission_denied() {
        let status = TrayStatus {
            helper: HelperHealth::PermissionDenied,
            ..TrayStatus::default()
        };
        assert_eq!(status.icon_variant(), icons::TrayIconVariant::Error);
    }

    #[test]
    fn icon_variant_error_after_discord_disconnect() {
        let status = TrayStatus {
            discord: DiscordHealth::Disconnected {
                detail: "lost".into(),
            },
            discord_was_connected: true,
            ..TrayStatus::default()
        };
        assert_eq!(status.icon_variant(), icons::TrayIconVariant::Error);
    }

    #[test]
    fn icon_variant_normal_discord_disconnect_before_first_connect() {
        // No error before first connection — suppress initial disconnect noise.
        let status = TrayStatus {
            discord: DiscordHealth::Disconnected {
                detail: "not yet".into(),
            },
            discord_was_connected: false,
            ..TrayStatus::default()
        };
        assert_eq!(status.icon_variant(), icons::TrayIconVariant::Normal);
    }

    // --- last_error_row_text ---

    #[test]
    fn last_error_row_text_none_when_no_error() {
        let status = TrayStatus::default();
        assert!(status.last_error_row_text().is_none());
    }

    #[test]
    fn last_error_row_text_prefixed() {
        let status = TrayStatus {
            last_error: Some("helper crashed".into()),
            ..TrayStatus::default()
        };
        assert_eq!(
            status.last_error_row_text(),
            Some("Last error: helper crashed".into())
        );
    }
}
