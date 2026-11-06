#!/bin/sh
# Build kimo-gui-linux-x86_64.tar.xz (+ .sha512) from a built GUI and engine. Used by the release workflow and tests.
#   make-package.sh <kimo-gui> <kimo-mcp> <fonts-dir|-> <out-dir>
set -eu
GUI=$1; ENGINE=$2; FONTS=$3; OUT=$4
HERE=$(cd "$(dirname "$0")" && pwd)
ST=$(mktemp -d); trap 'rm -rf "$ST"' EXIT
install -m 755 "$GUI" "$ST/kimo-gui"
install -m 755 "$ENGINE" "$ST/kimo-mcp"
[ "$FONTS" != "-" ] && [ -d "$FONTS" ] && cp -r "$FONTS" "$ST/fonts"
cp "$HERE/../../icons/128x128.png" "$ST/kimo.png"
cp "$HERE/../../../../LICENSE" "$ST/LICENSE" 2>/dev/null || true
mkdir -p "$OUT"
# fixed order and owner so the archive is reproducible for a given input
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime='2026-01-01' -C "$ST" -cf - . | xz -9e -T1 > "$OUT/kimo-gui-linux-x86_64.tar.xz"
(cd "$OUT" && sha512sum kimo-gui-linux-x86_64.tar.xz > kimo-gui-linux-x86_64.tar.xz.sha512)
cp "$HERE/kimo-setup.sh" "$OUT/kimo-setup.sh"
ls -l "$OUT"
