#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
BIN_DIR=${HOME}/.local/bin
APP_DIR=${HOME}/.local/share/applications
DOC_DIR=${HOME}/.local/share/doc/termi
DATA_DIR=${HOME}/.local/share/termi
ICON_DIR=${HOME}/.local/share/icons/hicolor/scalable/apps
METAINFO_DIR=${HOME}/.local/share/metainfo
LEGACY_ICON=${HOME}/.local/share/icons/hicolor/256x256/apps/termi.png
TARGET=x86_64-unknown-linux-gnu

if [[ $(uname -m) != x86_64 ]]; then
    printf 'Termi v1 currently supports x86_64 Linux only.\n' >&2
    exit 1
fi

cd "$ROOT"
cargo build --release --locked --target "$TARGET"
if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate packaging/ai.clairos.termi.desktop
fi

install -d "$BIN_DIR" "$APP_DIR" "$DOC_DIR" "$DATA_DIR" "$ICON_DIR" "$METAINFO_DIR"
install -m 0755 "target/$TARGET/release/termi" "$BIN_DIR/termi"
install -m 0644 packaging/ai.clairos.termi.desktop "$APP_DIR/ai.clairos.termi.desktop"
install -m 0644 assets/icons/ai.clairos.termi.svg "$ICON_DIR/ai.clairos.termi.svg"
install -m 0644 packaging/ai.clairos.termi.metainfo.xml "$METAINFO_DIR/ai.clairos.termi.metainfo.xml"
install -m 0644 README.md "$DOC_DIR/README.md"
install -m 0644 CHANGELOG.md "$DOC_DIR/CHANGELOG.md"
install -m 0644 LICENSE "$DOC_DIR/copyright"
if [[ -f THIRD_PARTY_NOTICES.html ]]; then
    install -m 0644 THIRD_PARTY_NOTICES.html "$DOC_DIR/THIRD_PARTY_NOTICES.html"
else
    install -m 0644 THIRD_PARTY_NOTICES.md "$DOC_DIR/THIRD_PARTY_NOTICES.md"
fi
install -m 0644 assets/shell/termi.bash "$DATA_DIR/termi.bash"
"$BIN_DIR/termi" --version

LEGACY_DESKTOP="$APP_DIR/termi.desktop"
if [[ -f "$LEGACY_DESKTOP" ]] \
    && grep -Fxq 'Name=Termi' "$LEGACY_DESKTOP" \
    && grep -Eq '^Exec=.*(/|^)termi([[:space:]]|$)' "$LEGACY_DESKTOP"; then
    rm -f "$LEGACY_DESKTOP" "$LEGACY_ICON"
    printf 'Removed legacy Termi desktop entry: %s\n' "$LEGACY_DESKTOP"
fi

DESKTOP_DIR=
if command -v xdg-user-dir >/dev/null 2>&1; then
    DESKTOP_DIR=$(xdg-user-dir DESKTOP 2>/dev/null || true)
fi
if [[ -n "$DESKTOP_DIR" && -d "$DESKTOP_DIR" ]] \
    && command -v desktop-file-edit >/dev/null 2>&1; then
    for desktop_shortcut in \
        "$DESKTOP_DIR/Termi.desktop" \
        "$DESKTOP_DIR/termi.desktop"; do
        if [[ -f "$desktop_shortcut" ]] \
            && grep -Fxq 'Name=Termi' "$desktop_shortcut" \
            && grep -Eq '^Exec=.*(/|^)termi([[:space:]]|$)' "$desktop_shortcut"; then
            desktop_position=
            if command -v gio >/dev/null 2>&1; then
                desktop_position=$(gio info \
                    -a metadata::nautilus-icon-position \
                    "$desktop_shortcut" 2>/dev/null \
                    | sed -n 's/^[[:space:]]*metadata::nautilus-icon-position: //p')
            fi
            desktop-file-edit \
                --set-name=Termi \
                --set-generic-name='Terminal Emulator' \
                --set-comment='Native galactic terminal built with Rust and Slint' \
                --set-icon="$ICON_DIR/ai.clairos.termi.svg" \
                --set-key=Exec --set-value="$BIN_DIR/termi" \
                --set-key=TryExec --set-value="$BIN_DIR/termi" \
                --set-key=StartupWMClass --set-value=ai.clairos.termi \
                "$desktop_shortcut"
            chmod 0755 "$desktop_shortcut"
            if command -v gio >/dev/null 2>&1; then
                gio set "$desktop_shortcut" metadata::trusted true \
                    >/dev/null 2>&1 || true
                if [[ -n "$desktop_position" ]]; then
                    gio set "$desktop_shortcut" \
                        metadata::nautilus-icon-position "$desktop_position" \
                        >/dev/null 2>&1 || true
                fi
            fi
            printf 'Updated desktop shortcut: %s\n' "$desktop_shortcut"
        fi
    done
fi

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache --force --ignore-theme-index \
        "${HOME}/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi

printf 'Installed Termi to %s\n' "$BIN_DIR/termi"
printf 'Desktop entry: %s\n' "$APP_DIR/ai.clairos.termi.desktop"
printf 'Desktop icon: %s\n' "$ICON_DIR/ai.clairos.termi.svg"
