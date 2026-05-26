use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Per-field Discord display toggles. Defaults to all enabled.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(default)]
pub struct DisplayConfig {
    pub show_title: bool,
    pub show_artist: bool,
    pub show_album: bool,
    pub show_artwork: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            show_title: true,
            show_artist: true,
            show_album: true,
            show_artwork: true,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct Config {
    #[serde(default)]
    pub display: DisplayConfig,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not resolve application support directory")]
    NoAppDir,
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config file: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Returns path to config file: ~/Library/Application Support/relay/config.toml
pub fn config_path() -> Result<PathBuf, ConfigError> {
    let app_dir = dirs::data_dir().ok_or(ConfigError::NoAppDir)?;
    Ok(app_dir.join("relay").join("config.toml"))
}

/// Test-only: load from custom directory
pub fn load_from_dir(dir: &Path) -> Result<Config, ConfigError> {
    let config_file = dir.join("config.toml");
    match std::fs::read_to_string(&config_file) {
        Ok(content) => {
            let config = toml::from_str(&content)?;
            Ok(config)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(ConfigError::Io(e)),
    }
}

/// Test-only: save to custom directory
pub fn save_to_dir(config: &Config, dir: &Path) -> Result<(), ConfigError> {
    std::fs::create_dir_all(dir)?;
    let config_file = dir.join("config.toml");
    let content = toml::to_string_pretty(config)?;
    std::fs::write(config_file, content)?;
    Ok(())
}

/// Loads config from disk. If file missing, returns Config::default() without error.
pub fn load() -> Result<Config, ConfigError> {
    let path = config_path()?;
    let dir = path.parent().ok_or(ConfigError::NoAppDir)?;
    load_from_dir(dir)
}

/// Saves config to disk. Creates parent directories if needed.
pub fn save(config: &Config) -> Result<(), ConfigError> {
    let path = config_path()?;
    let dir = path.parent().ok_or(ConfigError::NoAppDir)?;
    save_to_dir(config, dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempdir().unwrap();
        let cfg = load_from_dir(dir.path()).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn save_and_reload_round_trips() {
        let dir = tempdir().unwrap();
        let cfg = Config::default();
        save_to_dir(&cfg, dir.path()).unwrap();
        let reloaded = load_from_dir(dir.path()).unwrap();
        assert_eq!(reloaded, cfg);
    }

    #[test]
    fn load_ignores_legacy_enabled_field() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("config.toml"), "enabled = false\n").unwrap();
        let cfg = load_from_dir(dir.path()).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn load_missing_display_section_uses_defaults() {
        let dir = tempdir().unwrap();
        // Config file with no [display] section at all.
        std::fs::write(dir.path().join("config.toml"), "").unwrap();
        let cfg = load_from_dir(dir.path()).unwrap();
        assert_eq!(cfg.display, DisplayConfig::default());
        assert!(cfg.display.show_title);
        assert!(cfg.display.show_artist);
        assert!(cfg.display.show_album);
        assert!(cfg.display.show_artwork);
    }

    #[test]
    fn partial_display_section_uses_field_defaults() {
        let dir = tempdir().unwrap();
        // Only override show_artwork; other three should remain true.
        std::fs::write(
            dir.path().join("config.toml"),
            "[display]\nshow_artwork = false\n",
        )
        .unwrap();
        let cfg = load_from_dir(dir.path()).unwrap();
        assert!(cfg.display.show_title);
        assert!(cfg.display.show_artist);
        assert!(cfg.display.show_album);
        assert!(!cfg.display.show_artwork);
    }

    #[test]
    fn round_trip_with_custom_display() {
        let dir = tempdir().unwrap();
        let cfg = Config {
            display: DisplayConfig {
                show_title: false,
                show_artist: false,
                show_album: false,
                show_artwork: false,
            },
        };
        save_to_dir(&cfg, dir.path()).unwrap();
        let reloaded = load_from_dir(dir.path()).unwrap();
        assert_eq!(reloaded, cfg);
    }
}
