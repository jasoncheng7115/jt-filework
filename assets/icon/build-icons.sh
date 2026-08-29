#!/usr/bin/env bash
# Build every platform icon artefact from the three master SVGs.
#
# Requires: rsvg-convert (librsvg), ImageMagick, and on macOS iconutil.
# Run from anywhere; paths are resolved relative to this script.
#
# Size -> artwork mapping (see README.md for why):
#   <= 16px        jt-filework-16.svg
#   17..64px       jt-filework-32.svg
#   >= 65px        jt-filework.svg

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out="$here/generated"
rm -rf "$out"
mkdir -p "$out/png"

art_for() {
    local size="$1"
    if   [ "$size" -le 16 ]; then echo "$here/jt-filework-16.svg"
    elif [ "$size" -le 64 ]; then echo "$here/jt-filework-32.svg"
    else                          echo "$here/jt-filework.svg"
    fi
}

render() {  # render <size> <destination>
    rsvg-convert -w "$1" -h "$1" "$(art_for "$1")" -o "$2"
}

echo "==> PNG set"
for size in 16 24 32 48 64 128 256 512 1024; do
    render "$size" "$out/png/jt-filework-${size}.png"
done

echo "==> macOS .icns"
iconset="$out/JTFileWork.iconset"
mkdir -p "$iconset"
render 16   "$iconset/icon_16x16.png"
render 32   "$iconset/icon_16x16@2x.png"
render 32   "$iconset/icon_32x32.png"
render 64   "$iconset/icon_32x32@2x.png"
render 128  "$iconset/icon_128x128.png"
render 256  "$iconset/icon_128x128@2x.png"
render 256  "$iconset/icon_256x256.png"
render 512  "$iconset/icon_256x256@2x.png"
render 512  "$iconset/icon_512x512.png"
render 1024 "$iconset/icon_512x512@2x.png"
if command -v iconutil >/dev/null 2>&1; then
    iconutil -c icns "$iconset" -o "$out/JTFileWork.icns"
    rm -rf "$iconset"
else
    echo "    iconutil not found (not macOS); leaving the .iconset directory"
fi

echo "==> Windows .ico"
magick \
    "$out/png/jt-filework-16.png" \
    "$out/png/jt-filework-24.png" \
    "$out/png/jt-filework-32.png" \
    "$out/png/jt-filework-48.png" \
    "$out/png/jt-filework-64.png" \
    "$out/png/jt-filework-128.png" \
    "$out/png/jt-filework-256.png" \
    "$out/JTFileWork.ico"

echo "==> contact sheet (review artefact)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cp "$out/png/jt-filework-512.png" "$tmp/a512.png"
for size in 128 64 32 16; do
    magick "$out/png/jt-filework-${size}.png" -filter point -resize 512x512 "$tmp/b${size}.png"
done
# montage emits a harmless FreeType warning when no label font is configured.
magick montage \
    "$tmp/a512.png" "$tmp/b128.png" "$tmp/b64.png" "$tmp/b32.png" "$tmp/b16.png" \
    -tile 5x1 -geometry +12+12 -background '#8A8A8A' "$out/contact-sheet.png"

echo "done -> $out"
