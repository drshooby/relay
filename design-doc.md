# V4 Design Document
## Relay — Apple Music → Discord Rich Presence

---

## Overview

A lightweight macOS menu bar daemon written in Rust that listens for Apple Music playback events and reflects the currently playing track as a Discord Rich Presence status. When music stops or pauses, the status is cleared. The app is open source, requires no backend, and runs entirely on the user's machine.

---

## Goals

- Show the currently playing Apple Music track on the user's Discord profile automatically
- Zero manual interaction after initial setup
- Open source, no telemetry, no accounts, no backend
- Mac-only for v1

## Non-Goals (v1)

- Spotify or any other player support
- Windows or Linux support
- Scrobbling (Last.fm etc.)
- "Listen along" functionality
- Animated album art
- A settings UI

---

## User Story

> As someone who listens to Apple Music, I want my Discord status to automatically show what I'm playing — with track name, artist, and album art — so my friends can see it without me doing anything manually.

---

## Happy Path Flow

```
1. App launches → sits in menu bar, connects to Discord RPC socket
2. User plays a track in Apple Music
3. Swift helper fires a playback event → app receives track metadata
4. Event debounce window (1.5s) passes — confirms track is stable, not a rapid skip
5. App queries iTunes Search API with artist + track title → gets artwork URL
6. App sets Discord Rich Presence:
     - Activity type: "Listening to"
     - Line 1: track title
     - Line 2: artist — album
     - Large image: artwork URL (600x600, falling back to 100x100)
     - Large image tooltip: album name
     - Timestamps: system time at track event (Discord counts up automatically)
7. User pauses → event fires → app clears the Discord status
8. Track changes → repeat from step 3
```

---

## Architecture

### Components

```
┌─────────────────────────────────────────────────┐
│                   macOS Process                 │
│                                                 │
│  ┌─────────────┐     ┌──────────────────────┐  │
│  │ Swift Media │────▶│   Event Handler      │  │
│  │   Helper   │     │   (Rust async task)  │  │
│  │  (bundled  │     └──────────┬───────────┘  │
│  │ subprocess)│                │               │
│  └─────────────┘           debounce            │
│   stdout/stdin                (1.5s)            │
│   or local socket                │               │
│                                  ▼               │
│                       ┌──────────────────────┐  │
│                       │   iTunes Search API  │  │
│                       │   (artwork lookup +  │  │
│                       │    URL cache)        │  │
│                       └──────────┬───────────┘  │
│                                  │               │
│                                  ▼               │
│                       ┌──────────────────────┐  │
│                       │   Discord RPC Client │  │
│                       │   (local IPC socket) │  │
│                       └──────────────────────┘  │
│                                                 │
│  ┌─────────────┐                                │
│  │  Menu Bar   │── status display               │
│  │  (tray UI)  │── quit                         │
│  └─────────────┘                                │
└─────────────────────────────────────────────────┘
```

### Key Crates

| Crate | Purpose |
|---|---|
| Swift media helper (bundled binary) | macOS media event source — observes `MPNowPlayingInfoCenter` / `MPRemoteCommandCenter` via public Apple APIs, forwards events to Rust over IPC |
| `discord-rich-presence` or raw IPC | Discord RPC socket communication |
| `tokio` | Async runtime |
| `reqwest` | iTunes Search API HTTP calls |
| `serde` / `serde_json` | JSON parsing (iTunes API response, RPC payloads, IPC messages) |
| `tray-icon` | macOS menu bar icon + status menu |
| `dirs` | Resolving config/cache paths (`~/Library/Application Support/`) |

---

## macOS Media Detection

### Chosen Approach: Bundled Swift Helper Binary

A small Swift executable is compiled and bundled inside the Relay app package. Rust spawns it as a subprocess on startup and communicates with it over stdout/stdin (newline-delimited JSON events). The Swift helper uses **public, documented Apple APIs** to observe playback state and forwards events to Rust as they occur.

**APIs used in the Swift helper:**
- `MPNowPlayingInfoCenter` — reads current track metadata (title, artist, album, duration)
- `MPRemoteCommandCenter` — observes playback state changes (play, pause, stop, track change)
- Both are part of the `MediaPlayer` framework, fully public and supported by Apple

