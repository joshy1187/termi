#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
BIN_DIR=${HOME}/.local/bin
APP_DIR=${HOME}/.local/share/applications

cd "$ROOT"
cargo build --release
install -d "$BIN_DIR" "$APP_DIR"
install -m 0755 target/release/termi "$BIN_DIR/termi"
install -m 0644 packaging/ai.clairos.termi.desktop "$APP_DIR/ai.clairos.termi.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
fi

printf 'Installed Termi to %s\n' "$BIN_DIR/termi"
printf 'Desktop entry: %s\n' "$APP_DIR/ai.clairos.termi.desktop"
