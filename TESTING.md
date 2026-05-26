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
- [ ] Menu shows status line, optional error detail line, and **Quit Relay** (no enable toggle)
- [ ] "Now Playing: ..." shows correct track info when playing
- [ ] "Relay: Idle" shows when nothing playing
- [ ] "Relay: media access unavailable" on helper error + greyed **details** line (e.g. exit code); icon appears **dimmed**
- [ ] Quit Discord → dimmed icon + "Relay: discord unavailable" + detail line; reopen Discord → icon returns to normal
- [ ] Error dimming is legible in both light and dark menu bar appearances

## End-to-end

Prerequisites: macOS, Apple Music open, Discord running, app built with `cargo build`.

- [ ] Launch app: `./target/debug/relay`
- [ ] Tray icon appears in menu bar
- [ ] Play a track in Apple Music → Discord profile shows **"Listening to"** with artwork after ~1.5s
- [ ] Pause → Discord status clears
- [ ] Skip tracks rapidly → only final track shown (debounce works)
- [ ] Kill helper: `pkill relay-helper` → dimmed icon, status + details lines, no restart loop
- [ ] Quit Discord, wait 5s, reopen → app reconnects and re-publishes active track
- [ ] **Quit Relay** from menu → app exits cleanly

## Resume position accuracy (#37)

Prerequisites: macOS, Apple Music open, Discord running, second Discord account visible for observation.

- [ ] **#37** — Play a track → scrub to ~1:30 → pause → resume → Discord card on second account shows approximately 1:30 (within ±3 s) and counting forward within 3 s of resume
- [ ] **#37b** — Play a track from the beginning → pause at some mid-track position (without scrubbing) → resume → Discord card shows the correct mid-track position (not 0:00)
- [ ] After the above tests, confirm stderr contains `[relay-helper] Music.playerInfo.resume: track_changed` (AppleScript path was used, not the stale MPNowPlayingInfoCenter path)

## Music.app quit / launch detection (#33)

Prerequisites: macOS, Apple Music playing a track, Discord showing active "Listening to" card.

- [ ] **#33** — While a track is playing and Discord shows activity, press ⌘Q in Music.app → Discord activity card clears within ~1 s; stderr contains `[relay-helper] Music.app terminated → playback_stopped`
- [ ] **#33b** — With stale Discord activity visible (e.g. from a previous session or a force-quit scenario), launch Music.app without playing anything → Discord activity clears; stderr contains `[relay-helper] Music.app launched → playback_stopped (resets stale state)`
- [ ] After Music.app relaunches and playback begins, Discord shows the new track correctly (NSWorkspace launch event did not break subsequent play detection)
