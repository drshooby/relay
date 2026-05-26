# Spec — Sub-project #4: `.app` bundle + distribution (#21)

## Context

End users currently need `cargo build` + a terminal to run Relay. v2 needs a double-clickable `Relay.app` distributed as a `.dmg` via GitHub Releases, with the Swift helper bundled inside. Existing scaffolding: `packaging/Info.plist` (minimal), `resolve_helper_path()` already supports `Contents/Resources/relay-helper` (`src/media/helper_path.rs:23`), and `build.rs` compiles the helper via `swiftc` into `OUT_DIR`.

## Design

### Bundle tool: cargo-packager
Declarative config in `Cargo.toml`. CI runs `cargo packager --release` and uploads the produced `.dmg`. arm64-only for v1.

Add to `Cargo.toml`:

```toml
[package.metadata.packager]
product-name = "Relay"
identifier = "com.drshooby.relay"
category = "Music"
description = "Apple Music → Discord Rich Presence"
authors = ["David Shubov"]
icons = ["assets/icons/Relay.icns"]
resources = [{ src = "target/release/relay-helper", target = "relay-helper" }]
formats = ["dmg"]
before-packaging-command = "bash packaging/make-icns.sh"

[package.metadata.packager.macos]
minimum-system-version = "11.0"
signing-identity = "-"   # ad-hoc; replace with Developer ID later
info-plist-path = "packaging/Info.plist"
```

The exact key names follow cargo-packager's current schema — the implementer must verify against the version they install (`cargo install cargo-packager`) and adjust kebab/snake casing if the tool has moved on. See https://github.com/crabnebula-dev/cargo-packager.

### Helper bundling
`build.rs` currently writes the helper to `OUT_DIR/relay-helper` (a hash-suffixed path). cargo-packager needs a stable path. Extend `build.rs` to *also* copy the compiled helper to `target/<profile>/relay-helper`:

```rust
let stable = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("target")
    .join(&profile)
    .join(HELPER_BINARY_NAME);
std::fs::copy(&dest, &stable).expect("failed to copy helper to stable target path");
```

Inside the bundle the helper lands at `Relay.app/Contents/Resources/relay-helper` — `resolve_helper_path()` step 2 already finds it. Verify that path resolution still works (see Verification §3).

### Info.plist
Trim `packaging/Info.plist` to just what cargo-packager will *merge in* on top of its generated plist:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>LSUIElement</key>
    <true/>
    <key>NSAppleEventsUsageDescription</key>
    <string>Relay reads playback state from Apple Music to display it on your Discord status.</string>
</dict>
</plist>
```

Bundle name, identifier, version, executable, icon are sourced from Cargo metadata. `LSUIElement` keeps Relay out of the Dock. `NSAppleEventsUsageDescription` gives the Apple Events permission prompt (issue #31) a non-generic explanation — small UX win in this PR; formal onboarding deferred to sub-project #5.

### Icon generation
`packaging/make-icns.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
SRC="assets/icons/relay.png"
OUT="assets/icons/Relay.icns"
TMP="$(mktemp -d)/Relay.iconset"
mkdir -p "$TMP"
for SIZE in 16 32 64 128 256 512 1024; do
  HALF=$((SIZE / 2))
  sips -z "$SIZE" "$SIZE" "$SRC" --out "$TMP/icon_${HALF}x${HALF}@2x.png" >/dev/null
  sips -z "$SIZE" "$SIZE" "$SRC" --out "$TMP/icon_${SIZE}x${SIZE}.png" >/dev/null
done
iconutil -c icns "$TMP" -o "$OUT"
```

Add `assets/icons/Relay.icns` to `.gitignore` (regenerated each build).

### GitHub Actions release workflow
`.github/workflows/release.yml`:

```yaml
name: Release
on:
  push:
    tags: ["v*"]
permissions:
  contents: write
jobs:
  build:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Install cargo-packager
        run: cargo install cargo-packager --locked
      - name: Package
        run: cargo packager --release --formats dmg
      - name: Upload to release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            target/release/*.dmg
            dist/*.dmg
```

The exact output path varies by cargo-packager version — the implementer must verify and tighten the glob. `softprops/action-gh-release@v2` auto-creates a release on tag push if one doesn't exist.

### Dev workflow preservation
`cargo run` and `cargo build` MUST remain unaffected. Verify by running both after the changes. cargo-packager only activates on explicit `cargo packager` invocation.

## Critical files

- `Cargo.toml` — add `[package.metadata.packager]`
- `build.rs` — copy helper to stable `target/<profile>/relay-helper`
- `packaging/Info.plist` — trim to merge-only keys (LSUIElement + usage string)
- `packaging/make-icns.sh` — new, generates `.icns`
- `.gitignore` — add `assets/icons/Relay.icns`
- `.github/workflows/release.yml` — new
- `README.md` — add "Install" section pointing to GitHub Releases / `.dmg`

## Verification

1. **Local cargo workflow unaffected**: `cargo run` and `cargo build --release` succeed; the helper still launches.
2. **Local packager build**: `cargo install cargo-packager --locked` then `cargo packager --release --formats dmg` produces a `.dmg` containing `Relay.app`. Open the `.dmg`, drag to /Applications, launch — menu bar icon appears, no Terminal, Discord activity publishes when Apple Music plays.
3. **Helper path resolution**: launch the installed `.app` from /Applications, verify (via Console.app or just observing functionality) that the bundled helper at `Contents/Resources/relay-helper` is the one running.
4. **Lint/test gates green**: `cargo clippy -- -D warnings`, `cargo fmt -- --check`, `cargo test`.
5. **CI dry run**: NOT required as part of the PR — workflow exists, will be exercised on first real tag. Implementer should `act` or `actionlint` the YAML if possible, but no live CI run.

## Out of scope

- Developer ID signing / notarization (separate ticket; ad-hoc sign acceptable for v1).
- Universal binary (arm64 + x86_64) — arm64-only.
- Homebrew cask.
- Auto-update mechanism.
- Login-item integration (belongs to sub-project #5 / issue #20).

## Execution

- Worktree: `.worktrees/app-bundle` on branch `feat/app-bundle`.
- One Sonnet subagent. PR title: `feat(packaging): Relay.app bundle and release workflow`. Body: `Closes #21.`
