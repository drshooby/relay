# Status Dashboard + Permission Onboarding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the flat `TrayState` enum with a `TrayStatus` struct that exposes independent Discord/helper/playback rows in the tray menu, and add first-run permission onboarding for the Apple Events automation prompt.

**Architecture:** `TrayStatus` (src/tray/mod.rs) holds three independent sub-enums (PlaybackStatus, DiscordHealth, HelperHealth) plus an optional last-error string. The pipeline updates each sub-field individually and sends the full struct via a new `UserEvent::StatusUpdate` variant. The event loop rebuilds each menu row's text in-place via `set_text`. The Swift helper emits a new `permission_denied` IPC event when AppleScript fails with error -1743. A first-run marker prevents the pre-prompt notification from firing more than once.

**Tech Stack:** Rust / Tokio / tray-icon / muda menu crate, Swift helper, NSAppleScript, `osascript` subprocess for notifications.

---

## File Map

| File | Action | Purpose |
|------|--------|---------|
| `src/constants.rs` | Modify | Add all new label constants |
| `src/media/event.rs` | Modify | Add `MediaEvent::PermissionDenied` variant + test |
| `helper/Sources/main.swift` | Modify | Emit `permission_denied` on AppleScript error -1743 |
| `src/config.rs` | Modify | Add `data_dir()` helper |
| `src/onboarding.rs` | Create | `maybe_show_first_run_notification()` |
| `src/lib.rs` | Modify | Expose `onboarding` module |
| `src/tray/mod.rs` | Modify | Replace `TrayState` enum with `TrayStatus` struct + sub-enums |
| `src/tray/icons.rs` | Modify | Port `TrayState::icon()` to work on `TrayStatus` |
| `src/tray/event_loop.rs` | Modify | Rebuild menu layout; replace `UserEvent::StateUpdate` with `StatusUpdate` |
| `src/pipeline.rs` | Modify | Replace `TrayHealth` with `TrayStatus`; route `PermissionDenied` |
| `src/main.rs` | Modify | Silent tracing default; call `maybe_show_first_run_notification()` |
| `TESTING.md` | Modify | Append manual checks section |

---

## Task 1: Add new constants to `src/constants.rs`

**Files:**
- Modify: `src/constants.rs`

- [ ] **Step 1: Add the new constants**

Open `src/constants.rs` and append the following constants at the end of the file (before the `#[cfg(test)]` block):

```rust
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
```

- [ ] **Step 2: Run clippy to confirm no issues**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo clippy -- -D warnings 2>&1 | head -30
```

Expected: passes (no new warnings)

- [ ] **Step 3: Commit**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && git add src/constants.rs && git commit -m "feat(constants): add status dashboard and onboarding labels"
```

---

## Task 2: Add `MediaEvent::PermissionDenied` to `src/media/event.rs`

**Files:**
- Modify: `src/media/event.rs`

- [ ] **Step 1: Write the failing test**

Add at the bottom of the `#[cfg(test)]` block in `src/media/event.rs`:

```rust
#[test]
fn parses_permission_denied_event() {
    let line = r#"{"event":"permission_denied"}"#;
    let ev = parse_event_line(line).unwrap();
    assert_eq!(ev, MediaEvent::PermissionDenied);
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo test parses_permission_denied_event 2>&1
```

Expected: compile error — `PermissionDenied` variant does not exist yet.

- [ ] **Step 3: Add `PermissionDenied` variant**

In `src/media/event.rs`, add `PermissionDenied,` as the last variant of the `MediaEvent` enum (before the closing `}`):

```rust
#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum MediaEvent {
    TrackChanged {
        title: String,
        #[serde(default)]
        artist: String,
        #[serde(default)]
        album: String,
        #[serde(
            default,
            rename = "elapsed",
            deserialize_with = "deserialize_optional_u64"
        )]
        elapsed_secs: Option<u64>,
        #[serde(
            default,
            rename = "duration",
            deserialize_with = "deserialize_optional_u64"
        )]
        duration_secs: Option<u64>,
    },
    PositionChanged {
        #[serde(rename = "elapsed", deserialize_with = "deserialize_u64")]
        elapsed_secs: u64,
    },
    PlaybackPaused,
    PlaybackStopped,
    PermissionDenied,
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo test parses_permission_denied_event 2>&1
```

Expected: PASS

- [ ] **Step 5: Run all tests to confirm no regressions**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo test 2>&1
```

Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && git add src/media/event.rs && git commit -m "feat(media): add PermissionDenied IPC event variant"
```

---

## Task 3: Update Swift helper to emit `permission_denied` on error -1743

**Files:**
- Modify: `helper/Sources/main.swift`

- [ ] **Step 1: Locate the error-handling branch in `emitCurrentState`**

In `helper/Sources/main.swift`, find this block (around line 263):

```swift
if let err = errorDict {
    log("\(reason): AppleScript query failed: \(err)")
    return
}
```

- [ ] **Step 2: Replace the error-handling branch to promote -1743**

Replace the block above with:

```swift
if let err = errorDict {
    let errNum = (err[NSAppleScript.errorNumber] as? NSNumber)?.intValue ?? 0
    if errNum == -1743 {
        emit(["event": "permission_denied"])
        log("\(reason): permission_denied (errAEEventNotPermitted)")
        return
    }
    log("\(reason): AppleScript query failed: \(err)")
    return
}
```

- [ ] **Step 3: Verify the Swift helper still builds**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo build 2>&1 | tail -20
```

Expected: builds cleanly (build.rs compiles the helper)

- [ ] **Step 4: Commit**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && git add helper/Sources/main.swift && git commit -m "feat(helper): emit permission_denied on AppleScript error -1743"
```

---

## Task 4: Add `config::data_dir()` and create `src/onboarding.rs`

**Files:**
- Modify: `src/config.rs`
- Create: `src/onboarding.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add `data_dir()` to `src/config.rs`**

Add after the `config_path()` function:

```rust
/// Returns the application data directory: ~/Library/Application Support/relay
pub fn data_dir() -> Result<PathBuf, ConfigError> {
    let path = config_path()?;
    path.parent()
        .map(|p| p.to_owned())
        .ok_or(ConfigError::NoAppDir)
}
```

- [ ] **Step 2: Run clippy**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo clippy -- -D warnings 2>&1 | head -20
```

Expected: passes

- [ ] **Step 3: Create `src/onboarding.rs`**

```rust
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
```

- [ ] **Step 4: Expose the module in `src/lib.rs`**

Add `pub mod onboarding;` to `src/lib.rs`.

