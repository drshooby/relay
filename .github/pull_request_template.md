## Summary
<!-- What this PR does -->

## Test plan
<!-- Automated -->
- [ ] `cargo fmt --check`
- [ ] `cargo clippy -- -D warnings`
- [ ] `cargo test`

## Manual E2E (required for integration PR — all must be checked)
- [ ] Play track in Apple Music → Discord shows **"Listening to"** with artwork after ~1.5s debounce
- [ ] Pause → Discord status clears
- [ ] Skip tracks rapidly → only final track appears (no flash of skipped tracks)
- [ ] Toggle **Enabled** off → Discord clears immediately; events ignored
- [ ] Toggle **Enabled** on → resumes; current track picked up if playing
- [ ] Kill Swift helper process → tray shows error badge + "Relay: media access unavailable"; no restart loop
- [ ] Discord closed mid-session → reconnects with backoff; re-publishes active track when Discord reopens

Full steps: see [TESTING.md](TESTING.md#end-to-end).
