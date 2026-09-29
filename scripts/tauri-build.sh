#!/usr/bin/env bash
# Wrapper around `tauri build` used by CI (tauri-action tauriScript) and local
# release builds. tauri-action invokes it as: scripts/tauri-build.sh build <args...>
# On Linux it patches the produced AppImage afterwards so it renders on modern
# Wayland hosts without workarounds (see scripts/patch-appimage.mjs, issue #46).
set -euo pipefail

cd "$(dirname "$0")/.."

pnpm tauri "$@"

if [ "${1:-}" = "build" ] && [ "$(uname -s)" = "Linux" ]; then
  node scripts/patch-appimage.mjs --dir src-tauri/target/release/bundle/appimage
fi
