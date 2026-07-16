#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
TARGET=x86_64-unknown-linux-gnu
ARCH=amd64
DIST="$ROOT/dist"
APPIMAGE_ARCH=x86_64
APPIMAGE_TOOLS="$ROOT/target/appimage-tools"
LINUXDEPLOY="$APPIMAGE_TOOLS/linuxdeploy"
LINUXDEPLOY_PLUGIN="$APPIMAGE_TOOLS/linuxdeploy-plugin-appimage"
APPIMAGE_RUNTIME="$APPIMAGE_TOOLS/runtime-x86_64"
LINUXDEPLOY_URL=https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
LINUXDEPLOY_SHA256=e87ee0815d109282fdda73e34c2361d64d02b0ffaea3674b18f1fd1f6a687dcf
LINUXDEPLOY_PLUGIN_URL=https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-x86_64.AppImage
LINUXDEPLOY_PLUGIN_SHA256=1da16a46fa5e058ae740e7c35ed0d36d86cb869ac9cc8a5fd9a1847d7978d99a
APPIMAGE_RUNTIME_URL=https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-x86_64
APPIMAGE_RUNTIME_SHA256=1cc49bcf1e2ccd593c379adb17c9f85a36d619088296504de95b1d06215aebbf

command -v cargo >/dev/null
command -v curl >/dev/null
command -v dpkg-deb >/dev/null
command -v jq >/dev/null
command -v sha256sum >/dev/null

download_verified_tool() {
    local destination=$1
    local url=$2
    local expected_sha256=$3

    if [[ -f "$destination" ]] \
        && printf '%s  %s\n' "$expected_sha256" "$destination" \
            | sha256sum --check --status; then
        chmod 0755 "$destination"
        return
    fi

    mkdir -p "$(dirname -- "$destination")"
    local temporary
    temporary=$(mktemp "${destination}.download.XXXXXX")
    if ! curl --fail --location --retry 3 --proto '=https' --tlsv1.2 \
        --output "$temporary" "$url"; then
        rm -f "$temporary"
        return 1
    fi
    if ! printf '%s  %s\n' "$expected_sha256" "$temporary" \
        | sha256sum --check --status; then
        printf 'Checksum verification failed for %s.\n' "$url" >&2
        rm -f "$temporary"
        return 1
    fi
    chmod 0755 "$temporary"
    mv -f "$temporary" "$destination"
}

HOST_ARCH=$(uname -m)
if [[ "$HOST_ARCH" != x86_64 ]]; then
    printf 'Termi v1 packages currently support x86_64 hosts only (found %s).\n' "$HOST_ARCH" >&2
    exit 1
fi

cd "$ROOT"
VERSION=$(cargo metadata --no-deps --format-version 1 --locked | jq -r '.packages[0].version')
EXPECTED_VERSION=${1:-$VERSION}
if [[ "$EXPECTED_VERSION" != "$VERSION" ]]; then
    printf 'Requested version %s does not match Cargo.toml version %s.\n' "$EXPECTED_VERSION" "$VERSION" >&2
    exit 1
fi

SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2>/dev/null || date +%s)}
export SOURCE_DATE_EPOCH

cargo build --release --locked --target "$TARGET"
BINARY="$ROOT/target/$TARGET/release/termi"
"$BINARY" --version
"$BINARY" --print-config-path >/dev/null

rm -rf "$DIST"
mkdir -p "$DIST"

ARCHIVE_NAME="termi-$VERSION-$TARGET"
ARCHIVE_ROOT="$DIST/$ARCHIVE_NAME"
install -D -m 0755 "$BINARY" "$ARCHIVE_ROOT/bin/termi"
install -D -m 0644 packaging/ai.clairos.termi.desktop "$ARCHIVE_ROOT/share/applications/ai.clairos.termi.desktop"
install -D -m 0644 assets/icons/ai.clairos.termi.svg "$ARCHIVE_ROOT/share/icons/hicolor/scalable/apps/ai.clairos.termi.svg"
install -D -m 0644 packaging/ai.clairos.termi.metainfo.xml "$ARCHIVE_ROOT/share/metainfo/ai.clairos.termi.metainfo.xml"
install -D -m 0644 assets/shell/termi.bash "$ARCHIVE_ROOT/share/termi/termi.bash"
install -D -m 0644 README.md "$ARCHIVE_ROOT/README.md"
install -D -m 0644 CHANGELOG.md "$ARCHIVE_ROOT/CHANGELOG.md"
install -D -m 0644 LICENSE "$ARCHIVE_ROOT/LICENSE"
if [[ -f THIRD_PARTY_NOTICES.html ]]; then
    install -D -m 0644 THIRD_PARTY_NOTICES.html "$ARCHIVE_ROOT/THIRD_PARTY_NOTICES.html"
else
    install -D -m 0644 THIRD_PARTY_NOTICES.md "$ARCHIVE_ROOT/THIRD_PARTY_NOTICES.md"
fi

tar \
    --sort=name \
    --mtime="@$SOURCE_DATE_EPOCH" \
    --owner=0 \
    --group=0 \
    --numeric-owner \
    -C "$DIST" \
    -czf "$DIST/$ARCHIVE_NAME.tar.gz" \
    "$ARCHIVE_NAME"
rm -rf "$ARCHIVE_ROOT"