**Why this over `mediaremote-rs`:**

| Approach | Event-driven | Public API | macOS 15.4+ | Notarization safe | Notes |
|---|---|---|---|---|---|
| Swift helper (`MediaPlayer`) | Yes | Yes | Yes | Yes | Recommended |
| `mediaremote-rs` | Yes | No | Yes (Perl hack) | No | Private framework, borrowed time |
| `osascript` / JXA | No (polling) | Yes | Yes | Yes | Ruled out — no events |

**IPC protocol:** The Swift helper writes newline-delimited JSON to stdout. Rust reads it line by line via `tokio::process`. Example event:

```json
{"event": "track_changed", "title": "Happier Than Ever", "artist": "Billie Eilish", "album": "Happier Than Ever"}
{"event": "playback_paused"}
{"event": "playback_stopped"}
```

**Build:** The Swift helper is a separate target in the repo, compiled with `swiftc` as part of the build process. The resulting binary is bundled into the Relay `.app` package at a known relative path.

### Contingency & Degradation Path

If the Swift helper fails to launch or crashes, Rust detects the subprocess exit, logs the failure, shows a clear error state in the menu bar ("Relay: media access unavailable"), and does not attempt to restart in a tight loop. Public Apple APIs are stable and this scenario is unlikely, but it is handled explicitly.

### Event Debouncing

All track change events are debounced by **1.5 seconds** before triggering network calls or RPC updates. This prevents rate-limiting the iTunes API and avoids flashing intermediate statuses when a user rapidly skips tracks. The debounce duration is defined as a named constant (`TRACK_DEBOUNCE_MS`) and is a candidate for a user-configurable option in v2.

**Cancellation behaviour:** If a new track event fires within the debounce window, the pending task must be actively cancelled (via `tokio` `AbortHandle` or by aborting the existing `JoinHandle`) before the new debounce timer starts. Events are not queued — only the most recent event matters. This ensures stale API calls for skipped tracks are never dispatched.

---

## Album Art

### Strategy

1. When a new track event fires and passes the debounce window, check the in-memory cache (keyed by `artist + album`) for a known artwork URL
2. On cache miss, call the iTunes Search API:
   ```
   https://itunes.apple.com/search?term={artist}+{track}&entity=musicTrack&limit=5
   ```
3. Extract `artworkUrl100` from the best matching result
4. Attempt to replace `100x100` with `600x600` in the URL for a higher resolution image
5. If the 600x600 URL returns a 404 or fails to load, fall back to the original `100x100` URL
6. Pass the final URL directly into the Discord RPC `large_image` field (Discord supports external URLs here)
7. Cache the result

### Fallback

If the iTunes API returns no result or times out, omit the image field entirely. No default placeholder for v1.

### Caching

- **In-memory:** `HashMap<(artist, album), ArtworkUrl>` — fast lookup within a session
- **On-disk:** `~/Library/Application Support/relay/artwork_cache.json`, TTL of 30 days per entry
- **Pruning strategy:** Lazy eviction on read — when an entry is loaded from disk, its timestamp is checked. If it exceeds the 30-day TTL it is discarded and a fresh API call is made. No artwork data is stored locally, only the URL.

---

## Discord RPC

### Connection

Discord exposes a local Unix socket at `$TMPDIR/discord-ipc-0` (macOS). The app connects on startup and maintains a persistent connection. On disconnect (Discord closed), the app retries with exponential backoff and resumes automatically when Discord reopens.

### Activity Payload

```json
{
  "activity_type": 2,
  "details": "Track Title",
  "state": "Artist · Album",
  "timestamps": {
    "start": 1234567890
  },
  "assets": {
    "large_image": "https://is1-ssl.mzstatic.com/image/...",
    "large_text": "Album Name"
  }
}
```

Activity type `2` = "Listening to", which produces the correct label on the Discord profile card.

**Timestamps:** The app captures the system time when the track event fires and passes it as the `start` value. Discord automatically counts up from this value, providing a basic elapsed time indicator. Seek-aware precision (adjusting elapsed time when a user scrubs mid-song) is deferred to v2.