- [ ] **Step 5: Run clippy**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo clippy -- -D warnings 2>&1 | head -20
```

Expected: passes

- [ ] **Step 6: Commit**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && git add src/config.rs src/onboarding.rs src/lib.rs && git commit -m "feat(onboarding): add first-run permission notification"
```

---

## Task 5: Refactor `TrayState` → `TrayStatus` in `src/tray/mod.rs`

This is the core refactor. The existing `TrayState` enum is replaced with a `TrayStatus` struct containing three independent sub-enums.

**Files:**
- Modify: `src/tray/mod.rs`
- Modify: `src/tray/icons.rs` (port `TrayState::icon()` to use `TrayStatus`)

**Note:** This task will temporarily break `pipeline.rs` and `event_loop.rs` — that's expected. We fix those in Tasks 6 and 7.

- [ ] **Step 1: Completely replace `src/tray/mod.rs`**

Write the full new content:

```rust
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
        assert_eq!(DiscordHealth::Connected.row_text(), TRAY_DISCORD_LABEL_CONNECTED);
    }

    #[test]
    fn discord_reconnecting_row_text() {
        let s = DiscordHealth::Reconnecting { backoff_ms: 1200 };
        assert_eq!(s.row_text(), "Discord: Reconnecting in 1200ms");
    }

    #[test]
    fn discord_disconnected_row_text() {
        let s = DiscordHealth::Disconnected { detail: "pipe closed".into() };
        assert_eq!(s.row_text(), "Discord: Disconnected");
    }

    // --- HelperHealth row_text ---

    #[test]
    fn helper_running_row_text() {
        assert_eq!(HelperHealth::Running.row_text(), TRAY_HELPER_LABEL_RUNNING);
    }

    #[test]
    fn helper_unavailable_row_text() {
        let s = HelperHealth::Unavailable { detail: "exited with code 1".into() };
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
            discord: DiscordHealth::Disconnected { detail: "lost".into() },
            discord_was_connected: true,
            ..TrayStatus::default()
        };
        assert_eq!(status.icon_variant(), icons::TrayIconVariant::Error);
    }

    #[test]
    fn icon_variant_normal_discord_disconnect_before_first_connect() {
        // No error before first connection — suppress initial disconnect noise.
        let status = TrayStatus {
            discord: DiscordHealth::Disconnected { detail: "not yet".into() },
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
```

- [ ] **Step 2: Update `src/tray/icons.rs` to use `TrayStatus`**

The current `icons.rs` has `impl TrayState { pub fn icon(&self) -> Icon { ... } }`. Replace that `impl TrayState` block with `impl TrayStatus`:

Find this block in `src/tray/icons.rs`:
```rust
impl TrayState {
    pub fn icon(&self) -> Icon {
        let set = icons();
        match self.icon_variant() {
            TrayIconVariant::Normal => set.normal.clone(),
            TrayIconVariant::Error => set.error.clone(),
        }
    }
}
```

Replace with:
```rust
impl TrayStatus {
    pub fn icon(&self) -> Icon {
        let set = icons();
        match self.icon_variant() {
            TrayIconVariant::Normal => set.normal.clone(),
            TrayIconVariant::Error => set.error.clone(),
        }
    }
}
```

Also update the import at the top of `icons.rs`: replace `use crate::tray::TrayState;` with `use crate::tray::TrayStatus;`.

- [ ] **Step 3: Run cargo check to verify the mod.rs and icons.rs compile**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo check 2>&1 | head -40
```

Expected: errors only in `pipeline.rs` and `event_loop.rs` (they still reference old types) — `tray/mod.rs` and `tray/icons.rs` should be clean.

- [ ] **Step 4: Run the new tray tests in isolation**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo test -p relay tray 2>&1
```

Expected: all tray tests pass

- [ ] **Step 5: Commit**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && git add src/tray/mod.rs src/tray/icons.rs && git commit -m "refactor(tray): replace TrayState enum with TrayStatus struct"
```

---

## Task 6: Update `src/pipeline.rs` to use `TrayStatus`

**Files:**
- Modify: `src/pipeline.rs`

The existing `TrayHealth` struct is removed. `TrayStatus` is used directly. We keep the `discord_was_connected` semantics inside `TrayStatus.discord_was_connected`.

- [ ] **Step 1: Write the new failing test first**

In `pipeline.rs` `#[cfg(test)]` block, add this new test (it will fail to compile until the implementation is done):

```rust
#[test]
fn permission_denied_then_track_changed_clears_helper_state() {
    let mut status = TrayStatus::new();

    // Simulate permission denied
    status.helper = HelperHealth::PermissionDenied;
    status.last_error = Some(TRAY_PERMISSION_DENIED_DETAIL.to_string());

    assert_eq!(status.helper, HelperHealth::PermissionDenied);
    assert!(status.last_error.is_some());

    // Simulate a successful track event arriving — should clear the permission state
    if matches!(status.helper, HelperHealth::PermissionDenied) {
        status.helper = HelperHealth::Running;
        status.last_error = None;
    }

    assert_eq!(status.helper, HelperHealth::Running);
    assert!(status.last_error.is_none());
}
```

- [ ] **Step 2: Replace the full `src/pipeline.rs`**

The key changes:
1. Remove `TrayHealth` struct entirely
2. Use `TrayStatus` directly as `status: TrayStatus`
3. Add a `send_status` closure that sends `UserEvent::StatusUpdate(status.clone())`
4. Route `MediaEvent::PermissionDenied` to `status.helper = HelperHealth::PermissionDenied` + `status.last_error = Some(TRAY_PERMISSION_DENIED_DETAIL.into())`
5. When any successful track/pause/stop event arrives while `status.helper == HelperHealth::PermissionDenied`, clear it back to `Running` and clear `last_error`
6. On helper process exit/IO error, set `status.helper = HelperHealth::Unavailable { detail }` and `status.last_error = Some(detail)`
7. On Discord status changes, update `status.discord` and `status.discord_was_connected`
8. Port the old tests (`resolved_*`) to work with the new `TrayStatus` fields

Here is the full replacement for `src/pipeline.rs`:

