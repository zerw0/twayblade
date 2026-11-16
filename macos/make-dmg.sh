#!/usr/bin/env bash
# Packs a built Twayblade.app into a compressed disk image with an
# Applications shortcut, for direct downloads and the Homebrew cask (#52).
#
#     bash macos/build.sh --release
#     bash macos/make-dmg.sh [path/to/Twayblade.app] [output.dmg]
#
# The image is not signed or notarised: Developer ID signing and notarisation
# need release credentials and explicit authorization, and run on the .app
# before this step. Uses only hdiutil, which ships with macOS.
set -euo pipefail
cd "$(dirname "$0")/.."

app="${1:-target/release/bundle/Twayblade.app}"
if [[ ! -d "$app/Contents/MacOS" ]]; then
    echo "make-dmg: no app bundle at $app; run macos/build.sh --release first" >&2
    exit 1
fi
# Release builds name the image after the tag (TWAYBLADE_VERSION, e.g.
# 0.1.0-rc.1); otherwise the bundle's version.
version="${TWAYBLADE_VERSION:-$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist")}"
arch="$(/usr/bin/lipo -archs "$app/Contents/MacOS/twayblade" | tr ' ' '-')"
out="${2:-dist/Twayblade-$version-$arch.dmg}"

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
# ditto keeps the bundle's symlinks, extended attributes and signature.
/usr/bin/ditto "$app" "$stage/Twayblade.app"
ln -s /Applications "$stage/Applications"

mkdir -p "$(dirname "$out")"
rm -f "$out"
/usr/bin/hdiutil create -quiet -volname "Twayblade $version" -srcfolder "$stage" \
    -fs HFS+ -format UDZO -imagekey zlib-level=9 "$out"
/usr/bin/hdiutil verify -quiet "$out"
echo "$out"
