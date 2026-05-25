# Manual Testing — Swift Helper

Prerequisites: macOS, Apple Music, Xcode CLI tools (`swiftc`).

## Build
- [ ] `cargo build --release` (helper compiled via build.rs)
- [ ] Confirm binary exists at path printed by `build.rs` / `RELAY_HELPER_PATH`

## Event emission
- [ ] Run helper directly: `<path>/relay-helper`
- [ ] Play a track in Apple Music → stdout receives one `track_changed` JSON line with title/artist/album
- [ ] Pause → stdout receives `playback_paused`
- [ ] Stop/quit playback → stdout receives `playback_stopped`
- [ ] Skip rapidly between tracks → one line per change (no partial JSON)

## stdout/stderr separation
- [ ] stderr contains `[relay-helper]` diagnostic lines
- [ ] stdout contains **only** valid JSON lines (pipe through `jq .` — every line parses)

## Shutdown
- [ ] SIGINT (Ctrl+C) → clean exit, no zombie process

## Discord RPC
- [ ] With Discord running, app sets activity and Discord profile shows **"Listening to"** not "Playing"
- [ ] Track title appears as details, "Artist · Album" as state
- [ ] Artwork visible (large image) when available

## Tray

- [ ] App launches, **visible** menu bar icon (template glyph) in light and dark menu bar
- [ ] **No Dock icon** while running (`cargo run` uses accessory activation policy)
- [ ] Menu opens on click; tooltip shows "Relay"
- [ ] "Now Playing: ..." shows correct track info when playing
- [ ] "Relay: Idle" shows when nothing playing
- [ ] Toggle **Enabled** off → dimmed icon + "Relay: Disabled"
- [ ] "Relay: media access unavailable" on helper error + greyed **details** line (e.g. exit code)
- [ ] Quit Discord while enabled → error icon + "Relay: discord unavailable" + detail line; reopen Discord → icon returns to normal

## End-to-end

Prerequisites: macOS, Apple Music open, Discord running, app built with `cargo build`.

- [ ] Launch app: `./target/debug/relay`
- [ ] Tray icon appears in menu bar
- [ ] Play a track in Apple Music → Discord profile shows **"Listening to"** with artwork after ~1.5s
- [ ] Pause → Discord status clears
- [ ] Skip tracks rapidly → only final track shown (debounce works)
- [ ] Toggle **Enabled** off via menu → Discord clears, events stop
- [ ] Toggle **Enabled** on → resumes
- [ ] Kill helper: `pkill relay-helper` → error icon, status + details lines, no restart loop
- [ ] Quit Discord, wait 5s, reopen → app reconnects and re-publishes active track