```rust
use std::sync::Arc;

use tokio::sync::RwLock;
use winit::event_loop::EventLoopProxy;

use crate::artwork::cache::ArtworkCache;
use crate::artwork::itunes::{search_track, TrackLookup};
use crate::config::{self, Config, DisplayConfig};
use crate::constants::{
    CHANNEL_BUFFER_SIZE, TRACK_DEBOUNCE_MS, TRAY_PERMISSION_DENIED_DETAIL,
};
use crate::discord::activity::{compute_started_at, TrackInfo};
use crate::discord::client::{run_discord_client, DiscordCommand, DiscordStatus};
use crate::media::debounce::Debouncer;
use crate::media::event::{HelperCommand, HelperStatus, MediaEvent};
use crate::media::reader;
use crate::tray::event_loop::UserEvent;
use crate::tray::{DiscordHealth, HelperHealth, PlaybackStatus, TrayStatus};

/// Which display field to toggle.
#[derive(Debug, Clone, Copy)]
pub enum DisplayField {
    Title,
    Artist,
    Album,
    Artwork,
}

/// Commands sent from the main (winit) thread to the Tokio pipeline.
#[derive(Debug)]
pub enum AppCommand {
    Quit,
    SetDisplayField { field: DisplayField, enabled: bool },
}

/// Cached Discord activity fields reused for position-only updates.
#[derive(Debug, Clone)]
struct ActiveTrack {
    track: TrackInfo,
    artwork_url: Option<String>,
    track_url: Option<String>,
    duration_secs: Option<u64>,
    paused: bool,
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn send_set_activity(
    discord_tx: &tokio::sync::mpsc::Sender<DiscordCommand>,
    active: &ActiveTrack,
    elapsed_secs: Option<u64>,
    debounce_ms: u64,
    display: DisplayConfig,
) {
    let started_at = compute_started_at(now_unix_secs(), elapsed_secs, debounce_ms);
    let _ = discord_tx
        .send(DiscordCommand::SetActivity {
            track: active.track.clone(),
            artwork_url: active.artwork_url.clone(),
            track_url: active.track_url.clone(),
            started_at,
            duration_secs: active.duration_secs,
            display,
        })
        .await;
}

pub async fn run_pipeline(
    proxy: EventLoopProxy<UserEvent>,
    mut app_cmd_rx: tokio::sync::mpsc::Receiver<AppCommand>,
    cfg: Arc<RwLock<Config>>,
) {
    use tokio::sync::mpsc;

    let (event_tx, mut event_rx) = mpsc::channel::<MediaEvent>(CHANNEL_BUFFER_SIZE);
    let (status_tx, mut status_rx) = mpsc::channel(4);
    let (discord_tx, discord_rx) = mpsc::channel::<DiscordCommand>(CHANNEL_BUFFER_SIZE);
    let (discord_status_tx, mut discord_status_rx) = mpsc::channel::<DiscordStatus>(8);
    let (helper_cmd_tx, helper_cmd_rx) = mpsc::channel::<HelperCommand>(8);

    // Spawn the Swift helper reader/writer.
    tokio::spawn(async move {
        reader::run_helper(event_tx, status_tx, helper_cmd_rx).await;
    });

    // Spawn the Discord RPC client. Receives helper_cmd_tx so it can request
    // a fresh state snapshot from the helper after a reconnect.
    tokio::spawn(async move {
        run_discord_client(discord_rx, helper_cmd_tx, discord_status_tx).await;
    });

    // Pipeline state.
    let mut debouncer = Debouncer::new(std::time::Duration::from_millis(TRACK_DEBOUNCE_MS));
    let (debounced_tx, mut debounced_rx) = mpsc::channel::<MediaEvent>(CHANNEL_BUFFER_SIZE);
    let mut artwork_cache = tokio::task::spawn_blocking(ArtworkCache::load)
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
    let http_client = reqwest::Client::new();
    let mut status = TrayStatus::new();
    let mut last_sent: Option<TrayStatus> = None;
    let mut active_track: Option<ActiveTrack> = None;

    let mut send_status = |s: TrayStatus| {
        if last_sent.as_ref() == Some(&s) {
            return;
        }
        last_sent = Some(s.clone());
        let _ = proxy.send_event(UserEvent::StatusUpdate(s));
    };

    loop {
        tokio::select! {
            // Helper process status changes (exit / IO error).
            Some(helper_status) = status_rx.recv() => {
                match helper_status {
                    HelperStatus::Running => {}
                    HelperStatus::Exited { code } => {
                        let detail = match code {
                            Some(c) => format!("helper exited with code {}", c),
                            None => "helper exited".to_string(),
                        };
                        tracing::error!("helper exited: {detail}");
                        status.helper = HelperHealth::Unavailable { detail: detail.clone() };
                        status.last_error = Some(detail);
                        send_status(status.clone());
                    }
                    HelperStatus::IoError => {
                        let detail = "helper io error".to_string();
                        tracing::error!("{detail}");
                        status.helper = HelperHealth::Unavailable { detail: detail.clone() };
                        status.last_error = Some(detail);
                        send_status(status.clone());
                    }
                }
            }

            Some(discord_status) = discord_status_rx.recv() => {
                match discord_status {
                    DiscordStatus::Connected => {
                        tracing::info!("discord connected");
                        status.discord = DiscordHealth::Connected;
                        status.discord_was_connected = true;
                        send_status(status.clone());
                    }
                    DiscordStatus::Disconnected { detail } => {
                        status.discord = DiscordHealth::Disconnected { detail };
                        if status.discord_was_connected {
                            send_status(status.clone());
                        }
                    }
                    DiscordStatus::Reconnecting { backoff_ms } => {
                        status.discord = DiscordHealth::Reconnecting { backoff_ms };
                        if status.discord_was_connected {
                            send_status(status.clone());
                        }
                    }
                }
            }

            // Raw media events from the helper.
            Some(event) = event_rx.recv() => {
                match &event {
                    MediaEvent::PermissionDenied => {
                        tracing::warn!("apple music permission denied");
                        status.helper = HelperHealth::PermissionDenied;
                        status.last_error = Some(TRAY_PERMISSION_DENIED_DETAIL.to_string());
                        send_status(status.clone());
                    }
                    MediaEvent::PositionChanged { .. } => {
                        if let MediaEvent::PositionChanged { elapsed_secs } = event {
                            if let Some(active) = active_track.as_ref() {
                                // Skip position updates while paused — adding timestamps to
                                // a paused card would incorrectly re-enable the progress bar.
                                if !active.paused {
                                    let display = cfg.read().await.display.clone();
                                    send_set_activity(
                                        &discord_tx,
                                        active,
                                        Some(elapsed_secs),
                                        0,
                                        display,
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                    MediaEvent::TrackChanged { .. } => {
                        // Clear permission-denied state on recovery.
                        if matches!(status.helper, HelperHealth::PermissionDenied) {
                            status.helper = HelperHealth::Running;
                            status.last_error = None;
                        }
                        // Drop stale metadata until debounce completes so position_changed
                        // cannot advance the progress bar for the previous track.
                        active_track = None;
                        debouncer.submit(event, debounced_tx.clone());
                    }
                    MediaEvent::PlaybackPaused => {
                        // Clear permission-denied state on recovery.
                        if matches!(status.helper, HelperHealth::PermissionDenied) {
                            status.helper = HelperHealth::Running;
                            status.last_error = None;
                        }
                        // Do NOT clear active_track here — let the debounced handler
                        // mutate the paused flag so rapid pause→resume doesn't lose
                        // track context.
                        debouncer.submit(event, debounced_tx.clone());
                    }
                    MediaEvent::PlaybackStopped => {
                        // Clear permission-denied state on recovery.
                        if matches!(status.helper, HelperHealth::PermissionDenied) {
                            status.helper = HelperHealth::Running;
                            status.last_error = None;
                        }
                        active_track = None;
                        debouncer.submit(event, debounced_tx.clone());
                    }
                }
            }

            // Debounced events — look up artwork, push to Discord, refresh tray.
            Some(event) = debounced_rx.recv() => {
                match event {
                    MediaEvent::TrackChanged {
                        title,
                        artist,
                        album,
                        elapsed_secs,
                        duration_secs,
                    } => {
                        let track = TrackInfo {
                            title: title.clone(),
                            artist: artist.clone(),
                            album,
                        };

                        status.playback = PlaybackStatus::Playing {
                            title: title.clone(),
                            artist: artist.clone(),
                        };
                        send_status(status.clone());

                        // Artwork + track URL: cache-first, then iTunes search.
                        let lookup = if let Some(cached) = artwork_cache.get(&artist, &title) {
                            cached
                        } else {
                            match search_track(&http_client, &artist, &title).await {
                                Ok(Some(found)) => {
                                    artwork_cache.insert(&artist, &title, found.clone());
                                    let cache_snapshot = artwork_cache.clone();
                                    tokio::task::spawn_blocking(move || {
                                        if let Err(e) = cache_snapshot.save() {
                                            tracing::warn!("failed to persist artwork cache: {e}");
                                        }
                                    });
                                    found
                                }
                                Ok(None) => TrackLookup {
                                    artwork_url: None,
                                    track_url: None,
                                    duration_secs: None,
                                },
                                Err(e) => {
                                    tracing::warn!("artwork lookup failed: {e}");
                                    TrackLookup {
                                        artwork_url: None,
                                        track_url: None,
                                        duration_secs: None,
                                    }
                                }
                            }
                        };

                        let resolved_duration = duration_secs.or(lookup.duration_secs);

                        let active = ActiveTrack {
                            track: track.clone(),
                            artwork_url: lookup.artwork_url.clone(),
                            track_url: lookup.track_url.clone(),
                            duration_secs: resolved_duration,
                            paused: false,
                        };
                        active_track = Some(active.clone());
                        let display = cfg.read().await.display.clone();
                        send_set_activity(
                            &discord_tx,
                            &active,
                            elapsed_secs,
                            TRACK_DEBOUNCE_MS,
                            display,
                        )
                        .await;
                    }

                    MediaEvent::PlaybackPaused => {
                        if let Some(active) = active_track.as_mut() {
                            active.paused = true;
                            status.playback = PlaybackStatus::Paused {
                                title: active.track.title.clone(),
                                artist: active.track.artist.clone(),
                            };
                            send_status(status.clone());
                            let display = cfg.read().await.display.clone();
                            let _ = discord_tx
                                .send(DiscordCommand::SetPausedActivity {
                                    track: active.track.clone(),
                                    artwork_url: active.artwork_url.clone(),
                                    track_url: active.track_url.clone(),
                                    display,
                                })
                                .await;
                        } else {
                            // No active track established yet — safe fallback.
                            status.playback = PlaybackStatus::Idle;
                            send_status(status.clone());
                            let _ = discord_tx.send(DiscordCommand::ClearActivity).await;
                        }
                    }

                    MediaEvent::PlaybackStopped => {
                        active_track = None;
                        status.playback = PlaybackStatus::Idle;
                        send_status(status.clone());
                        let _ = discord_tx.send(DiscordCommand::ClearActivity).await;
                    }

                    MediaEvent::PositionChanged { .. } => {
                        // Position updates are handled on the raw event path.
                    }

                    MediaEvent::PermissionDenied => {
                        // Already handled on raw event path.
                    }
                }
            }

            // App commands from the main thread (quit / display toggles).
            cmd = app_cmd_rx.recv() => {
                match cmd {
                    Some(AppCommand::Quit) => {
                        let _ = discord_tx.send(DiscordCommand::Shutdown).await;
                        break;
                    }
                    Some(AppCommand::SetDisplayField { field, enabled }) => {
                        // Mutate the shared config.
                        {
                            let mut guard = cfg.write().await;
                            match field {
                                DisplayField::Title => guard.display.show_title = enabled,
                                DisplayField::Artist => guard.display.show_artist = enabled,
                                DisplayField::Album => guard.display.show_album = enabled,
                                DisplayField::Artwork => guard.display.show_artwork = enabled,
                            }
                        }
                        tracing::info!(
                            "display field {:?} set to {enabled}",
                            field
                        );

                        // Persist asynchronously — failure is non-fatal.
                        let snapshot = cfg.read().await.clone();
                        tokio::task::spawn_blocking(move || {
                            if let Err(e) = config::save(&snapshot) {
                                tracing::warn!("failed to persist config after display toggle: {e}");
                            }
                        });

                        // Force-republish with the new display snapshot.
                        let display = cfg.read().await.display.clone();
                        if let Some(active) = active_track.as_ref() {
                            if active.paused {
                                let _ = discord_tx
                                    .send(DiscordCommand::SetPausedActivity {
                                        track: active.track.clone(),
                                        artwork_url: active.artwork_url.clone(),
                                        track_url: active.track_url.clone(),
                                        display,
                                    })
                                    .await;
                            } else {
                                send_set_activity(
                                    &discord_tx,
                                    active,
                                    None,
                                    0,
                                    display,
                                )
                                .await;
                            }
                        }
                    }
                    None => break, // sender dropped — treat as quit
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::TRAY_PERMISSION_DENIED_DETAIL;
    use crate::tray::{DiscordHealth, HelperHealth, TrayStatus};

    #[test]
    fn status_shows_discord_error_after_disconnect() {
        let mut status = TrayStatus::new();
        status.discord_was_connected = true;
        status.discord = DiscordHealth::Disconnected {
            detail: "discord ipc: pipe closed".to_string(),
        };
        assert_eq!(status.discord.row_text(), "Discord: Disconnected");
        assert_eq!(
            status.icon_variant(),
            crate::tray::icons::TrayIconVariant::Error
        );
    }

    #[test]
    fn status_no_discord_error_before_first_connection() {
        let status = TrayStatus::new();
        // discord_was_connected defaults to false — should be Normal even though
        // DiscordHealth defaults to Connected, but let's test the Disconnected case too.
        let mut s = status.clone();
        s.discord = DiscordHealth::Disconnected { detail: "not yet connected".into() };
        // discord_was_connected is still false
        assert_eq!(
            s.icon_variant(),
            crate::tray::icons::TrayIconVariant::Normal
        );
    }

    #[test]
    fn status_returns_to_normal_after_discord_reconnect() {
        let mut status = TrayStatus::new();
        status.discord_was_connected = true;
        status.discord = DiscordHealth::Disconnected { detail: "lost".into() };
        assert_eq!(
            status.icon_variant(),
            crate::tray::icons::TrayIconVariant::Error
        );

        status.discord = DiscordHealth::Connected;
        assert_eq!(
            status.icon_variant(),
            crate::tray::icons::TrayIconVariant::Normal
        );
    }

    #[test]
    fn status_helper_unavailable_sets_error() {
        let mut status = TrayStatus::new();
        let detail = "helper exited with code 1".to_string();
        status.helper = HelperHealth::Unavailable { detail: detail.clone() };
        status.last_error = Some(detail);
        assert_eq!(
            status.icon_variant(),
            crate::tray::icons::TrayIconVariant::Error
        );
    }

    #[test]
    fn permission_denied_then_track_changed_clears_helper_state() {
        let mut status = TrayStatus::new();

        // Simulate permission denied
        status.helper = HelperHealth::PermissionDenied;
        status.last_error = Some(TRAY_PERMISSION_DENIED_DETAIL.to_string());

        assert_eq!(status.helper, HelperHealth::PermissionDenied);
        assert!(status.last_error.is_some());

        // Simulate a successful track event arriving — should clear the permission state
        if matches!(status.helper, HelperHealth::PermissionDenied) {
            status.helper = HelperHealth::Running;
            status.last_error = None;
        }

        assert_eq!(status.helper, HelperHealth::Running);
        assert!(status.last_error.is_none());
    }
}
```

