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
