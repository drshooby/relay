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
