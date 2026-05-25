# Relay — Claude Code Rules

See `design-doc.md` for the product spec.

## Project Structure

```
relay/
├── src/
│   ├── main.rs          — wiring only, no business logic
│   ├── pipeline.rs      — async pipeline (run_pipeline, AppCommand)
│   ├── config.rs        — config read/write
│   ├── constants.rs     — ALL named constants
│   ├── discord/         — RPC client + activity payloads
│   ├── media/           — Swift helper subprocess + event parsing
│   ├── artwork/         — iTunes API + cache
│   └── tray/            — menu bar UI state + winit event loop
└── helper/              — Swift command-line tool (separate target)
    └── Sources/main.swift
```

## Hard Rules — Never Violate

- No `unwrap()` / `expect()` in production paths — tests and truly unreachable paths only (add a comment)
- No `println!` for logging — use `tracing` exclusively
- No blocking calls inside async Tokio tasks — use `tokio::task::spawn_blocking`
- No magic numbers or strings inline — everything goes in `src/constants.rs`
- No `unsafe` blocks
- No `features = ["full"]` on Tokio — enumerate only needed features
- No queuing debounced events — abort + restart timer (`JoinHandle::abort()`)
- No writing to stdout in the Swift helper except JSON events
- Never commit directly to `main`

## IPC Contract (Rust ↔ Swift)

Bidirectional newline-delimited JSON. All fields are strings — no numbers, booleans, or nested objects.

### Outbound (Swift → Rust over stdout)

| Field | Type | Present when |
|---|---|---|
| `event` | string | always |
| `title` / `artist` / `album` | string | `track_changed` only |

Valid `event` values: `track_changed`, `playback_paused`, `playback_stopped`.

- Unrecognised `event` → silently ignore (forward compat)
- Malformed JSON → log warning, skip line, never crash
- Omit optional fields instead of sending empty strings

### Inbound (Rust → Swift over stdin)

| Field | Type | Present when |
|---|---|---|
| `command` | string | always |

Valid `command` values: `refresh` (re-query Music.app and emit the current state).

- Unrecognised `command` → log to stderr, ignore
- Malformed JSON → log to stderr, skip line, never crash

## Rust Standards

### Error Handling
- `thiserror` for module-level typed enums; `anyhow` only at top-level in `main.rs`
- Always propagate with `?` — never swallow silently
- Error messages: lowercase, no trailing period, actionable
- Log errors at the boundary where caught, not where created

### Async / Tokio
- Feature set: `["rt-multi-thread", "macros", "process", "io-util", "time", "sync"]`
- Prefer `tokio::sync::mpsc` over `Arc<Mutex<T>>` for cross-task events
- One task per concern (media watcher, Discord RPC, etc.)
- Debounce pattern — abort, don't queue:
  ```rust
  if let Some(h) = pending.take() { h.abort(); }
  pending = Some(tokio::spawn(async move { /* ... */ }));
  ```

### Logging (`tracing` only)
- `info!` — lifecycle events (connected, track changed, status cleared)
- `warn!` — recoverable issues (API miss, artwork fallback)
- `error!` — functional failures (helper crash, RPC disconnect)
- `debug!` — verbose output, gated by `RUST_LOG=debug`
- Never log user data (track titles, artist names) above `debug`

### Workflow
- `cargo clippy -- -D warnings` before every commit
- `cargo fmt` — always
- Pin dependency versions in `Cargo.toml`
- Unit-test pure logic (cache TTL, URL manipulation, JSON parsing)

## Swift Helper Standards

- **stdout** — JSON events ONLY, never anything else
- **stderr** — diagnostics via `fputs("[relay-helper] ...\n", stderr)`
- `fflush(stdout)` after every emit — subprocess stdout is fully buffered
- Signal handlers before `RunLoop.main.run()`:
  ```swift
  signal(SIGTERM) { _ in exit(0) }
  signal(SIGINT)  { _ in exit(0) }
  ```
- Use `RunLoop.main.run()` — not `dispatchMain()`
- `MPNowPlayingInfoCenter` / `MPRemoteCommandCenter` observation on main thread only
- No third-party dependencies — Foundation and MediaPlayer only

## macOS Event Loop Constraint

`tray-icon` requires an NSApplication event loop on the main thread:

- `main()` must NOT be `#[tokio::main]`
- winit `EventLoop` runs on main thread; Tokio runtime on a background `std::thread`
- `TrayIcon` must be created inside the running event loop (`resumed()` / `StartCause::Init`)
- State updates: Tokio → main via `EventLoopProxy::send_event`
- Commands: main → Tokio via `tokio::sync::mpsc` with `blocking_send`

## Directory Creation

Always `create_dir_all` before file writes:

```rust
std::fs::create_dir_all(&dir).context("failed to create relay data directory")?;
```

## Git Hygiene

- Commit format: `<type>(<scope>): <description>` — e.g. `feat(artwork): add 600x600 fallback`
- Types: `feat`, `fix`, `chore`, `docs`, `refactor`, `test`
- One logical change per commit
- PRs reference issues: `Closes #N`
