# Spec — Sub-project #5 (Part A): Status dashboard + permission onboarding (#19, #31)

## Context

Sub-project #5 of the Relay roadmap, split into two PRs. This is Part A — the menu-bar UX layer that turns Relay from "developer with terminal" into "end user with menu bar." Part B (SwiftUI preferences window — issue #20) is deferred to a follow-up, since the display toggles from sub-project #3 already cover most of #20's original scope.

Two issues bundled because they share tray plumbing:
- **#19** — replace terminal debug output with rich in-menu status (Discord, helper, last error).
- **#31** — first-run onboarding for the Apple Events permission prompt + recovery affordance if denied.

## Design

### Part 1 — #19 Status dashboard

#### Tray menu structure (after)

```
Now Playing: <title> — <artist>      (or "Idle", or "Paused — <title> — <artist>")
─────────────
Discord:     Connected               (or "Reconnecting in 1200ms", "Disconnected")
Helper:      Running                 (or "Unavailable", "Apple Music access denied")
─────────────
Last error:  <short message>         (only when set; clears on recovery)
─────────────
Display ▸                            (existing submenu — keep)
─────────────
Open System Settings…                (only when helper == PermissionDenied)
─────────────
Quit Relay
```

Status rows are disabled menu items (cosmetic). They update in place via `MenuItem::set_text`.

#### Refactor `TrayState` → `TrayStatus`

Today `tray::TrayState` is an enum mashing playback + error states into one label. Promote to a struct with independent fields:

```rust
// src/tray/mod.rs
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrayStatus {
    pub playback: PlaybackStatus,    // Idle | Playing { title, artist } | Paused { title, artist }
    pub discord:  DiscordHealth,     // Connected | Reconnecting { backoff_ms } | Disconnected { detail }
    pub helper:   HelperHealth,      // Running | Unavailable { detail } | PermissionDenied
    pub last_error: Option<String>,  // most recent transient error (helper crash, RPC fail)
}
```

Each sub-enum implements a `fn label(&self) -> String` (or `.row_text(&self)`). The icon variant becomes `TrayStatus::icon_variant(&self) -> TrayIconVariant` — picks `Error` if `last_error` is `Some` OR `helper == PermissionDenied` OR `discord` is in a non-Connected state for more than one tick; otherwise `Normal`.

Existing tests in `tray/mod.rs` and `pipeline.rs` (the `TrayHealth` tests) must be updated to reflect the new struct. The `TrayHealth` struct in `pipeline.rs` may collapse into `TrayStatus` directly — likely cleaner; verify during implementation.

#### Pipeline plumbing (`src/pipeline.rs`)

`run_pipeline` already receives helper status, discord status, and media events. Today it folds them into a single `TrayState` via `TrayHealth.resolved()`. Replace with `TrayStatus` updates that change *only the affected field* (no `resolved()` collapsing). Send a `UserEvent::StatusUpdate(TrayStatus)` to the event loop on each change.

#### Silent stdout/stderr by default

`tracing_subscriber` is currently initialized with `EnvFilter::from_default_env()` somewhere in `main.rs`. When `RUST_LOG` is unset, default to `EnvFilter::new("off")` (or `"error"` if you want crashes visible). Developer workflow (`RUST_LOG=info cargo run`) still works.

Helper-side: the helper still writes `[relay-helper]` lines to stderr — that's fine when launched from a terminal. Inside `.app` execution, stderr lands in `Console.app` (under the Relay process), not in any visible terminal window. No change needed there.

### Part 2 — #31 Permission onboarding

#### IPC contract extension

Add a new event to the contract (per CLAUDE.md "Outbound" table):

| `event` value | Fields | When |
|---|---|---|
| `permission_denied` | (none) | Helper's AppleScript snapshot fails with errAEEventNotPermitted (-1743) |

Update `src/media/event.rs::MediaEvent`:

```rust
#[serde(tag = "event", rename_all = "snake_case")]
pub enum MediaEvent {
    // existing variants...
    PermissionDenied,
}
```

Add a parse test.

#### Helper change (`helper/Sources/main.swift::emitCurrentState`)

Today the AppleScript error branch logs and returns. Promote -1743 specifically:

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

The constant `-1743` and key name `NSAppleScript.errorNumber` are stable Apple API.

#### Pipeline routes `PermissionDenied`

In the raw `event_rx` arm in `pipeline.rs`, treat `MediaEvent::PermissionDenied` as a helper health update:

```rust
MediaEvent::PermissionDenied => {
    status.helper = HelperHealth::PermissionDenied;
    status.last_error = Some(TRAY_PERMISSION_DENIED_DETAIL.into());
    send_status(status.clone());
}
```

When a successful event subsequently arrives (track_changed / playback_paused / playback_stopped from a recovered AppleScript run), clear the permission-denied state:

```rust
if matches!(status.helper, HelperHealth::PermissionDenied) {
    status.helper = HelperHealth::Running;
    status.last_error = None;
}
```

#### "Open System Settings…" menu item

