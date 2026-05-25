# Relay

A macOS menu bar app that shows your Apple Music activity on Discord as "Listening to". No backend, no telemetry, no accounts — runs entirely on your machine.

<p align="center">
  <img src="./assets/icons/relay.png" alt="relay logo">
</p>

## What it does

- Watches Apple Music playback via a bundled Swift helper
- Debounces events (1.5s) to avoid flashing status on rapid track skips
- Looks up album artwork from iTunes Search API, caches locally
- Sets Discord Rich Presence: "Listening to" with track title, artist · album, artwork, and timestamp
- Clears status on pause/stop
- Menu bar icon (normal when healthy, dimmed on error) with status display and quit; runs as a menu-bar accessory (no Dock icon)

## Architecture

Swift helper (stdout JSON) → Media reader → Debouncer → Artwork cache + iTunes Search → Discord RPC

## Build

Requirements: macOS, Rust toolchain, Xcode CLI tools (for `swiftc`).

Install Xcode CLI tools if needed:

    xcode-select --install

Build:

    cargo build --release

The Swift helper is compiled automatically by `build.rs` — no separate script needed.

## Run

    ./target/release/relay

The app appears in your menu bar (no Dock tile). Make sure Discord is running before launching.

When packaged as `Relay.app`, use [`packaging/Info.plist`](packaging/Info.plist) (`LSUIElement`) alongside the winit accessory policy.

## Login Items (auto-launch)

To launch Relay automatically at login:
1. Open System Settings → General → Login Items
2. Click + and add the `relay` binary

(macOS will prompt for permission on first launch.)

## Manual Testing

See [TESTING.md](TESTING.md) for step-by-step testing instructions for the Swift helper, Discord integration, and full end-to-end flow.

## Notes

- macOS only (v1)
- Requires Discord to be running for Rich Presence to work
- Apple Music must be granted media access (macOS will prompt)
- No data leaves your machine

> **NOTE:** App still in development, expect bugs here and there!

## License

MIT
