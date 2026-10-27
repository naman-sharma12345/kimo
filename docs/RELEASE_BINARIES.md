# Release binaries

Assets per tag `vX.Y.Z` (or a manual `workflow_dispatch` run of the `release` workflow with
`draft_release` ticked):

- `kimo-<tag>-linux-x86_64-glibc.tar.gz` - the engine (`kimo-mcp`), built on Ubuntu 22.04, needs
  glibc >= 2.35. This is the only Linux engine build this release ships: the static musl build was
  dropped on 2026-10-17 (the vendored Manifold C++ library has no musl C++ compiler on the runner).
- `kimo-<version>-windows-x86_64-setup.exe` - per-user Windows x64 installer for the engine, with
  uninstall support.
- `kimo-<version>-windows-x86_64.zip` - portable Windows x64 package for the engine.
- Kimo Spectator (the desktop app, `crates/kimo-gui`): `Kimo-Spectator_<version>_amd64.deb` and
  `Kimo-Spectator_<version>_amd64.AppImage` for Linux, `Kimo-Spectator_<version>_x64_en-US.msi` for
  Windows.
- `SHA256SUMS`, covering every file above; a `.sha256` also sits beside each engine package.
- No macOS build this release (coming later).

All installers and binaries are unsigned. Manual `workflow_dispatch` runs with `draft_release` off
produce workflow artifacts and `SHA256SUMS` only, no release; a tag push (or a dispatch run on a `v*`
ref with `draft_release` on) attaches everything to a DRAFT release for a maintainer to review and
publish by hand. See [Windows setup](../packaging/windows/WINDOWS.md) for the installer/zip; the engine
packages are not run on a real Windows PC as part of CI.

Engine package layout: `bin/kimo-mcp` (or `bin/kimo-mcp.exe`), `fonts/*.ttf` (+ `fetch.sh`),
`README.md`, `FEATURES.md`, this file. Keep `fonts/` next to `bin/`; Kimo finds it from the executable
path (also `KIMO_FONTS=<dir>`, `./fonts`, `~/kimo/fonts`, `/usr/share/fonts`).

## What a fresh box needs

- Nothing to link: the default build has no OCIO, no C/C++ runtime deps. `kimo-mcp` speaks MCP over stdio.
- **`ffmpeg` and `ffprobe` on PATH** (required at runtime): all media decode/encode, audio, mattes, export
  shell out to them. They are NOT bundled. Install with e.g. `apt install ffmpeg`.
- Optional: `whisper` on PATH, only for the transcription command.
- Fonts: bundled OFL Google Fonts (downloaded at release time from github.com/google/fonts). If a font is
  missing, Kimo falls back to Montserrat, then `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf`.
  A text command with its default font path `/usr/share/fonts/truetype/noto/...` needs that file or an explicit font name.
- Optional env: `KIMO_FONTS`, `KIMO_THREADS` (max 16), guard limits in `guard.rs`.

## OCIO (native-ocio feature) is NOT in release binaries

`--features native-ocio` links the system shared `libOpenColorIO` (OCIO 2.x) and needs `OCIO_INCLUDE_DIR`
and `OCIO_LIB_DIR` at build time (see `docs/OCIO_NATIVE.md`); it cannot be static-musl. Release builds omit it, so
explicit OCIO effects fail closed. No `.ocio` config or data file is shipped or required by default builds;
OCIO configs are user-supplied, self-contained files passed by path. Shipping a native-ocio build would need
a bundled libOpenColorIO plus matching headers on the build runner, which is not set up.

## Cutting a release

    git tag v6.0.0 && git push origin v6.0.0     # builds everything and opens a DRAFT release to review
    # or: Actions > release > Run workflow on a v* ref, with "draft_release" ticked to attach a draft
    #     (left off, it's a dry run: workflow artifacts + SHA256SUMS only, no release)

Build locally: `cargo build --release --locked -p kimo-mcp --target x86_64-unknown-linux-gnu`
(the glibc target this release ships; no musl target is built or shipped).
