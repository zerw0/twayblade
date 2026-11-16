#!/usr/bin/env bash
# Regenerates the Linux PNG and the Windows icon from the app icon source,
# assets/twayblade-icon-transparent.png (1254 px). The macOS
# assets/Twayblade.icns is made from the same source with iconutil.
# Needs ImageMagick, e.g. `nix shell nixpkgs#imagemagick`.
set -euo pipefail
cd "$(dirname "$0")"
source=../../assets/twayblade-icon-transparent.png
magick "$source" -resize 512x512 com.twayblade.desktop.png
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
for size in 16 24 32 48 64 128 256; do
    magick "$source" -resize "${size}x${size}" "$work/$size.png"
done
# Windows installer, shortcut and Apps & Features icon.
magick "$work"/{16,24,32,48,64,128,256}.png twayblade.ico
