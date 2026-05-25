pub const DISCORD_CLIENT_ID: &str = "1508293679427616768";
pub const TRACK_DEBOUNCE_MS: u64 = 1500;
pub const ARTWORK_CACHE_TTL_DAYS: u64 = 30;
pub const DISCORD_RETRY_BASE_MS: u64 = 1000;
pub const DISCORD_RETRY_MAX_MS: u64 = 30_000;
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
