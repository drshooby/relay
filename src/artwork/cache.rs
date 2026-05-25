use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

use crate::constants::ARTWORK_CACHE_TTL_DAYS;

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("could not resolve app data directory")]
    NoDataDir,
    #[error("failed to read cache file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse cache: {0}")]
    Parse(#[from] serde_json::Error),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedArtwork {
    pub url: String,
    pub cached_at_secs: u64, // Unix timestamp seconds
}

impl CachedArtwork {
    fn is_expired(&self) -> bool {
        let ttl = Duration::from_secs(ARTWORK_CACHE_TTL_DAYS * 24 * 60 * 60);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        now.saturating_sub(self.cached_at_secs) > ttl.as_secs()
    }
}

/// Encode (artist, title) into a single string key for JSON-serializable HashMap.
fn make_key(artist: &str, title: &str) -> String {
    // Use a zero-byte separator that is unlikely to appear in artist/title.
    // The key is only used internally and in the on-disk JSON.
    format!("{}\x00{}", artist, title)
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ArtworkCache {
    entries: HashMap<String, CachedArtwork>,
}

impl ArtworkCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the cache file path: ~/Library/Application Support/relay/artwork_cache.json
    pub fn cache_path() -> Result<PathBuf, CacheError> {
        let data_dir = dirs::data_dir().ok_or(CacheError::NoDataDir)?;
        Ok(data_dir.join("relay").join("artwork_cache.json"))
    }

    /// Load from default path. Returns empty cache if file doesn't exist.
    pub fn load() -> Result<Self, CacheError> {
        let path = Self::cache_path()?;
        Self::load_from_path(&path)
    }

    /// Load from a custom path (for testing).
    pub fn load_from_path(path: &Path) -> Result<Self, CacheError> {
        match std::fs::read_to_string(path) {
            Ok(content) => Ok(serde_json::from_str(&content)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::new()),
            Err(e) => Err(CacheError::Io(e)),
        }
    }

    /// Save to default path.
    pub fn save(&self) -> Result<(), CacheError> {
        let path = Self::cache_path()?;
        self.save_to_path(&path)
    }

    /// Save to a custom path (for testing).
    pub fn save_to_path(&self, path: &Path) -> Result<(), CacheError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Look up artwork. Returns None if not cached or if entry is expired (and removes it).
    pub fn get(&mut self, artist: &str, title: &str) -> Option<String> {
        let key = make_key(artist, title);
        if let Some(entry) = self.entries.get(&key) {
            if entry.is_expired() {
                self.entries.remove(&key);
                return None;
            }
            return Some(entry.url.clone());
        }
        None
    }

    /// Store an artwork URL in the cache.
    pub fn insert(&mut self, artist: &str, title: &str, url: String) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        self.entries.insert(
            make_key(artist, title),
            CachedArtwork {
                url,
                cached_at_secs: now,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn in_memory_hit_returns_url() {
        let mut cache = ArtworkCache::new();
        cache.insert(
            "Queen",
            "Bohemian Rhapsody",
            "https://example.com/art.jpg".into(),
        );
        let result = cache.get("Queen", "Bohemian Rhapsody");
        assert_eq!(result, Some("https://example.com/art.jpg".to_string()));
    }

    #[test]
    fn expired_entry_evicted_on_read() {
        let mut cache = ArtworkCache::new();
        // Insert with a very old timestamp (31 days ago)
        let old_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(31 * 24 * 60 * 60);
        cache.entries.insert(
            make_key("Queen", "Bohemian Rhapsody"),
            CachedArtwork {
                url: "https://example.com/art.jpg".into(),
                cached_at_secs: old_timestamp,
            },
        );
        let result = cache.get("Queen", "Bohemian Rhapsody");
        assert!(result.is_none(), "expired entry should return None");
        assert!(cache.entries.is_empty(), "expired entry should be evicted");
    }

    #[test]
    fn disk_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("artwork_cache.json");

        let mut cache = ArtworkCache::new();
        cache.insert(
            "Queen",
            "Bohemian Rhapsody",
            "https://example.com/art.jpg".into(),
        );
        cache.save_to_path(&path).unwrap();

        let mut loaded = ArtworkCache::load_from_path(&path).unwrap();
        let result = loaded.get("Queen", "Bohemian Rhapsody");
        assert_eq!(result, Some("https://example.com/art.jpg".to_string()));
    }
}
