use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { enabled: true }
    }
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
    fn load_missing_file_returns_default_enabled() {
        let dir = tempdir().unwrap();
        let cfg = load_from_dir(dir.path()).unwrap();
        assert!(cfg.enabled);
    }

    #[test]
    fn save_and_reload_preserves_disabled_state() {
        let dir = tempdir().unwrap();
        let cfg = Config { enabled: false };
        save_to_dir(&cfg, dir.path()).unwrap();
        let reloaded = load_from_dir(dir.path()).unwrap();
        assert!(!reloaded.enabled);
    }
}