- [ ] **Step 3: Run cargo check**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo check 2>&1 | head -40
```

Expected: errors only in `event_loop.rs` (still references `StateUpdate`) — `pipeline.rs` should be clean.

- [ ] **Step 4: Run the new pipeline tests**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo test pipeline 2>&1
```

Expected: all pass

- [ ] **Step 5: Commit**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && git add src/pipeline.rs && git commit -m "refactor(pipeline): replace TrayHealth with TrayStatus; route PermissionDenied"
```

---

## Task 7: Rebuild `src/tray/event_loop.rs` with new menu layout

**Files:**
- Modify: `src/tray/event_loop.rs`

This is the final plumbing task. We replace `UserEvent::StateUpdate(TrayState)` with `UserEvent::StatusUpdate(TrayStatus)` and rebuild the menu with the new row structure.

Menu order:
1. Playback row (disabled)
2. Separator
3. Discord row (disabled)
4. Helper row (disabled)
5. Separator
6. Last error row (disabled, hidden via empty text when no error)
7. Separator
8. Display submenu (existing — preserve)
9. Separator
10. "Open System Settings…" item (enabled only when PermissionDenied)
11. Separator
12. Quit Relay

- [ ] **Step 1: Write the full replacement `src/tray/event_loop.rs`**

```rust
// TrayIcon is created inside `resumed()` (macOS requirement — must be on main thread after
// the event loop has started running).
// State updates arrive via EventLoopProxy<UserEvent> from the Tokio background thread.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    TrayIcon, TrayIconBuilder,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
};