DEB_ROOT="$DIST/deb-root"
install -D -m 0755 "$BINARY" "$DEB_ROOT/usr/bin/termi"
install -D -m 0644 packaging/ai.clairos.termi.desktop "$DEB_ROOT/usr/share/applications/ai.clairos.termi.desktop"
install -D -m 0644 assets/icons/ai.clairos.termi.svg "$DEB_ROOT/usr/share/icons/hicolor/scalable/apps/ai.clairos.termi.svg"
install -D -m 0644 packaging/ai.clairos.termi.metainfo.xml "$DEB_ROOT/usr/share/metainfo/ai.clairos.termi.metainfo.xml"
install -D -m 0644 assets/shell/termi.bash "$DEB_ROOT/usr/share/termi/termi.bash"
install -D -m 0644 README.md "$DEB_ROOT/usr/share/doc/termi/README.md"
install -D -m 0644 CHANGELOG.md "$DEB_ROOT/usr/share/doc/termi/CHANGELOG.md"
install -D -m 0644 LICENSE "$DEB_ROOT/usr/share/doc/termi/copyright"
if [[ -f THIRD_PARTY_NOTICES.html ]]; then
    install -D -m 0644 THIRD_PARTY_NOTICES.html "$DEB_ROOT/usr/share/doc/termi/THIRD_PARTY_NOTICES.html"
else
    install -D -m 0644 THIRD_PARTY_NOTICES.md "$DEB_ROOT/usr/share/doc/termi/THIRD_PARTY_NOTICES.md"
fi

INSTALLED_SIZE=$(du -sk "$DEB_ROOT/usr" | cut -f1)
mkdir -p "$DEB_ROOT/DEBIAN"
cat > "$DEB_ROOT/DEBIAN/control" <<EOF
Package: termi
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: ${TERMI_MAINTAINER:-Josh <128094135+joshy1187@users.noreply.github.com>}
Installed-Size: $INSTALLED_SIZE
Depends: libc6 (>= 2.39), libegl1, libfontconfig1, libfreetype6, libgcc-s1, libgl1, libwayland-client0, libwayland-egl1, libx11-6, libx11-xcb1, libxcb1, libxcursor1, libxi6, libxkbcommon-x11-0, libxkbcommon0
Description: native Rust and Slint terminal emulator
 Termi is a multi-tab Linux terminal with scrollback search, clipboard support,
 mouse reporting, bracketed paste, and a frameless desktop interface.
EOF

DEB_PATH="$DIST/termi_${VERSION}_${ARCH}.deb"
dpkg-deb --root-owner-group --build "$DEB_ROOT" "$DEB_PATH"
rm -rf "$DEB_ROOT"

download_verified_tool "$LINUXDEPLOY" "$LINUXDEPLOY_URL" "$LINUXDEPLOY_SHA256"
download_verified_tool \
    "$LINUXDEPLOY_PLUGIN" \
    "$LINUXDEPLOY_PLUGIN_URL" \
    "$LINUXDEPLOY_PLUGIN_SHA256"
download_verified_tool \
    "$APPIMAGE_RUNTIME" \
    "$APPIMAGE_RUNTIME_URL" \
    "$APPIMAGE_RUNTIME_SHA256"

APPDIR="$ROOT/target/appimage/Termi.AppDir"
APPIMAGE_PATH="$DIST/termi-$VERSION-$APPIMAGE_ARCH.AppImage"
rm -rf "$APPDIR"
install -D -m 0644 packaging/ai.clairos.termi.metainfo.xml "$APPDIR/usr/share/metainfo/ai.clairos.termi.appdata.xml"
install -D -m 0644 assets/shell/termi.bash "$APPDIR/usr/share/termi/termi.bash"
install -D -m 0644 README.md "$APPDIR/usr/share/doc/termi/README.md"
install -D -m 0644 CHANGELOG.md "$APPDIR/usr/share/doc/termi/CHANGELOG.md"
install -D -m 0644 LICENSE "$APPDIR/usr/share/doc/termi/copyright"
if [[ -f THIRD_PARTY_NOTICES.html ]]; then
    install -D -m 0644 THIRD_PARTY_NOTICES.html "$APPDIR/usr/share/doc/termi/THIRD_PARTY_NOTICES.html"
else
    install -D -m 0644 THIRD_PARTY_NOTICES.md "$APPDIR/usr/share/doc/termi/THIRD_PARTY_NOTICES.md"
fi

(
    export APPIMAGE_EXTRACT_AND_RUN=1
    export ARCH=$APPIMAGE_ARCH
    export LDAI_OUTPUT=$APPIMAGE_PATH
    export LDAI_RUNTIME_FILE=$APPIMAGE_RUNTIME
    export LINUXDEPLOY_OUTPUT_APP_NAME=termi
    export LINUXDEPLOY_OUTPUT_VERSION=$VERSION
    export PATH="$APPIMAGE_TOOLS:$PATH"
    "$LINUXDEPLOY" \
        --appdir "$APPDIR" \
        --executable "$BINARY" \
        --desktop-file packaging/ai.clairos.termi.desktop \
        --icon-file assets/icons/ai.clairos.termi.svg \
        --output appimage
)
[[ -x "$APPIMAGE_PATH" ]]
rm -rf "$APPDIR"

(
    cd "$DIST"
    sha256sum ./*.AppImage ./*.deb ./*.tar.gz > SHA256SUMS
)

printf 'Release artifacts written to %s\n' "$DIST"
