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