use crate::config::Config;
use crate::constants::{
    SYSTEM_SETTINGS_AUTOMATION_URL, TRAY_DISPLAY_ALBUM_LABEL, TRAY_DISPLAY_ARTIST_LABEL,
    TRAY_DISPLAY_ARTWORK_LABEL, TRAY_DISPLAY_SUBMENU_LABEL, TRAY_DISPLAY_TITLE_LABEL,
    TRAY_OPEN_SETTINGS_LABEL,
};
use crate::pipeline::DisplayField;
use crate::tray::icons::{self, TrayIconVariant};
use crate::tray::{HelperHealth, PlaybackStatus, TrayStatus};

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

    /// Shared config — used to read initial display state when building the tray.
    cfg: Arc<RwLock<Config>>,

    tray_icon: Option<TrayIcon>,

    // Status row items.
    playback_item: Option<MenuItem>,
    discord_item: Option<MenuItem>,
    helper_item: Option<MenuItem>,
    last_error_item: Option<MenuItem>,
    settings_item: Option<MenuItem>,
    quit_item: Option<MenuItem>,

    last_icon_variant: Option<TrayIconVariant>,

    // Display submenu toggles.
    display_title_item: Option<CheckMenuItem>,
    display_artist_item: Option<CheckMenuItem>,
    display_album_item: Option<CheckMenuItem>,
    display_artwork_item: Option<CheckMenuItem>,
}

impl RelayApp {
    pub fn new(
        app_cmd_tx: tokio::sync::mpsc::Sender<crate::AppCommand>,
        cfg: Arc<RwLock<Config>>,
    ) -> Self {
        Self {
            app_cmd_tx,
            cfg,
            tray_icon: None,
            playback_item: None,
            discord_item: None,
            helper_item: None,
            last_error_item: None,
            settings_item: None,
            quit_item: None,
            last_icon_variant: None,
            display_title_item: None,
            display_artist_item: None,
            display_album_item: None,
            display_artwork_item: None,
        }
    }

