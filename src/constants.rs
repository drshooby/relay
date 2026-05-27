pub const DISCORD_CLIENT_ID: &str = "1508293679427616768";
pub const TRACK_DEBOUNCE_MS: u64 = 1500;
pub const CHANNEL_BUFFER_SIZE: usize = 32;
pub const TRAY_POLL_INTERVAL_MS: u64 = 16;
pub const ARTWORK_CACHE_TTL_DAYS: u64 = 30;
pub const DISCORD_RETRY_BASE_MS: u64 = 1000;
pub const DISCORD_RETRY_MAX_MS: u64 = 30_000;
/// Delay between Discord IPC connect() and the first activity write.
/// connect() returns after sending the handshake but before Discord's daemon is
/// ready to process commands; without this delay, the first set_activity after
/// reconnect is acknowledged on the wire but not displayed.
pub const DISCORD_POST_CONNECT_DELAY_MS: u64 = 1000;
/// Interval for re-sending the current activity to detect a dead IPC socket.
/// Discord's IPC has no peer-disconnect callback, so we only learn the socket is
/// dead by trying to write. Re-sending the same activity is idempotent (no UI
/// flicker) and keeps recovery latency bounded after Discord is closed/reopened.
pub const DISCORD_HEARTBEAT_INTERVAL_MS: u64 = 15_000;
pub const ITUNES_SEARCH_URL: &str = "https://itunes.apple.com/search";
pub const ITUNES_SEARCH_LIMIT: u32 = 5;
pub const ITUNES_ARTWORK_SIZE_SMALL: &str = "100x100";
pub const ITUNES_ARTWORK_SIZE_LARGE: &str = "600x600";
pub const HELPER_BINARY_NAME: &str = "relay-helper";
pub const DISCORD_BUTTON_LISTEN_LABEL: &str = "Listen to this song";
/// Shown in the profile card header as "Listening to {name}" (overrides the Relay app name).
pub const DISCORD_ACTIVITY_NAME: &str = "Apple Music";
/// Rich Presence art asset key for the Relay badge on album art (upload assets/icons/relay-discord.png in Discord Developer Portal → Rich Presence → Art Assets).
/// Also used as the large image when iTunes artwork is unavailable.
pub const DISCORD_ASSET_RELAY_BADGE: &str = "relay-discord";
pub const DISCORD_ASSET_RELAY_BADGE_TEXT: &str = "relay";
/// Minimum elapsed-time delta (seconds) before the helper emits position_changed.
pub const POSITION_CHANGE_THRESHOLD_SECS: u64 = 3;

/// Shown in the Discord card details line when `show_title` is disabled.
pub const DISCORD_PRIVATE_TITLE: &str = "Listening to music";

pub const TRAY_DISPLAY_SUBMENU_LABEL: &str = "Display";
pub const TRAY_DISPLAY_TITLE_LABEL: &str = "Show Title";
pub const TRAY_DISPLAY_ARTIST_LABEL: &str = "Show Artist";
pub const TRAY_DISPLAY_ALBUM_LABEL: &str = "Show Album";
pub const TRAY_DISPLAY_ARTWORK_LABEL: &str = "Show Artwork";

pub const TRAY_ICON_RELAY: &[u8] = include_bytes!("../assets/icons/relay.png");
/// Alpha multiplier for the error-state tray icon (0–255). Template icons use alpha as the mask,
/// so dimming reads as grayed out in the menu bar.
pub const TRAY_ICON_ERROR_ALPHA: u8 = 128;

pub const TRAY_ERROR_HELPER_MESSAGE: &str = "media access unavailable";
pub const TRAY_ERROR_DISCORD_MESSAGE: &str = "discord unavailable";
pub const TRAY_ERROR_DISCORD_DISCONNECTED_DETAIL: &str = "discord ipc: disconnected";

// Status dashboard row labels
pub const TRAY_DISCORD_LABEL_CONNECTED: &str = "Discord: Connected";
pub const TRAY_DISCORD_LABEL_PREFIX_RECONNECTING: &str = "Discord: Reconnecting in ";
pub const TRAY_DISCORD_LABEL_DISCONNECTED: &str = "Discord: Disconnected";
pub const TRAY_HELPER_LABEL_RUNNING: &str = "Helper: Running";
pub const TRAY_HELPER_LABEL_PERMISSION_DENIED: &str = "Helper: Apple Music access denied";
pub const TRAY_HELPER_LABEL_UNAVAILABLE_PREFIX: &str = "Helper: Unavailable \u{2014} ";
pub const TRAY_LAST_ERROR_PREFIX: &str = "Last error: ";
pub const TRAY_OPEN_SETTINGS_LABEL: &str = "Open System Settings\u{2026}";
pub const TRAY_PERMISSION_DENIED_DETAIL: &str = "Apple Music access denied";
pub const SYSTEM_SETTINGS_AUTOMATION_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation";
pub const FIRST_RUN_NOTIFICATION_OSASCRIPT: &str =
    "display notification \"macOS will ask for permission to read Apple Music. Click OK to enable Relay.\" with title \"Relay\"";
pub const TRAY_PLAYBACK_IDLE_LABEL: &str = "Now Playing: Idle";
pub const TRAY_PLAYBACK_PLAYING_PREFIX: &str = "Now Playing: ";
pub const TRAY_PLAYBACK_PAUSED_PREFIX: &str = "Paused \u{2014} ";

pub const TRAY_PREFERENCES_LABEL: &str = "Preferences\u{2026}";
pub const PREFS_APP_NAME: &str = "RelayPreferences.app";
pub const NOWPLAYING_SNAPSHOT_FILE: &str = "nowplaying.json";
pub const DISCORD_ACTIVITY_HELP_URL: &str =
    "https://support.discord.com/hc/en-us/articles/115000076487";
/// Filename for the artwork cache under the relay application support directory.
pub const ARTWORK_CACHE_SUBDIR: &str = "artwork_cache.json";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discord_client_id_matches_app_id_file() {
        let app_id = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("APP_ID"),
        )
        .expect("APP_ID file should exist at repo root");
        assert_eq!(DISCORD_CLIENT_ID, app_id.trim());
    }
}