Conditionally inserted when `helper == PermissionDenied`. Action: `Command::new("open").arg(SYSTEM_SETTINGS_AUTOMATION_URL).status()` where the URL is `x-apple.systempreferences:com.apple.preference.security?Privacy_Automation`. Constant in `src/constants.rs`.

The tray-icon crate's `Menu` is largely static — adding/removing items at runtime requires rebuilding. Pragmatic alternative: always include the item in the menu but `set_enabled(false)` + `set_text("")` when not in PermissionDenied state. The empty text + disabled state makes it invisible-ish. Choose whichever is cleaner; the spec doesn't mandate either.

#### First-run pre-prompt notification

Detect first run by checking if `~/Library/Application Support/relay/.permission-prompt-shown` exists. If absent, fire a notification before `run_pipeline` spawns the helper:

```rust
// src/main.rs (or a new src/onboarding.rs)
fn maybe_show_first_run_notification() {
    let marker = config::data_dir().join(".permission-prompt-shown");
    if marker.exists() { return; }

    let _ = std::process::Command::new("osascript")
        .args([
            "-e",
            "display notification \"macOS will ask for permission to read Apple Music. Click OK in the prompt to enable Relay.\" with title \"Relay\" sound name \"Funk\""
        ])
        .status();

    // Create marker (best-effort; failure is non-fatal).
    let _ = std::fs::create_dir_all(marker.parent().unwrap());
    let _ = std::fs::write(&marker, "");
}
```

`osascript display notification` is the menubar-app standard — no notification entitlement needed, integrates with Notification Center. Fired from Rust before the helper subprocess starts, so the user sees Relay's notification *just before* the OS Automation prompt appears.

Add a helper `config::data_dir() -> PathBuf` (or reuse the existing path logic — `config_path().parent()`).

## Critical files

- `src/tray/mod.rs` — `TrayStatus` struct + sub-enums + `label()` impls + `icon_variant`
- `src/tray/event_loop.rs` — add Discord/Helper/Error rows, conditional Settings item, update on `StatusUpdate`
- `src/pipeline.rs` — replace `TrayHealth` with `TrayStatus`, route `PermissionDenied`, clear-on-recovery
- `src/media/event.rs` — `MediaEvent::PermissionDenied` + parse test
- `helper/Sources/main.swift` — emit `permission_denied` on -1743
- `src/main.rs` — silent default tracing; `maybe_show_first_run_notification()` before pipeline spawns
- `src/constants.rs` — new labels (`TRAY_DISCORD_LABEL`, `TRAY_HELPER_LABEL`, `TRAY_LAST_ERROR_LABEL`, `TRAY_OPEN_SETTINGS_LABEL`, `TRAY_PERMISSION_DENIED_DETAIL`, `SYSTEM_SETTINGS_AUTOMATION_URL`)
- `TESTING.md` — manual checklist for onboarding scenarios

## Tests

### Unit
- `tray/mod.rs`: row-text rendering for each sub-enum state. `icon_variant` picks Error when `last_error` is set or `helper == PermissionDenied`.
- `media/event.rs`: `parses_permission_denied_event`.
- `pipeline.rs`: refactor existing `TrayHealth` tests onto `TrayStatus`. Add: `permission_denied_then_track_changed_clears_helper_state`.

### Manual (document in PR; user runs)
1. **Status rows render**: launch app, click tray. See "Discord: Connected", "Helper: Running" rows. Play track → "Now Playing" row updates. Pause → "Paused" row.
2. **Silent stdout/stderr**: `./Relay.app/Contents/MacOS/Relay` (no `RUST_LOG`). Confirm zero lines on stdout/stderr.
3. **Developer logging still works**: `RUST_LOG=info ./Relay.app/Contents/MacOS/Relay` shows the usual lifecycle lines.
4. **Discord disconnect surfaces**: kill Discord while running. "Discord: Reconnecting in 1200ms" appears in menu within ~2 s. Restart Discord. "Discord: Connected" returns.
5. **Helper crash surfaces**: `pkill relay-helper`. "Helper: Unavailable" appears.
6. **Permission denied flow**:
   - System Settings → Privacy & Security → Automation → Relay → toggle Music **off**.
   - Restart Relay. Menu shows "Helper: Apple Music access denied" + "Last error: …" + "Open System Settings…" item appears.
   - Click "Open System Settings…" — automation pane opens.
   - Re-enable Music. Play a track. State clears, normal status returns.
7. **First-run notification**:
   - Delete `~/Library/Application Support/relay/.permission-prompt-shown`.
   - Restart Relay. macOS notification appears with the pre-prompt copy *before* the system Automation prompt.
   - Subsequent launches: no notification.

## Out of scope

- SwiftUI preferences window (#20) — deferred to Part B.
- Login-item integration — depends on #20.
- Debounce-duration setting — depends on #20.
- Notification sound customization.

## Execution

- Worktree: `.claude/worktrees/status-dashboard-onboarding` (already entered) on branch `worktree-status-dashboard-onboarding`.
- One Sonnet subagent. This is the largest sub-project to date — the subagent should TaskCreate a checklist from the implementation tasks.
- PR title: `feat(tray): in-menu status dashboard and permission onboarding`. Body: `Closes #19. Closes #31.`
