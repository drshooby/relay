use std::path::PathBuf;

use crate::config;
use crate::constants::FIRST_RUN_NOTIFICATION_OSASCRIPT;

/// Show a macOS notification before the first Apple Events permission prompt.
/// Uses a marker file so the notification fires at most once per install.
pub fn maybe_show_first_run_notification() {
    let marker = match marker_path() {
        Some(p) => p,
        None => return,
    };

    if marker.exists() {
        return;
    }

    let status = std::process::Command::new("osascript")
        .args(["-e", FIRST_RUN_NOTIFICATION_OSASCRIPT])
        .status();

    if let Err(e) = status {
        tracing::warn!("first-run notification failed: {e}");
    }

    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&marker, "");
}

fn marker_path() -> Option<PathBuf> {
    config::data_dir()
        .ok()
        .map(|d| d.join(".permission-prompt-shown"))
}