    fn build_tray(&mut self) {
        // Read the current display config (blocking — main thread, pre-loop-start).
        let display = self.cfg.blocking_read().display.clone();

        // Status rows — all start disabled (cosmetic).
        let initial_status = TrayStatus::new();
        let playback_item = MenuItem::new(initial_status.playback.row_text(), false, None);
        let discord_item = MenuItem::new(initial_status.discord.row_text(), false, None);
        let helper_item = MenuItem::new(initial_status.helper.row_text(), false, None);
        let last_error_item = MenuItem::new("", false, None);

        // Display submenu with 4 checkable toggles.
        let display_title_item =
            CheckMenuItem::new(TRAY_DISPLAY_TITLE_LABEL, true, display.show_title, None);
        let display_artist_item =
            CheckMenuItem::new(TRAY_DISPLAY_ARTIST_LABEL, true, display.show_artist, None);
        let display_album_item =
            CheckMenuItem::new(TRAY_DISPLAY_ALBUM_LABEL, true, display.show_album, None);
        let display_artwork_item =
            CheckMenuItem::new(TRAY_DISPLAY_ARTWORK_LABEL, true, display.show_artwork, None);
        let display_submenu = Submenu::with_items(
            TRAY_DISPLAY_SUBMENU_LABEL,
            true,
            &[
                &display_title_item,
                &display_artist_item,
                &display_album_item,
                &display_artwork_item,
            ],
        )
        .expect("failed to build display submenu");

        // "Open System Settings…" — always present, toggled by PermissionDenied state.
        let settings_item = MenuItem::new("", false, None);

        let quit_item = MenuItem::new("Quit Relay", true, None);

        let sep = || PredefinedMenuItem::separator();

        let menu = Menu::with_items(&[
            &playback_item,
            &sep(),
            &discord_item,
            &helper_item,
            &sep(),
            &last_error_item,
            &sep(),
            &display_submenu,
            &sep(),
            &settings_item,
            &sep(),
            &quit_item,
        ])
        .expect("failed to build tray menu");

        let icon = icons::default_icon();

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .with_tooltip("Relay")
            .build()
            .expect("failed to build tray icon");

        tray.set_icon_as_template(true);

        self.playback_item = Some(playback_item);
        self.discord_item = Some(discord_item);
        self.helper_item = Some(helper_item);
        self.last_error_item = Some(last_error_item);
        self.settings_item = Some(settings_item);
        self.quit_item = Some(quit_item);
        self.display_title_item = Some(display_title_item);
        self.display_artist_item = Some(display_artist_item);
        self.display_album_item = Some(display_album_item);
        self.display_artwork_item = Some(display_artwork_item);
        self.tray_icon = Some(tray);
        self.last_icon_variant = Some(TrayIconVariant::Normal);
    }