### Discord Application

A Discord application will be registered by the maintainer and its `client_id` will be hardcoded into the binary. This removes all setup friction — users do not need to visit the Discord Developer Portal. The `client_id` controls the app name shown in the Discord status ("Listening to **Relay**").

---

## Persistence & State

The app is stateless between runs — it reads whatever is currently playing when it starts, then reacts to events. Nothing needs to be stored beyond:

| What | Where | Format |
|---|---|---|
| Artwork URL cache | `~/Library/Application Support/relay/artwork_cache.json` | JSON |

The Discord `client_id` is hardcoded in v1 and does not appear in the config file. A `config.toml` path is reserved for future settings but is unused in v1.

---

## Lifecycle & Resilience

| Scenario | Behaviour |
|---|---|
| App starts, Discord not running | Wait and retry connection with exponential backoff |
| Discord closes mid-session | Detect socket disconnect, clear activity, retry |
| Apple Music closes | Swift helper fires stop event → clear activity |
| Swift helper fails to launch or crashes | Log error, show error state in menu bar, do not restart in tight loop |
| User quits Relay | Clear Discord activity and exit |
| Mac restarts | App does not auto-launch in v1 (user adds to Login Items manually) |
| iTunes Search API fails | Skip artwork, set status without image |
| 600x600 artwork URL 404s | Fall back to 100x100 URL |
| Track metadata incomplete | Set whatever fields are available, omit empty ones |
| Rapid track skipping | Debounce absorbs intermediate events, only final track triggers update |

---

## Menu Bar UI

Minimal. Single icon in the menu bar with a dropdown:

```
● Now Playing: Happier Than Ever — Billie Eilish   (informational)
  helper exited with code 1                        (error detail, when applicable)
─────────────────────────────────────────────────
  Quit Relay
```

When idle, the status line shows "Relay: Idle". When the Swift helper or Discord fails, the status and detail lines show an error; the icon dims to signal degraded state.

### Icon States

The menu bar icon itself reflects app state so the user never needs to click in to know something is wrong:

| State | Icon |
|---|---|
| Healthy (idle or playing) | Full-opacity template icon |
| Error (helper or Discord failure) | Dimmed template icon (reduced alpha) |

No preferences window for v1.

---

## Configuration File (v1)

No user-facing config in v1. `~/Library/Application Support/relay/config.toml` may exist from earlier builds; unknown keys are ignored. Settings will land here in a future version.

---

## Open Questions / Deferred to v2

- **Auto-launch on login** — deferred; user sets up manually via Login Items in v1 to avoid macOS code-signing and notarization requirements
- **Precise seek-aware timestamps** — v1 includes basic start-time tracking; adjusting elapsed time dynamically when a user scrubs mid-song is v2
- **Debounce configurability** — `TRACK_DEBOUNCE_MS` is a hardcoded constant in v1; expose as a config option in v2
- **Code signing / notarization** — needed for clean distribution; deferred
- **Homebrew formula** — natural distribution path for open source macOS tooling
- **"Listen to this song" button** — iTunes deep link in the status; easy add for v2
- **Animated album art** — Music Presence supports this via a separate service; skip for v1

---

## v1 Scope Summary

**In scope:**
- Apple Music playback detection (play, pause, track change) via bundled Swift helper using public `MediaPlayer` framework APIs
- 1.5s event debounce before triggering network/RPC updates
- Discord Rich Presence status with title, artist, album, and artwork
- Album art via iTunes Search API — 600x600 with 100x100 fallback
- In-memory + on-disk artwork URL caching with lazy TTL eviction
- Hardcoded Discord `client_id` — no developer portal setup required
- Persistent Discord RPC connection with exponential backoff reconnect
- Menu bar icon with now-playing display, error detail line, and quit
- Icon state reflects health (normal or dimmed error)
- Graceful degradation if Swift helper fails
- Open source, MIT licensed

**Out of scope:**
- Any other music player
- Windows / Linux
- Preferences UI
- Auto-launch
- Code signing
- Scrobbling, listen-along, animated art, seek-aware timestamps
