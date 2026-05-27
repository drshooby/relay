//! Append-only rotating error log.
//!
//! Each entry is a newline-delimited JSON object:
//! `{"ts":"<ISO8601>","component":"<str>","message":"<str>"}`
//!
//! The log is capped at [`ERRORS_LOG_MAX_LINES`] entries.  When the cap is
//! reached the oldest entries are dropped (the file is rewritten with only the
//! most-recent entries).

use std::path::PathBuf;

use crate::config::data_dir;
use crate::constants::{ERRORS_LOG_FILE, ERRORS_LOG_MAX_LINES};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Append one error entry to the rotating log.
///
/// File I/O is performed on a `spawn_blocking` thread.  The caller `await`s
/// the spawned task — if the spawn itself fails the error is logged via
/// `tracing::warn!` and execution continues.
pub async fn record(
    component: impl Into<String> + Send + 'static,
    message: impl Into<String> + Send + 'static,
) {
    let component = component.into();
    let message = message.into();
    match tokio::task::spawn_blocking(move || append_entry(component, message)).await {
        Ok(Err(e)) => tracing::warn!("failed to write error log entry: {e}"),
        Err(e) => tracing::warn!("error log spawn_blocking panicked: {e}"),
        Ok(Ok(())) => {}
    }
}

// ---------------------------------------------------------------------------
// Internal implementation
// ---------------------------------------------------------------------------

/// Returns the path to the errors log file.
fn log_path() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let dir = data_dir()?;
    Ok(dir.join(ERRORS_LOG_FILE))
}

/// Build an ISO8601 timestamp string for right now.
fn iso8601_now() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format: YYYY-MM-DDTHH:MM:SSZ (manual, no chrono dep)
    let s = secs;
    let sec = s % 60;
    let min = (s / 60) % 60;
    let hour = (s / 3600) % 24;
    let days = s / 86400;
    // Days since Unix epoch → calendar date
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, min, sec
    )
}

/// Convert days-since-epoch to (year, month, day).
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Gregorian calendar algorithm (integer arithmetic only).
    let mut year = 1970u64;
    loop {
        let leap = is_leap(year);
        let days_in_year = if leap { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let leap = is_leap(year);
    let days_in_month = [
        31u64,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u64;
    for &dim in &days_in_month {
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

/// Append a single entry to the log file at `path_override` (test shim) or
/// the default path, rotating if the file exceeds [`ERRORS_LOG_MAX_LINES`].
///
/// Exposed as `pub(crate)` for tests that need to inject a custom path.
pub(crate) fn append_entry_at(
    path: &std::path::Path,
    component: &str,
    message: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let ts = iso8601_now();
    let entry = format!(
        "{{\"ts\":\"{ts}\",\"component\":\"{}\",\"message\":\"{}\"}}",
        component.replace('"', "\\\""),
        message.replace('"', "\\\""),
    );

    // Read existing lines (if any).
    let mut lines: Vec<String> = if path.exists() {
        std::fs::read_to_string(path)?
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect()
    } else {
        Vec::new()
    };

    lines.push(entry);

    // Rotate: keep only the last ERRORS_LOG_MAX_LINES entries.
    if lines.len() > ERRORS_LOG_MAX_LINES {
        let drop = lines.len() - ERRORS_LOG_MAX_LINES;
        lines.drain(..drop);
    }

    let content = lines.join("\n") + "\n";
    std::fs::write(path, content)?;
    Ok(())
}

fn append_entry(
    component: String,
    message: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = log_path()?;
    append_entry_at(&path, &component, &message)
}

/// Read all entries from the log at the given path, returning them in
/// reverse-chronological order (newest first).
///
/// Used only in tests — the Swift UI reads the file directly.
#[cfg(test)]
pub(crate) fn read_entries_at(path: &std::path::Path) -> Vec<serde_json::Value> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut entries: Vec<serde_json::Value> = content
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    entries.reverse();
    entries
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_log(dir: &TempDir) -> std::path::PathBuf {
        dir.path().join("errors.jsonl")
    }

    #[test]
    fn append_creates_dir_if_missing() {
        let dir = TempDir::new().expect("tempdir");
        let nested = dir.path().join("a/b/c/errors.jsonl");
        append_entry_at(&nested, "test", "hello").expect("should succeed");
        assert!(nested.exists(), "file should be created");
    }

    #[test]
    fn append_writes_then_rotates_at_cap() {
        let dir = TempDir::new().expect("tempdir");
        let path = temp_log(&dir);

        // Write ERRORS_LOG_MAX_LINES + 5 entries.
        let total = ERRORS_LOG_MAX_LINES + 5;
        for i in 0..total {
            append_entry_at(&path, "comp", &format!("msg-{i}")).expect("write ok");
        }

        let content = std::fs::read_to_string(&path).expect("read ok");
        let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();

        // File must have exactly ERRORS_LOG_MAX_LINES entries after rotation.
        assert_eq!(
            lines.len(),
            ERRORS_LOG_MAX_LINES,
            "expected {ERRORS_LOG_MAX_LINES} lines after rotation, got {}",
            lines.len()
        );

        // The oldest 5 entries (msg-0 .. msg-4) must have been dropped.
        for line in &lines {
            for dropped in 0..5 {
                assert!(
                    !line.contains(&format!("msg-{dropped}\"")),
                    "dropped entry msg-{dropped} should not be in rotated log"
                );
            }
        }

        // The newest entry must still be present.
        let last_msg = format!("msg-{}", total - 1);
        assert!(
            lines.iter().any(|l| l.contains(&last_msg)),
            "newest entry {last_msg} must be present"
        );
    }

    #[test]
    fn read_returns_reverse_chronological() {
        let dir = TempDir::new().expect("tempdir");
        let path = temp_log(&dir);

        for i in 0..5u64 {
            append_entry_at(&path, "comp", &format!("msg-{i}")).expect("write ok");
        }

        let entries = read_entries_at(&path);
        assert_eq!(entries.len(), 5);

        // First element should be the last-written entry.
        let first_msg = entries[0]["message"].as_str().expect("message field");
        assert_eq!(first_msg, "msg-4", "first returned entry should be newest");

        let last_msg = entries[4]["message"].as_str().expect("message field");
        assert_eq!(last_msg, "msg-0", "last returned entry should be oldest");
    }

    #[test]
    fn entry_has_required_fields() {
        let dir = TempDir::new().expect("tempdir");
        let path = temp_log(&dir);

        append_entry_at(&path, "discord", "disconnected").expect("write ok");

        let content = std::fs::read_to_string(&path).expect("read");
        let line = content.lines().next().expect("one line");
        let v: serde_json::Value = serde_json::from_str(line).expect("valid json");
        assert!(v["ts"].is_string(), "ts must be a string");
        assert_eq!(v["component"], "discord");
        assert_eq!(v["message"], "disconnected");
    }
}
