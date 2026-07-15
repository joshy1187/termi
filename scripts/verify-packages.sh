#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
DIST=${1:-$ROOT/dist}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

DIST=$(cd -- "$DIST" && pwd)
cd "$DIST"
sha256sum --check SHA256SUMS

DEB=$(find . -maxdepth 1 -name 'termi_*_amd64.deb' -print -quit)
ARCHIVE=$(find . -maxdepth 1 -name 'termi-*-x86_64-unknown-linux-gnu.tar.gz' -print -quit)
APPIMAGE=$(find . -maxdepth 1 -name 'termi-*-x86_64.AppImage' -print -quit)
[[ -n "$DEB" && -n "$ARCHIVE" && -n "$APPIMAGE" ]]
grep -Fq "  $APPIMAGE" SHA256SUMS
[[ -x "$APPIMAGE" ]]
file "$APPIMAGE" | grep -q 'ELF 64-bit LSB.*x86-64'

dpkg-deb --info "$DEB" >/dev/null
dpkg-deb --extract "$DEB" "$WORK/deb"
[[ -f "$WORK/deb/usr/share/applications/ai.clairos.termi.desktop" ]]
[[ -f "$WORK/deb/usr/share/icons/hicolor/scalable/apps/ai.clairos.termi.svg" ]]
[[ -f "$WORK/deb/usr/share/metainfo/ai.clairos.termi.metainfo.xml" ]]
[[ -f "$WORK/deb/usr/share/termi/termi.bash" ]]
[[ -f "$WORK/deb/usr/share/doc/termi/CHANGELOG.md" ]]
[[ -f "$WORK/deb/usr/share/doc/termi/copyright" ]]
find "$WORK/deb/usr/share/doc/termi" -maxdepth 1 \
    -name 'THIRD_PARTY_NOTICES.*' -type f -print -quit | grep -q .
"$WORK/deb/usr/bin/termi" --version
"$WORK/deb/usr/bin/termi" --print-config-path >/dev/null
ldd "$WORK/deb/usr/bin/termi" | tee "$WORK/ldd.txt" >/dev/null
if grep -q 'not found' "$WORK/ldd.txt"; then
    printf 'Packaged binary has unresolved shared libraries.\n' >&2
    exit 1
fi

if command -v desktop-file-validate >/dev/null; then
    desktop-file-validate "$WORK/deb/usr/share/applications/ai.clairos.termi.desktop"
fi
grep -Fxq 'StartupWMClass=ai.clairos.termi' \
    "$WORK/deb/usr/share/applications/ai.clairos.termi.desktop"
grep -Fxq 'Icon=ai.clairos.termi' \
    "$WORK/deb/usr/share/applications/ai.clairos.termi.desktop"

DEPENDS=$(dpkg-deb -f "$DEB" Depends)
for dependency in \
    libegl1 libfontconfig1 libfreetype6 libgcc-s1 libgl1 \
    libwayland-client0 libwayland-egl1 libx11-6 libx11-xcb1 libxcb1 \
    libxcursor1 libxi6 libxkbcommon-x11-0 libxkbcommon0; do
    grep -Eq "(^|, )${dependency}( |,|$)" <<<"$DEPENDS"
done

tar -tzf "$ARCHIVE" >/dev/null
tar -xzf "$ARCHIVE" -C "$WORK"
ARCHIVE_ROOT=$(find "$WORK" -maxdepth 1 -type d -name 'termi-*-x86_64-unknown-linux-gnu' -print -quit)
[[ -f "$ARCHIVE_ROOT/share/applications/ai.clairos.termi.desktop" ]]
[[ -f "$ARCHIVE_ROOT/share/icons/hicolor/scalable/apps/ai.clairos.termi.svg" ]]
[[ -f "$ARCHIVE_ROOT/share/metainfo/ai.clairos.termi.metainfo.xml" ]]
[[ -f "$ARCHIVE_ROOT/share/termi/termi.bash" ]]
[[ -f "$ARCHIVE_ROOT/CHANGELOG.md" ]]
find "$ARCHIVE_ROOT" -maxdepth 1 -name 'THIRD_PARTY_NOTICES.*' -type f -print -quit | grep -q .
"$ARCHIVE_ROOT/bin/termi" --version

APPIMAGE_PATH="$DIST/${APPIMAGE#./}"
APPIMAGE_EXTRACT_AND_RUN=1 "$APPIMAGE_PATH" --version
mkdir -p "$WORK/appimage"
(
    cd "$WORK/appimage"
    "$APPIMAGE_PATH" --appimage-extract >/dev/null
)
APPIMAGE_ROOT="$WORK/appimage/squashfs-root"
[[ -x "$APPIMAGE_ROOT/AppRun" ]]
[[ -x "$APPIMAGE_ROOT/usr/bin/termi" ]]
[[ -e "$APPIMAGE_ROOT/ai.clairos.termi.desktop" ]]
[[ -e "$APPIMAGE_ROOT/ai.clairos.termi.svg" ]]
[[ -e "$APPIMAGE_ROOT/.DirIcon" ]]
[[ -f "$APPIMAGE_ROOT/usr/share/metainfo/ai.clairos.termi.appdata.xml" ]]
[[ -f "$APPIMAGE_ROOT/usr/share/termi/termi.bash" ]]
[[ -f "$APPIMAGE_ROOT/usr/share/doc/termi/CHANGELOG.md" ]]
[[ -f "$APPIMAGE_ROOT/usr/share/doc/termi/copyright" ]]
find "$APPIMAGE_ROOT/usr/share/doc/termi" -maxdepth 1 \
    -name 'THIRD_PARTY_NOTICES.*' -type f -print -quit | grep -q .
if command -v desktop-file-validate >/dev/null; then
    desktop-file-validate "$APPIMAGE_ROOT/ai.clairos.termi.desktop"
fi
grep -Fxq 'StartupWMClass=ai.clairos.termi' \
    "$APPIMAGE_ROOT/ai.clairos.termi.desktop"
grep -Fxq 'Icon=ai.clairos.termi' \
    "$APPIMAGE_ROOT/ai.clairos.termi.desktop"
if command -v appstreamcli >/dev/null; then
    appstreamcli validate --no-net \
        "$APPIMAGE_ROOT/usr/share/metainfo/ai.clairos.termi.appdata.xml"
fi

printf 'Verified %s, %s, and %s\n' "$DEB" "$ARCHIVE" "$APPIMAGE"
