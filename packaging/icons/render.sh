#!/usr/bin/env bash
# Regenerates the raster icons from com.twayblade.desktop.svg (the source).
# Needs rsvg-convert and ImageMagick, e.g. `nix shell nixpkgs#librsvg nixpkgs#imagemagick`.
set -euo pipefail
cd "$(dirname "$0")"
source=com.twayblade.desktop.svg
rsvg-convert -w 512 -h 512 "$source" -o com.twayblade.desktop.png
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
for size in 16 24 32 48 64 128 256; do
    rsvg-convert -w "$size" -h "$size" "$source" -o "$work/$size.png"
done
# Windows installer, shortcut and Apps & Features icon.
magick "$work"/{16,24,32,48,64,128,256}.png twayblade.ico