    fn apply_status(&mut self, status: &TrayStatus) {
        if let Some(item) = &self.playback_item {
            item.set_text(status.playback.row_text());
        }

        if let Some(item) = &self.discord_item {
            item.set_text(status.discord.row_text());
        }

        if let Some(item) = &self.helper_item {
            item.set_text(status.helper.row_text());
        }

        if let Some(item) = &self.last_error_item {
            if let Some(text) = status.last_error_row_text() {
                item.set_text(text);
                item.set_enabled(true);
            } else {
                item.set_text("");
                item.set_enabled(false);
            }
        }

        // "Open System Settings…" — only enabled/visible when PermissionDenied.
        if let Some(item) = &self.settings_item {
            if status.helper == HelperHealth::PermissionDenied {
                item.set_text(TRAY_OPEN_SETTINGS_LABEL);
                item.set_enabled(true);
            } else {
                item.set_text("");
                item.set_enabled(false);
            }
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

        // "Open System Settings…" click.
        if self.settings_item.as_ref().is_some_and(|i| event.id == i.id()) {
            if let Err(e) = std::process::Command::new("open")
                .arg(SYSTEM_SETTINGS_AUTOMATION_URL)
                .status()
            {
                tracing::warn!("failed to open system settings: {e}");
            }
            return;
        }

        // Display toggle handlers: read the new checked state and forward to pipeline.
        let display_toggles: &[(Option<&CheckMenuItem>, DisplayField)] = &[
            (self.display_title_item.as_ref(), DisplayField::Title),
            (self.display_artist_item.as_ref(), DisplayField::Artist),
            (self.display_album_item.as_ref(), DisplayField::Album),
            (self.display_artwork_item.as_ref(), DisplayField::Artwork),
        ];
        for (item_opt, field) in display_toggles {
            if let Some(item) = item_opt {
                if event.id == item.id() {
                    let enabled = item.is_checked();
                    tracing::debug!("display toggle {:?} -> {enabled}", field);
                    let _ = self
                        .app_cmd_tx
                        .blocking_send(crate::AppCommand::SetDisplayField {
                            field: *field,
                            enabled,
                        });
                    return;
                }
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
```

- [ ] **Step 2: Run cargo check**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo check 2>&1 | head -40
```

Expected: only `main.rs` errors (UserEvent type mismatch) — `event_loop.rs` and `pipeline.rs` should be clean.

- [ ] **Step 3: Commit**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && git add src/tray/event_loop.rs && git commit -m "feat(tray): rebuild status dashboard menu with Discord/helper/error rows"
```

---

## Task 8: Update `src/main.rs` (silent tracing, onboarding call, wire new UserEvent)

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Replace `src/main.rs` with the updated version**

The key changes:
1. Tracing default: `EnvFilter::new("off")` when `RUST_LOG` is unset
2. Call `relay::onboarding::maybe_show_first_run_notification()` before spawning the pipeline thread
3. No changes to the UserEvent wiring (pipeline sends `StatusUpdate`, which event_loop already handles)

```rust
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

use relay::config::{self, Config};
use relay::pipeline::{run_pipeline, AppCommand};
use relay::tray::event_loop::{build_event_loop, RelayApp, UserEvent};

fn main() -> anyhow::Result<()> {
    // 1. Initialise tracing — default to "off" so production builds are silent.
    //    Set RUST_LOG=info (or debug/trace) to enable logs.
    let filter = std::env::var("RUST_LOG")
        .map(|v| EnvFilter::new(v))
        .unwrap_or_else(|_| EnvFilter::new("off"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    // 2. First-run onboarding: fire a notification before the helper spawns so the
    //    user sees Relay's copy *before* the OS Automation permission prompt.
    relay::onboarding::maybe_show_first_run_notification();

    // 3. Load config — fall back to default on any error (e.g. first run).
    let initial_config = config::load().unwrap_or_else(|e| {
        tracing::warn!("failed to load config, using defaults: {e}");
        Config::default()
    });
    let cfg = Arc::new(RwLock::new(initial_config));

    // 4. Cross-thread channel: main → Tokio (commands).
    //    tokio::sync::mpsc works here: the main (winit) thread uses blocking_send,
    //    the Tokio pipeline uses .recv().await.
    let (app_cmd_tx, app_cmd_rx) = tokio::sync::mpsc::channel::<AppCommand>(8);

    // 5. Build winit event loop on the main thread (macOS requirement).
    let event_loop = build_event_loop();
    let proxy = event_loop.create_proxy();

    // 6. Spawn the Tokio runtime on a dedicated OS thread so it never blocks the main thread.
    let cfg_pipeline = cfg.clone();
    let _tokio_thread = std::thread::spawn(move || {
        // multi_thread scheduler: work-stealing pool.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            // Panic is acceptable at startup if the runtime cannot be created.
            .expect("failed to create tokio runtime");

        rt.block_on(async move {
            run_pipeline(proxy, app_cmd_rx, cfg_pipeline).await;
        });
    });

    // 7. Run the winit event loop on the main thread (blocks until exit).
    run_event_loop(event_loop, app_cmd_tx, cfg)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Winit event loop (main thread)
// ---------------------------------------------------------------------------

fn run_event_loop(
    event_loop: winit::event_loop::EventLoop<UserEvent>,
    app_cmd_tx: tokio::sync::mpsc::Sender<AppCommand>,
    cfg: Arc<RwLock<Config>>,
) -> anyhow::Result<()> {
    use relay::constants::TRAY_POLL_INTERVAL_MS;
    use winit::event_loop::ControlFlow;

    // WaitUntil so about_to_wait is called at ~60 fps without busy-spinning.
    event_loop.set_control_flow(ControlFlow::WaitUntil(
        std::time::Instant::now() + std::time::Duration::from_millis(TRAY_POLL_INTERVAL_MS),
    ));

    let mut app = RelayApp::new(app_cmd_tx, cfg);

    event_loop
        .run_app(&mut app)
        .map_err(|e| anyhow::anyhow!("winit event loop error: {e}"))?;

    Ok(())
}
```

- [ ] **Step 2: Run `cargo build` to confirm everything compiles**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo build 2>&1 | tail -10
```

Expected: builds cleanly with no errors.

- [ ] **Step 3: Run all tests**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo test 2>&1
```

Expected: all tests pass

- [ ] **Step 4: Run clippy**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo clippy -- -D warnings 2>&1
```

Expected: no warnings

- [ ] **Step 5: Run cargo fmt check**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo fmt -- --check 2>&1
```

Expected: passes (no diffs)

- [ ] **Step 6: Commit**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && git add src/main.rs && git commit -m "feat(main): silent tracing default; call first-run onboarding notification"
```

---

## Task 9: Append manual checks to `TESTING.md`

**Files:**
- Modify: `TESTING.md`

- [ ] **Step 1: Append new section to TESTING.md**

Append the following to the bottom of `TESTING.md`:

```markdown

## Status dashboard + onboarding manual checks

Prerequisites: macOS, Apple Music available, Discord running (or available to kill).

### 1. Status rows render

- [ ] Launch `cargo run`, click the tray icon
- [ ] See "Now Playing: Idle", "Discord: Connected", "Helper: Running" rows in the documented order
- [ ] Play a track in Apple Music → "Now Playing: <title> — <artist>" updates within ~2s
- [ ] Pause → "Paused — <title> — <artist>" appears in the playback row

### 2. Silent stdout/stderr

- [ ] Run `cargo run` (no RUST_LOG set) for ~5 s with Apple Music not playing
- [ ] Confirm zero lines on stdout and stderr
- [ ] ^C to stop

### 3. Developer logging still works

- [ ] Run `RUST_LOG=info cargo run` for ~5 s
- [ ] Confirm lifecycle log lines appear (discord connect, helper start, etc.)
- [ ] ^C to stop

### 4. Discord disconnect surfaces

- [ ] While `cargo run` is running, kill Discord (`pkill Discord`)
- [ ] Within ~2 s the "Discord: Reconnecting in Xms" row appears in the tray menu
- [ ] Restart Discord — "Discord: Connected" returns; icon returns to normal

### 5. Helper crash surfaces

- [ ] While `cargo run` is running, kill the helper (`pkill relay-helper`)
- [ ] "Helper: Unavailable — <detail>" row appears in the menu
- [ ] "Last error: …" row is visible and non-empty
- [ ] Tray icon appears dimmed (error variant)

### 6. Permission denied flow

- [ ] System Settings → Privacy & Security → Automation → Relay → toggle Music **off**
- [ ] Restart Relay (`cargo run`)
- [ ] Tray menu shows "Helper: Apple Music access denied" and "Last error: Apple Music access denied"
- [ ] "Open System Settings…" item is visible and enabled
- [ ] Click "Open System Settings…" — Automation pane opens in System Settings
- [ ] Re-enable Music in the Automation pane
- [ ] Play a track → all rows return to normal; "Open System Settings…" disappears; "Last error" clears

### 7. First-run notification

- [ ] Delete `~/Library/Application Support/relay/.permission-prompt-shown` (if it exists)
- [ ] Run `cargo run`
- [ ] macOS notification appears: "macOS will ask for permission to read Apple Music. Click OK to enable Relay." with title "Relay"
- [ ] Notification appears *before* any Apple Events permission prompt
- [ ] Subsequent launches of `cargo run`: no notification fires again
```

- [ ] **Step 2: Commit**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && git add TESTING.md && git commit -m "docs(testing): add status dashboard and onboarding manual checks"
```

---

## Task 10: Final verification and PR

- [ ] **Step 1: Run full verification suite**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo clippy -- -D warnings 2>&1
```

Expected: zero warnings.

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo fmt -- --check 2>&1
```

Expected: no diffs.

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo test 2>&1
```

Expected: all tests pass, including new tests:
- `parses_permission_denied_event`
- `playback_idle_row_text`, `playback_playing_row_text`, `playback_paused_row_text`
- `discord_connected_row_text`, `discord_reconnecting_row_text`, `discord_disconnected_row_text`
- `helper_running_row_text`, `helper_unavailable_row_text`, `helper_permission_denied_row_text`
- `icon_variant_normal_when_healthy`, `icon_variant_error_when_last_error_set`, etc.
- `permission_denied_then_track_changed_clears_helper_state`

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && cargo build --release 2>&1 | tail -5
```

Expected: release build succeeds.

- [ ] **Step 2: Smoke test — silent default**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && timeout 5 cargo run 2>&1 | wc -l
```

Expected: 0 lines (completely silent). Note: `timeout 5` will exit with code 124 which is expected — check the output count.

- [ ] **Step 3: Smoke test — RUST_LOG=info produces logs**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && timeout 5 sh -c 'RUST_LOG=info cargo run 2>&1' | head -10
```

Expected: lifecycle log lines visible.

- [ ] **Step 4: Create final commit consolidating into a single logical commit**

The individual task commits are already on the branch. Create the final squash commit:

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && git log --oneline 2>&1 | head -20
```

Review the commits made. If they look clean (one per task), proceed to the PR step. If there are stray or redundant commits, squash with:

```bash
# Count commits since main divergence
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && git log --oneline main..HEAD 2>&1
```

- [ ] **Step 5: Push and create PR**

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && git push -u origin worktree-status-dashboard-onboarding 2>&1
```

```bash
cd /Users/david/Desktop/relay/.claude/worktrees/status-dashboard-onboarding && gh pr create \
  --title "feat(tray): in-menu status dashboard and permission onboarding" \
  --body "$(cat <<'EOF'
## Summary

- Replaces the flat `TrayState` enum with a `TrayStatus` struct holding independent `PlaybackStatus`, `DiscordHealth`, and `HelperHealth` sub-enums, each rendered as a separate tray menu row.
- Adds a new `permission_denied` IPC event: the Swift helper emits it when AppleScript fails with error -1743 (errAEEventNotPermitted); the pipeline routes it to `HelperHealth::PermissionDenied` + an "Open System Settings…" menu affordance.
- Adds a first-run pre-prompt notification via `osascript` (fires once, gated by a marker file).
- Tracing is silent by default (`EnvFilter::new("off")`); set `RUST_LOG=info` to restore lifecycle logs.

Closes #19.
Closes #31.

## Verification

### Automated

```
cargo clippy -- -D warnings   # PASS
cargo fmt -- --check          # PASS
cargo test                    # PASS (all existing + new tests green)
cargo build --release         # PASS
```

### Smoke tests

- `cargo run` (5 s, no RUST_LOG) → **zero stdout/stderr lines**
- `RUST_LOG=info cargo run` (5 s) → lifecycle log lines appear as expected

### New unit tests added

- `parses_permission_denied_event` (media/event.rs)
- `playback_idle/playing/paused_row_text` (tray/mod.rs)
- `discord_connected/reconnecting/disconnected_row_text` (tray/mod.rs)
- `helper_running/unavailable/permission_denied_row_text` (tray/mod.rs)
- `icon_variant_*` suite (tray/mod.rs)
- `last_error_row_text_*` (tray/mod.rs)
- `permission_denied_then_track_changed_clears_helper_state` (pipeline.rs)
- Ported: `status_shows_discord_error_after_disconnect`, `status_no_discord_error_before_first_connection`, `status_returns_to_normal_after_discord_reconnect`, `status_helper_unavailable_sets_error` (pipeline.rs)

## Manual checks for reviewer

- [ ] **1. Status rows render** — launch app, click tray icon. See "Now Playing: Idle", "Discord: Connected", "Helper: Running" rows in order. Play a track → playback row updates. Pause → shows "Paused — …".
- [ ] **2. Silent stdout/stderr** — `cargo run` (no RUST_LOG) for 5 s with Apple Music not playing. Confirm zero lines on stdout/stderr.
- [ ] **3. Developer logging** — `RUST_LOG=info cargo run` for 5 s. Lifecycle logs appear.
- [ ] **4. Discord disconnect** — kill Discord while running. "Discord: Reconnecting in Xms" appears within ~2 s. Restart Discord → "Discord: Connected" returns.
- [ ] **5. Helper crash** — `pkill relay-helper`. "Helper: Unavailable — …" appears; "Last error" row populated; icon dimmed.
- [ ] **6. Permission denied flow** — toggle Music off in System Settings → Privacy → Automation → Relay. Restart Relay. Menu shows "Helper: Apple Music access denied" + "Last error: …" + "Open System Settings…". Click it → Automation pane opens. Re-enable Music → state clears on next successful track event.
- [ ] **7. First-run notification** — delete `~/Library/Application Support/relay/.permission-prompt-shown`. Run Relay. macOS notification appears before OS permission prompt. Subsequent launches: no notification.

## Deviations from spec

- `PlaybackStatus::Paused` row uses `"Paused — <title> — <artist>"` format (em-dash U+2014) matching the spec mockup.
- The `details_item` from the old menu (which showed error detail below the status row) is replaced by the dedicated "Last error: …" row — same information, cleaner structure.
- Old `TrayState::from_helper_status()` removed (was only used inside the old `TrayHealth`).
EOF
)"
```

- [ ] **Step 6: Confirm PR URL printed and record it**

---

## Self-Review Against Spec

### Spec Coverage Check

| Spec requirement | Task |
|---|---|
| `TrayStatus` struct with PlaybackStatus / DiscordHealth / HelperHealth | Task 5 |
| `row_text()` on each sub-enum | Task 5 |
| `icon_variant()` — Error on last_error, PermissionDenied, discord disconnect after first connect | Task 5 |
| `MediaEvent::PermissionDenied` variant + parse test | Task 2 |
| Helper emits `permission_denied` on error -1743 | Task 3 |
| Pipeline routes `PermissionDenied` to `status.helper = PermissionDenied` + `last_error` | Task 6 |
| Clear helper/last_error on subsequent track/pause/stop | Task 6 |
| `permission_denied_then_track_changed_clears_helper_state` test | Task 6 |
| Port old `TrayHealth` tests onto `TrayStatus` | Task 6 |
| New menu layout with playback/discord/helper/last-error rows | Task 7 |
| `UserEvent::StatusUpdate(TrayStatus)` replaces `StateUpdate(TrayState)` | Task 7 |
| "Open System Settings…" item toggled by PermissionDenied | Task 7 |
| `std::process::Command::new("open").arg(SYSTEM_SETTINGS_AUTOMATION_URL)` on click | Task 7 |
| Preserve Display submenu behavior | Task 7 |
| `config::data_dir()` | Task 4 |
| `maybe_show_first_run_notification()` in `src/onboarding.rs` | Task 4 |
| Marker file `~/Library/Application Support/relay/.permission-prompt-shown` | Task 4 |
| Silent tracing default — `EnvFilter::new("off")` | Task 8 |
| Call `maybe_show_first_run_notification()` in `main()` before pipeline | Task 8 |
| New constants: TRAY_DISCORD_LABEL_CONNECTED, TRAY_HELPER_LABEL_RUNNING, etc. | Task 1 |
| `TRAY_PERMISSION_DENIED_DETAIL` constant | Task 1 |
| `SYSTEM_SETTINGS_AUTOMATION_URL` constant | Task 1 |
| `FIRST_RUN_NOTIFICATION_OSASCRIPT` constant | Task 1 |
| `TESTING.md` — 7 manual scenarios | Task 9 |
| No unwrap/expect in prod paths | All tasks |
| No magic strings — constants only | Task 1 + all tasks |
| No println — tracing only | All tasks |
| IPC fields are strings only | Task 2 (PermissionDenied has no extra fields) |

All spec requirements are covered.

### Placeholder Scan

No TBDs, TODOs, or "similar to" references. All code is explicit.

### Type Consistency

- `TrayStatus` defined in Task 5, used in Tasks 6, 7, 8.
- `PlaybackStatus`, `DiscordHealth`, `HelperHealth` defined in Task 5, used throughout.
- `UserEvent::StatusUpdate(TrayStatus)` defined in Task 7, used in pipeline.rs (Task 6) and main.rs (Task 8).
- `TRAY_PERMISSION_DENIED_DETAIL` defined in Task 1, used in Tasks 6 and 9.
- `data_dir()` defined in Task 4, used in `onboarding.rs` (Task 4).

All consistent.
