#!/bin/sh
# kimo online setup for Linux (x86_64). Small on purpose: it downloads the kimo app (GUI + engine), checks its
# SHA-512, installs per user (no root), and adds the `kimo` command and a menu entry. FFmpeg and the speech model are
# fetched later by the app's own first-run window. Re-running upgrades; `--uninstall` removes it.
#   curl -fsSL https://github.com/naman-sharma12345/kimo/releases/latest/download/kimo-setup.sh | sh
# Overrides (tests, mirrors): KIMO_BASE_URL, KIMO_PREFIX (default ~/.local/share/kimo), KIMO_BIN_DIR (~/.local/bin),
# KIMO_APPS_DIR (~/.local/share/applications).
set -eu
BASE="${KIMO_BASE_URL:-https://github.com/naman-sharma12345/kimo/releases/latest/download}"
PKG="kimo-gui-linux-x86_64.tar.xz"
PREFIX="${KIMO_PREFIX:-$HOME/.local/share/kimo}"
BIN_DIR="${KIMO_BIN_DIR:-$HOME/.local/bin}"
APPS_DIR="${KIMO_APPS_DIR:-$HOME/.local/share/applications}"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/kimo-setup"

say() { printf '%s\n' "kimo: $*"; }
die() { printf '%s\n' "kimo: $*" >&2; exit 1; }

if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$BIN_DIR/kimo" "$APPS_DIR/kimo.desktop"
  [ -f "$PREFIX/.kimo-install" ] && rm -rf "$PREFIX"
  say "removed (your projects and settings are left alone)"; exit 0
fi

[ "$(uname -s)" = "Linux" ] || die "this setup is for Linux"
[ "$(uname -m)" = "x86_64" ] || die "only x86_64 builds are published so far ($(uname -m) found)"
command -v curl >/dev/null 2>&1 || die "curl is needed"
command -v sha512sum >/dev/null 2>&1 || die "sha512sum is needed (coreutils)"
command -v xz >/dev/null 2>&1 || die "xz is needed (xz-utils)"
if ! ldconfig -p 2>/dev/null | grep -q 'libwebkit2gtk-4.1'; then
  say "note: the window needs WebKitGTK 4.1 (Debian/Ubuntu: sudo apt install libwebkit2gtk-4.1-0; Fedora: sudo dnf install webkit2gtk4.1)"
fi

mkdir -p "$CACHE"
say "checking the release"
curl -fsSL --retry 3 "$BASE/$PKG.sha512" -o "$CACHE/$PKG.sha512" || die "could not reach $BASE (offline?)"
WANT=$(cut -d' ' -f1 < "$CACHE/$PKG.sha512")
[ "${#WANT}" -eq 128 ] || die "the release's checksum file is not a SHA-512"

have() { [ -f "$CACHE/$PKG" ] && [ "$(sha512sum "$CACHE/$PKG" | cut -d' ' -f1)" = "$WANT" ]; }
if ! have; then
  say "downloading kimo (resumes if interrupted)"
  # -C - resumes a partial file with an HTTP Range request
  curl -fL --retry 5 --retry-delay 2 -C - --progress-bar "$BASE/$PKG" -o "$CACHE/$PKG" || {
    # a server that refuses the range (or a stale partial) gets one clean retry
    rm -f "$CACHE/$PKG"; curl -fL --retry 3 --progress-bar "$BASE/$PKG" -o "$CACHE/$PKG" || die "download failed"; }
  have || { rm -f "$CACHE/$PKG"; die "the download does not match its SHA-512; refused and removed"; }
fi
say "SHA-512 verified"

STAGE="$PREFIX.new.$$"
rm -rf "$STAGE"; mkdir -p "$STAGE"
xz -dc "$CACHE/$PKG" | tar -xf - -C "$STAGE" || { rm -rf "$STAGE"; die "could not unpack"; }
[ -x "$STAGE/kimo-gui" ] && [ -x "$STAGE/kimo-mcp" ] || { rm -rf "$STAGE"; die "package is missing kimo-gui or kimo-mcp"; }
touch "$STAGE/.kimo-install"
# keep anything the app downloaded into the old install dir (none by default: the app uses ~/.local/share/dev.kimo.app)
if [ -d "$PREFIX" ]; then rm -rf "$PREFIX.old"; mv "$PREFIX" "$PREFIX.old"; fi
mkdir -p "$(dirname "$PREFIX")"; mv "$STAGE" "$PREFIX"; rm -rf "$PREFIX.old"

mkdir -p "$BIN_DIR" "$APPS_DIR"
ln -sf "$PREFIX/kimo-gui" "$BIN_DIR/kimo"
cat > "$APPS_DIR/kimo.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=kimo
Comment=AI video editor
Exec=$PREFIX/kimo-gui
Icon=$PREFIX/kimo.png
Categories=AudioVideo;Video;
Terminal=false
EOF
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true
VER=$("$PREFIX/kimo-gui" --version 2>/dev/null || echo "kimo")
say "installed $VER in $PREFIX"
say "open it from your app menu or run: kimo        (MCP for coding agents: kimo mcp)"
case ":$PATH:" in *":$BIN_DIR:"*) ;; *) say "add $BIN_DIR to your PATH to use the kimo command";; esac
