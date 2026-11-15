#!/usr/bin/env bash
# Builds dist/Twayblade-<version>-x86_64.AppImage from a release binary (#53).
#
#   packaging/linux/build-appimage.sh target/release/twayblade 0.1.0 dist
#
# linuxdeploy is pinned to a tagged release and checked against its SHA-256
# before it runs. It copies the binary's shared libraries into the image,
# except the system ones (glibc, GL, X11, Wayland) on its exclude list.
# Nothing is signed or uploaded.
set -euo pipefail

binary="$1"
version="$2"
out="$3"

LINUXDEPLOY_VERSION=1-alpha-20251107-1
LINUXDEPLOY_SHA256=c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d

root="$(cd "$(dirname "$0")/../.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

tool="$work/linuxdeploy-x86_64.AppImage"
curl -fsSL -o "$tool" \
    "https://github.com/linuxdeploy/linuxdeploy/releases/download/${LINUXDEPLOY_VERSION}/linuxdeploy-x86_64.AppImage"
echo "${LINUXDEPLOY_SHA256}  ${tool}" | sha256sum --check --quiet
chmod +x "$tool"

app="$work/AppDir"
install -Dm755 "$binary" "$app/usr/bin/twayblade"
install -Dm644 "$root/packaging/linux/com.twayblade.desktop.metainfo.xml" \
    "$app/usr/share/metainfo/com.twayblade.desktop.appdata.xml"
install -Dm644 "$root/packaging/icons/com.twayblade.desktop.svg" \
    "$app/usr/share/icons/hicolor/scalable/apps/com.twayblade.desktop.svg"
install -Dm644 "$root/crates/media/licenses/libwebrtc-a545de4.md" \
    "$app/usr/share/doc/twayblade/libwebrtc-LICENSE.md"
install -Dm644 "$root/LICENSE" "$app/usr/share/doc/twayblade/LICENSE"

mkdir -p "$out"
# The runner has no FUSE; run the tool from its extracted contents.
export APPIMAGE_EXTRACT_AND_RUN=1
export LDAI_OUTPUT="$out/Twayblade-${version}-x86_64.AppImage"
export OUTPUT="$LDAI_OUTPUT" # older appimage plugin name
export LINUXDEPLOY_OUTPUT_VERSION="$version"
"$tool" --appdir "$app" \
    --executable "$app/usr/bin/twayblade" \
    --desktop-file "$root/packaging/linux/com.twayblade.desktop.desktop" \
    --icon-file "$root/packaging/icons/com.twayblade.desktop.png" \
    --output appimage
test -s "$LDAI_OUTPUT"
echo "$LDAI_OUTPUT"
