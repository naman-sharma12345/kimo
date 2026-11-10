# Third-party software in Kimo Spectator (`crates/kimo-gui`)

This file covers the desktop spectator app and its installers (`.deb`, `.AppImage`, `.msi` / NSIS `.exe`). macOS is not built in this release; its crates are still listed so the list stays complete when it is.
The engine (`kimo-core`, `kimo-mcp`, `kimo-cli`) is a separate Cargo workspace and is not covered here.

Kimo itself is MIT licensed (see [LICENSE](LICENSE)).

## How this list was made

`crates/kimo-gui` is its own Cargo workspace and its `Cargo.lock` is deliberately not committed until the
MPL-2.0 decision below is made. The list was generated on 2026-10-17 from a lock that resolves
`tauri 2.12.4`, `tauri-build 2.7.3`, `wry 0.57.0`, `tao 0.37.1`, with:

```
cd crates/kimo-gui
cargo metadata --format-version 1 --locked > /dev/null
cargo tree --locked -e normal,build --target all --prefix none --format "{p}|{l}"
cargo tree --locked -e normal,build --target <triple> --prefix none --format "{p}|{l}"   # per platform
```

Because the lock is not committed, a fresh build (including the release workflow) resolves the newest
semver-compatible versions, which can differ from the ones below. Re-run the commands above to refresh.
Dev-dependencies are excluded: nothing in this list is a test-only crate.

**379 crates** across all platforms (Linux 261, Windows 220, macOS 217).
Licence expressions as declared by each crate:

| Licence expression | Crates |
|---|---|
| `MIT OR Apache-2.0` | 187 |
| `MIT` | 96 |
| `Apache-2.0 OR MIT` | 29 |
| `Unicode-3.0` | 18 |
| `Zlib OR Apache-2.0 OR MIT` | 15 |
| `Unlicense OR MIT` | 6 |
| `MPL-2.0` | 5 |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | 3 |
| `BSD-3-Clause OR MIT OR Apache-2.0` | 2 |
| `MIT OR Zlib OR Apache-2.0` | 2 |
| `Apache-2.0` | 2 |
| `MIT OR Apache-2.0 OR Zlib` | 2 |
| `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | 2 |
| `BSD-3-Clause` | 2 |
| `Apache-2.0 AND MIT` | 1 |
| `BSD-3-Clause OR MIT` | 1 |
| `Zlib` | 1 |
| `BSD-3-Clause AND MIT` | 1 |
| `CC0-1.0 OR MIT-0 OR Apache-2.0` | 1 |
| `0BSD OR MIT OR Apache-2.0` | 1 |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 |
| `Apache-2.0 WITH LLVM-exception` | 1 |

Everything is permissive (MIT, Apache-2.0, BSD, Zlib, Unicode-3.0, Unlicense, CC0, 0BSD) or offers a permissive
choice, with one exception: the five MPL-2.0 crates below.

## MPL-2.0 crates: decision pending (Naman)

These come with the Tauri / wry stack. They are file-level copyleft (MPL-2.0): using them unmodified in an MIT
app is allowed, but modified MPL files would have to be published. Kimo uses them as unmodified dependencies and does not modify them. Whether to accept
them is an open decision; until then they are listed here and no other non-permissive crate is added.

| Crate | Version | Licence | Pulled in by | Platforms | Source |
|---|---|---|---|---|---|
| `cssparser` | v0.37.0 | MPL-2.0, decision pending (Naman) | tauri-utils → dom_query → cssparser (Tauri HTML/CSP processing at build time and in tauri) | all | [crates.io cssparser 0.37.0](https://crates.io/crates/cssparser/0.37.0), repo: https://github.com/servo/rust-cssparser |
| `cssparser-macros` | v0.7.1 | MPL-2.0, decision pending (Naman) | cssparser → cssparser-macros (proc-macro, build time only, not linked into the app) | all | [crates.io cssparser-macros 0.7.1](https://crates.io/crates/cssparser-macros/0.7.1), repo: https://github.com/servo/rust-cssparser |
| `dtoa-short` | v0.3.5 | MPL-2.0, decision pending (Naman) | cssparser → dtoa-short | all | [crates.io dtoa-short 0.3.5](https://crates.io/crates/dtoa-short/0.3.5), repo: https://github.com/upsuper/dtoa-short |
| `option-ext` | v0.2.0 | MPL-2.0, decision pending (Naman) | dirs → dirs-sys → option-ext (dirs is used by tauri, tauri-build and wry) | all | [crates.io option-ext 0.2.0](https://crates.io/crates/option-ext/0.2.0), repo: https://github.com/soc/option-ext |
| `selectors` | v0.38.0 | MPL-2.0, decision pending (Naman) | tauri-utils → dom_query → selectors | all | [crates.io selectors 0.38.0](https://crates.io/crates/selectors/0.38.0), repo: https://github.com/servo/stylo |

## LGPL system libraries bundled in the AppImage: decision pending (Naman)

The `.AppImage` copies these Linux libraries in as unmodified, dynamically linked shared libraries (taken from
the Ubuntu 22.04 runner's packages; their Debian copyright files ride along under `usr/share/doc/<package>/` inside
the AppImage). The `.deb` does not ship them; it depends on the system packages. Versions were read on 2026-10-17
from the 0.1.0 CI AppImage (release dry run on `ca0334f`): from the library's own version functions or version
strings, and checked against the Ubuntu 22.04 package published at that version. A later build can bundle newer
22.04 updates.

| Library (files in `usr/lib`) | Upstream version | Ubuntu 22.04 source package | Licence | Source | Status |
|---|---|---|---|---|---|
| WebKitGTK 4.1 (`libwebkit2gtk-4.1.so.0`, `libjavascriptcoregtk-4.1.so.0`, `WebKitWebProcess`, `WebKitNetworkProcess`) | 2.50.4 | `webkit2gtk` 2.50.4-0ubuntu0.22.04.1 | LGPL-2.1+ with BSD-2-Clause parts | https://webkitgtk.org/releases/webkitgtk-2.50.4.tar.xz, https://launchpad.net/ubuntu/+source/webkit2gtk/2.50.4-0ubuntu0.22.04.1 | LGPL, decision pending (Naman) |
| GTK 3 (`libgtk-3.so.0`, `libgdk-3.so.0`, im/print modules) | 3.24.33 | `gtk+3.0` 3.24.33-1ubuntu2.2 | LGPL | https://download.gnome.org/sources/gtk+/3.24/gtk+-3.24.33.tar.xz, https://launchpad.net/ubuntu/+source/gtk+3.0/3.24.33-1ubuntu2.2 | LGPL, decision pending (Naman) |
| GLib (`libglib-2.0`, `libgobject-2.0`, `libgio-2.0`) | 2.72.4 | `glib2.0` 2.72.4-0ubuntu2.10 | LGPL | https://download.gnome.org/sources/glib/2.72/glib-2.72.4.tar.xz, https://launchpad.net/ubuntu/+source/glib2.0/2.72.4-0ubuntu2.10 | LGPL, decision pending (Naman) |
| libsoup 3 (`libsoup-3.0.so.0`) | 3.0.7 | `libsoup3` 3.0.7-0ubuntu1 | LGPL | https://download.gnome.org/sources/libsoup/3.0/libsoup-3.0.7.tar.xz, https://launchpad.net/ubuntu/+source/libsoup3/3.0.7-0ubuntu1 | LGPL, decision pending (Naman) |
| GStreamer core (`libgstreamer-1.0`, `libgstbase-1.0`) | 1.20.3 | `gstreamer1.0` 1.20.3-0ubuntu1.1 | LGPL | https://gstreamer.freedesktop.org/src/gstreamer/gstreamer-1.20.3.tar.xz, https://launchpad.net/ubuntu/+source/gstreamer1.0/1.20.3-0ubuntu1.1 | LGPL, decision pending (Naman) |
| GStreamer plugins-base libraries (`libgstapp`, `libgstaudio`, `libgstvideo`, `libgstgl`, `libgstpbutils`, `libgsttag`, `libgstallocators`, `libgstfft`) | 1.20.1 (package version; no version string in the binary) | `gst-plugins-base1.0` 1.20.1-1ubuntu0.7 | LGPL | https://gstreamer.freedesktop.org/src/gst-plugins-base/gst-plugins-base-1.20.1.tar.xz, https://launchpad.net/ubuntu/+source/gst-plugins-base1.0/1.20.1-1ubuntu0.7 | LGPL, decision pending (Naman) |

The other shared libraries in the AppImage (about 160 more entries: cairo, pango, harfbuzz, gdk-pixbuf, gnutls,
libgcrypt and so on) come from the same 22.04 packages under their own licences, recorded in the copyright files
inside the AppImage. They are not itemised here.

## Other licences worth naming

- `Unicode-3.0`: the ICU4X crates (`icu_*`, `zerovec`, `yoke`, `tinystr`, `litemap`, `writeable`, `zerofrom`, `zerotrie`, `potential_utf`), via URL handling. Permissive.
- `BSD-3-Clause`: `alloc-no-stdlib`, `alloc-stdlib`; `BSD-3-Clause AND MIT`: `brotli` (asset compression at build time). Permissive.
- `Zlib`: `foldhash`. Permissive.
- `Apache-2.0` only: `tao` (window/event loop), `sync_wrapper`. Permissive.
- `r-efi` offers `MIT OR Apache-2.0 OR LGPL-2.1-or-later`; Kimo takes it under MIT/Apache-2.0. It is only used on UEFI targets and is not linked into any desktop build.

## Bundler, installer tooling and system libraries (not Cargo crates)

| Component | Licence | Where it ends up |
|---|---|---|
| Tauri CLI 2.12.3 (`@tauri-apps/cli`, includes the Tauri bundler) | MIT OR Apache-2.0 | build tool only |
| tauri-action v1.0.0 (GitHub Action) | MIT OR Apache-2.0 | CI only |
| WiX Toolset v3 (downloaded by the bundler on the Windows runner) | MS-RL | build tool for the `.msi`; not verified here whether any WiX binary is carried inside the `.msi` |
| NSIS (fallback Windows installer) | zlib/libpng (installer stub) | its stub is part of the setup `.exe` |
| linuxdeploy and the AppImage type-2 runtime | MIT | the runtime is part of the `.AppImage` |
| WebKitGTK 4.1, JavaScriptCore, GTK 3, GLib, libsoup 3, GStreamer core libraries and their dependencies (Linux) | LGPL-2.1+ (WebKitGTK also BSD-2-Clause parts; dependencies under their own licences) | `.deb`: used from the system through package dependencies (`libwebkit2gtk-4.1-0`, `libgtk-3-0`), not shipped. `.AppImage`: copied in as shared libraries (checked in the 0.1.0 CI AppImage: `libwebkit2gtk-4.1.so.0`, `libjavascriptcoregtk-4.1.so.0`, `libgtk-3.so.0`, `libglib-2.0.so.0`, `libsoup-3.0.so.0`, `libgstreamer-1.0.so.0` and 163 more entries in `usr/lib`), dynamically linked and replaceable. Redistributing them carries the LGPL notice and source-offer duties: decision pending (Naman); versions and sources in the LGPL section above |
| Microsoft Edge WebView2 (Windows) | Microsoft licence | not shipped; the installer downloads Microsoft's bootstrapper at install time if WebView2 is missing |

## Assets inside the app

- Geist and Geist Mono fonts (`crates/kimo-gui/ui/fonts`): SIL Open Font License 1.1, licence text in `ui/fonts/OFL.txt`.
- App icons (`crates/kimo-gui/icons`): Kimo's own artwork, MIT with the rest of the repo.

## Full crate list

<details>
<summary>All crates (name, version, declared licence, platforms: L = Linux, W = Windows, M = macOS, - = none of the three desktop targets)</summary>

| Crate | Version | Licence | Platforms |
|---|---|---|---|
| `adler2` | 2.0.1 | `0BSD OR MIT OR Apache-2.0` | LWM |
| `aho-corasick` | 1.1.5 | `Unlicense OR MIT` | LWM |
| `alloc-no-stdlib` | 3.0.0 | `BSD-3-Clause` | LWM |
| `alloc-stdlib` | 0.3.0 | `BSD-3-Clause` | LWM |
| `anyhow` | 1.0.104 | `MIT OR Apache-2.0` | LWM |
| `atk` | 0.18.2 | `MIT` | L |
| `atk-sys` | 0.18.2 | `MIT` | L |
| `atomic-waker` | 1.1.2 | `Apache-2.0 OR MIT` | - |
| `autocfg` | 1.5.1 | `Apache-2.0 OR MIT` | LWM |
| `base64` | 0.21.7 | `MIT OR Apache-2.0` | - |
| `base64` | 0.22.1 | `MIT OR Apache-2.0` | - |
| `base64` | 0.23.1 | `MIT OR Apache-2.0` | LWM |
| `bit-set` | 0.8.0 | `Apache-2.0 OR MIT` | LWM |
| `bit-vec` | 0.8.0 | `Apache-2.0 OR MIT` | LWM |
| `bitflags` | 1.3.2 | `MIT/Apache-2.0` | LWM |
| `bitflags` | 2.13.2 | `MIT OR Apache-2.0` | LWM |
| `block-buffer` | 0.10.4 | `MIT OR Apache-2.0` | LWM |
| `block2` | 0.6.2 | `MIT` | M |
| `brotli` | 9.0.0 | `BSD-3-Clause AND MIT` | LWM |
| `brotli-decompressor` | 6.0.1 | `BSD-3-Clause/MIT` | LWM |
| `bumpalo` | 3.20.3 | `MIT OR Apache-2.0` | - |
| `bytemuck` | 1.25.2 | `Zlib OR Apache-2.0 OR MIT` | - |
| `byteorder` | 1.5.0 | `Unlicense OR MIT` | LWM |
| `bytes` | 1.12.1 | `MIT` | LWM |
| `cairo-rs` | 0.18.5 | `MIT` | L |
| `cairo-sys-rs` | 0.18.2 | `MIT` | L |
| `camino` | 1.2.6 | `MIT OR Apache-2.0` | LWM |
| `cargo-platform` | 0.1.9 | `MIT OR Apache-2.0` | LWM |
| `cargo_metadata` | 0.19.2 | `MIT` | LWM |
| `cargo_toml` | 1.0.1 | `Apache-2.0 OR MIT` | LWM |
| `cc` | 1.8.0 | `MIT OR Apache-2.0` | LWM |
| `cesu8` | 1.1.0 | `Apache-2.0/MIT` | - |
| `cfb` | 0.14.0 | `MIT` | LWM |
| `cfg-expr` | 0.15.8 | `MIT OR Apache-2.0` | L |
| `cfg-if` | 1.0.5 | `MIT OR Apache-2.0` | LWM |
| `combine` | 4.6.9 | `MIT` | - |
| `cookie` | 0.18.2 | `MIT OR Apache-2.0` | LWM |
| `core-foundation` | 0.10.1 | `MIT OR Apache-2.0` | M |
| `core-foundation-sys` | 0.8.7 | `MIT OR Apache-2.0` | M |
| `core-graphics` | 0.25.0 | `MIT OR Apache-2.0` | M |
| `core-graphics-types` | 0.2.0 | `MIT OR Apache-2.0` | M |
| `cpufeatures` | 0.2.17 | `MIT OR Apache-2.0` | LWM |
| `crc32fast` | 1.5.2 | `MIT OR Apache-2.0` | LWM |
| `crossbeam-channel` | 0.5.17 | `MIT OR Apache-2.0` | LWM |
| `crossbeam-utils` | 0.8.23 | `MIT OR Apache-2.0` | LWM |
| `crypto-common` | 0.1.7 | `MIT OR Apache-2.0` | LWM |
| `cssparser` | 0.37.0 | `MPL-2.0` | LWM |
| `cssparser-macros` (proc-macro) | 0.7.1 | `MPL-2.0` | LWM |
| `ctor` | 1.0.13 | `Apache-2.0 OR MIT` | LWM |
| `darling` | 0.24.1 | `MIT` | LWM |
| `darling_core` | 0.24.1 | `MIT` | LWM |
| `darling_macro` (proc-macro) | 0.24.1 | `MIT` | LWM |
| `dbus` | 0.9.12 | `Apache-2.0/MIT` | L |
| `deranged` | 0.5.9 | `MIT OR Apache-2.0` | LWM |
| `derive_more` | 2.1.1 | `MIT` | LWM |
| `derive_more-impl` (proc-macro) | 2.1.1 | `MIT` | LWM |
| `digest` | 0.10.7 | `MIT OR Apache-2.0` | LWM |
| `dirs` | 7.0.0 | `MIT OR Apache-2.0` | LWM |
| `dirs-sys` | 0.5.0 | `MIT OR Apache-2.0` | LWM |
| `dispatch2` | 0.3.1 | `Zlib OR Apache-2.0 OR MIT` | M |
| `displaydoc` (proc-macro) | 0.2.7 | `MIT OR Apache-2.0` | LWM |
| `dlopen2` | 0.8.2 | `MIT` | L |
| `dlopen2_derive` (proc-macro) | 0.4.3 | `MIT` | L |
| `dom_query` | 0.28.0 | `MIT` | LWM |
| `dpi` | 0.1.2 | `Apache-2.0 AND MIT` | LWM |
| `dtoa` | 1.0.11 | `MIT OR Apache-2.0` | LWM |
| `dtoa-short` | 0.3.5 | `MPL-2.0` | LWM |
| `dunce` | 1.0.5 | `CC0-1.0 OR MIT-0 OR Apache-2.0` | LWM |
| `dyn-clone` | 1.0.20 | `MIT OR Apache-2.0` | LWM |
| `embed-resource` | 3.0.12 | `MIT` | LWM |
| `embed_plist` | 1.2.2 | `MIT OR Apache-2.0` | M |
| `equivalent` | 1.0.3 | `Apache-2.0 OR MIT` | LWM |
| `erased-serde` | 0.4.10 | `MIT OR Apache-2.0` | LWM |
| `fastrand` | 2.5.0 | `Apache-2.0 OR MIT` | LWM |
| `fdeflate` | 0.3.7 | `MIT OR Apache-2.0` | LWM |
| `field-offset` | 0.3.6 | `MIT OR Apache-2.0` | L |
| `find-msvc-tools` | 0.1.14 | `MIT OR Apache-2.0` | LWM |
| `flate2` | 1.1.10 | `MIT OR Apache-2.0` | LWM |
| `fnv` | 1.0.7 | `Apache-2.0 / MIT` | LWM |
| `foldhash` | 0.2.0 | `Zlib` | LWM |
| `foreign-types` | 0.5.0 | `MIT/Apache-2.0` | M |
| `foreign-types-macros` (proc-macro) | 0.2.4 | `MIT/Apache-2.0` | M |
| `foreign-types-shared` | 0.3.1 | `MIT/Apache-2.0` | M |
| `form_urlencoded` | 1.2.2 | `MIT OR Apache-2.0` | LWM |
| `futures-channel` | 0.3.34 | `MIT OR Apache-2.0` | L |
| `futures-core` | 0.3.34 | `MIT OR Apache-2.0` | L |
| `futures-executor` | 0.3.34 | `MIT OR Apache-2.0` | L |
| `futures-io` | 0.3.34 | `MIT OR Apache-2.0` | L |
| `futures-macro` (proc-macro) | 0.3.34 | `MIT OR Apache-2.0` | L |
| `futures-sink` | 0.3.34 | `MIT OR Apache-2.0` | - |
| `futures-task` | 0.3.34 | `MIT OR Apache-2.0` | L |
| `futures-util` | 0.3.34 | `MIT OR Apache-2.0` | L |
| `gdk` | 0.18.2 | `MIT` | L |
| `gdk-pixbuf` | 0.18.5 | `MIT` | L |
| `gdk-pixbuf-sys` | 0.18.0 | `MIT` | L |
| `gdk-sys` | 0.18.2 | `MIT` | L |
| `gdkwayland-sys` | 0.18.2 | `MIT` | L |
| `gdkx11` | 0.18.2 | `MIT` | L |
| `gdkx11-sys` | 0.18.2 | `MIT` | L |
| `generic-array` | 0.14.7 | `MIT` | LWM |
| `getrandom` | 0.3.4 | `MIT OR Apache-2.0` | LWM |
| `getrandom` | 0.4.3 | `MIT OR Apache-2.0` | LWM |
| `gio` | 0.18.4 | `MIT` | L |
| `gio-sys` | 0.18.1 | `MIT` | L |
| `glib` | 0.18.5 | `MIT` | L |
| `glib-macros` (proc-macro) | 0.18.5 | `MIT` | L |
| `glib-sys` | 0.18.1 | `MIT` | L |
| `glob` | 0.3.4 | `MIT OR Apache-2.0` | LWM |
| `gobject-sys` | 0.18.0 | `MIT` | L |
| `gtk` | 0.18.2 | `MIT` | L |
| `gtk-sys` | 0.18.2 | `MIT` | L |
| `gtk3-macros` (proc-macro) | 0.18.2 | `MIT` | L |
| `hashbrown` | 0.12.3 | `MIT OR Apache-2.0` | LWM |
| `hashbrown` | 0.17.1 | `MIT OR Apache-2.0` | LWM |
| `heck` | 0.4.1 | `MIT OR Apache-2.0` | L |
| `heck` | 0.5.0 | `MIT OR Apache-2.0` | LWM |
| `html5ever` | 0.39.0 | `MIT OR Apache-2.0` | LWM |
| `http` | 1.5.0 | `MIT OR Apache-2.0` | LWM |
| `http-body` | 1.1.0 | `MIT` | - |
| `http-body-util` | 0.1.5 | `MIT` | - |
| `httparse` | 1.10.1 | `MIT OR Apache-2.0` | - |
| `hyper` | 1.12.0 | `MIT` | - |
| `hyper-util` | 0.1.21 | `MIT` | - |
| `ico` | 0.5.0 | `MIT` | LWM |
| `icu_collections` | 2.3.0 | `Unicode-3.0` | LWM |
| `icu_locale_core` | 2.3.0 | `Unicode-3.0` | LWM |
| `icu_normalizer` | 2.3.0 | `Unicode-3.0` | LWM |
| `icu_normalizer_data` | 2.3.0 | `Unicode-3.0` | LWM |
| `icu_properties` | 2.3.0 | `Unicode-3.0` | LWM |
| `icu_properties_data` | 2.3.0 | `Unicode-3.0` | LWM |
| `icu_provider` | 2.3.1 | `Unicode-3.0` | LWM |
| `ident_case` | 1.0.1 | `MIT/Apache-2.0` | LWM |
| `idna` | 1.1.0 | `MIT OR Apache-2.0` | LWM |
| `idna_adapter` | 1.2.2 | `Apache-2.0 OR MIT` | LWM |
| `indexmap` | 1.9.3 | `Apache-2.0 OR MIT` | LWM |
| `indexmap` | 2.14.2 | `Apache-2.0 OR MIT` | LWM |
| `infer` | 0.22.0 | `MIT` | LWM |
| `ipnet` | 2.12.2 | `MIT OR Apache-2.0` | - |
| `itoa` | 1.0.18 | `MIT OR Apache-2.0` | LWM |
| `javascriptcore-rs` | 1.1.2 | `MIT` | L |
| `javascriptcore-rs-sys` | 1.1.1 | `MIT` | L |
| `jni` | 0.21.1 | `MIT/Apache-2.0` | - |
| `jni-sys` | 0.3.1 | `MIT OR Apache-2.0` | - |
| `jni-sys` | 0.4.1 | `MIT OR Apache-2.0` | - |
| `jni-sys-macros` (proc-macro) | 0.4.1 | `MIT OR Apache-2.0` | - |
| `js-sys` | 0.3.106 | `MIT OR Apache-2.0` | - |
| `json-patch` | 4.2.0 | `MIT/Apache-2.0` | LWM |
| `jsonptr` | 0.7.1 | `MIT OR Apache-2.0` | LWM |
| `keyboard-types` | 0.8.3 | `MIT OR Apache-2.0` | LWM |
| `libc` | 0.2.190 | `MIT OR Apache-2.0` | LWM |
| `libdbus-sys` | 0.2.7 | `Apache-2.0/MIT` | L |
| `libredox` | 0.1.25 | `MIT` | - |
| `litemap` | 0.8.3 | `Unicode-3.0` | LWM |
| `lock_api` | 0.4.14 | `MIT OR Apache-2.0` | LWM |
| `log` | 0.4.34 | `MIT OR Apache-2.0` | LWM |
| `markup5ever` | 0.39.0 | `MIT OR Apache-2.0` | LWM |
| `memchr` | 2.8.3 | `Unlicense OR MIT` | LWM |
| `memoffset` | 0.9.1 | `MIT` | L |
| `mime` | 0.3.17 | `MIT OR Apache-2.0` | LWM |
| `miniz_oxide` | 0.8.9 | `MIT OR Zlib OR Apache-2.0` | LWM |
| `miniz_oxide` | 0.9.1 | `MIT OR Zlib OR Apache-2.0` | LWM |
| `mio` | 1.2.4 | `MIT` | - |
| `muda` | 0.20.0 | `Apache-2.0 OR MIT` | LWM |
| `ndk` | 0.9.0 | `MIT OR Apache-2.0` | - |
| `ndk-context` | 0.1.1 | `MIT OR Apache-2.0` | - |
| `ndk-sys` | 0.6.0+11769913 | `MIT OR Apache-2.0` | - |
| `new_debug_unreachable` | 1.0.6 | `MIT` | LWM |
| `num-conv` | 0.2.2 | `MIT OR Apache-2.0` | LWM |
| `num_enum` | 0.7.6 | `BSD-3-Clause OR MIT OR Apache-2.0` | - |
| `num_enum_derive` (proc-macro) | 0.7.6 | `BSD-3-Clause OR MIT OR Apache-2.0` | - |
| `objc2` | 0.6.5 | `MIT` | M |
| `objc2-app-kit` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | M |
| `objc2-cloud-kit` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | - |
| `objc2-core-data` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | - |
| `objc2-core-foundation` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | M |
| `objc2-core-graphics` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | - |
| `objc2-core-image` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | - |
| `objc2-core-location` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | - |
| `objc2-core-text` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | - |
| `objc2-encode` | 4.1.0 | `MIT` | M |
| `objc2-exception-helper` | 0.1.1 | `Zlib OR Apache-2.0 OR MIT` | M |
| `objc2-foundation` | 0.3.2 | `MIT` | M |
| `objc2-quartz-core` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | M |
| `objc2-ui-kit` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | - |
| `objc2-user-notifications` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | - |
| `objc2-web-kit` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | M |
| `once_cell` | 1.21.4 | `MIT OR Apache-2.0` | LWM |
| `option-ext` | 0.2.0 | `MPL-2.0` | LWM |
| `pango` | 0.18.3 | `MIT` | L |
| `pango-sys` | 0.18.0 | `MIT` | L |
| `parking_lot` | 0.12.5 | `MIT OR Apache-2.0` | LWM |
| `parking_lot_core` | 0.9.12 | `MIT OR Apache-2.0` | LWM |
| `percent-encoding` | 2.3.2 | `MIT OR Apache-2.0` | LWM |
| `phf` | 0.13.1 | `MIT` | LWM |
| `phf_codegen` | 0.13.1 | `MIT` | LWM |
| `phf_generator` | 0.13.1 | `MIT` | LWM |
| `phf_macros` (proc-macro) | 0.13.1 | `MIT` | LWM |
| `phf_shared` | 0.13.1 | `MIT` | LWM |
| `pin-project-lite` | 0.2.17 | `Apache-2.0 OR MIT` | LWM |
| `pkg-config` | 0.3.34 | `MIT OR Apache-2.0` | L |
| `plist` | 1.10.1 | `MIT` | LWM |
| `png` | 0.17.16 | `MIT OR Apache-2.0` | LWM |
| `png` | 0.18.1 | `MIT OR Apache-2.0` | LWM |
| `potential_utf` | 0.1.6 | `Unicode-3.0` | LWM |
| `powerfmt` | 0.2.1 | `MIT OR Apache-2.0` | LWM |
| `precomputed-hash` | 0.1.1 | `MIT` | LWM |
| `proc-macro-crate` | 1.3.1 | `MIT OR Apache-2.0` | L |
| `proc-macro-crate` | 2.0.2 | `MIT OR Apache-2.0` | L |
| `proc-macro-crate` | 3.5.0 | `MIT OR Apache-2.0` | - |
| `proc-macro-error` | 1.0.4 | `MIT OR Apache-2.0` | L |
| `proc-macro-error-attr` (proc-macro) | 1.0.4 | `MIT OR Apache-2.0` | L |
| `proc-macro2` | 1.0.107 | `MIT OR Apache-2.0` | LWM |
| `quick-xml` | 0.42.0 | `MIT` | LWM |
| `quote` | 1.0.47 | `MIT OR Apache-2.0` | LWM |
| `r-efi` | 5.3.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | - |
| `r-efi` | 6.0.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | - |
| `raw-window-handle` | 0.6.2 | `MIT OR Apache-2.0 OR Zlib` | LWM |
| `redox_syscall` | 0.5.18 | `MIT` | - |
| `redox_users` | 0.5.3 | `MIT` | - |
| `regex` | 1.13.1 | `MIT OR Apache-2.0` | LWM |
| `regex-automata` | 0.4.18 | `MIT OR Apache-2.0` | LWM |
| `regex-syntax` | 0.8.11 | `MIT OR Apache-2.0` | LWM |
| `reqwest` | 0.13.5 | `MIT OR Apache-2.0` | - |
| `rustc-hash` | 2.1.3 | `Apache-2.0 OR MIT` | LWM |
| `rustc_version` | 0.4.1 | `MIT OR Apache-2.0` | LWM |
| `rustversion` (proc-macro) | 1.0.23 | `MIT OR Apache-2.0` | - |
| `same-file` | 1.0.6 | `Unlicense/MIT` | LWM |
| `schemars` | 0.8.22 | `MIT` | LWM |
| `schemars_derive` (proc-macro) | 0.8.22 | `MIT` | LWM |
| `scopeguard` | 1.2.0 | `MIT OR Apache-2.0` | LWM |
| `selectors` | 0.38.0 | `MPL-2.0` | LWM |
| `semver` | 1.0.28 | `MIT OR Apache-2.0` | LWM |
| `serde` | 1.0.229 | `MIT OR Apache-2.0` | LWM |
| `serde-untagged` | 0.1.9 | `MIT OR Apache-2.0` | LWM |
| `serde_core` | 1.0.229 | `MIT OR Apache-2.0` | LWM |
| `serde_derive` (proc-macro) | 1.0.229 | `MIT OR Apache-2.0` | LWM |
| `serde_derive_internals` | 0.29.1 | `MIT OR Apache-2.0` | LWM |
| `serde_json` | 1.0.154 | `MIT OR Apache-2.0` | LWM |
| `serde_repr` (proc-macro) | 0.1.21 | `MIT OR Apache-2.0` | LWM |
| `serde_spanned` | 0.6.9 | `MIT OR Apache-2.0` | L |
| `serde_spanned` | 1.1.2 | `MIT OR Apache-2.0` | LWM |
| `serde_with` | 3.24.0 | `MIT OR Apache-2.0` | LWM |
| `serde_with_macros` (proc-macro) | 3.24.0 | `MIT OR Apache-2.0` | LWM |
| `serialize-to-javascript` | 0.1.2 | `MIT OR Apache-2.0` | LWM |
| `serialize-to-javascript-impl` (proc-macro) | 0.1.2 | `MIT OR Apache-2.0` | LWM |
| `servo_arc` | 0.4.3 | `MIT OR Apache-2.0` | LWM |
| `sha2` | 0.10.9 | `MIT OR Apache-2.0` | LWM |
| `shlex` | 2.0.1 | `MIT OR Apache-2.0` | LWM |
| `simd-adler32` | 0.3.10 | `MIT` | LWM |
| `siphasher` | 1.0.4 | `MIT OR Apache-2.0` | LWM |
| `slab` | 0.4.12 | `MIT` | L |
| `smallvec` | 1.16.3 | `MIT OR Apache-2.0` | LWM |
| `socket2` | 0.6.5 | `MIT OR Apache-2.0` | - |
| `softbuffer` | 0.4.8 | `MIT OR Apache-2.0` | W |
| `soup3` | 0.5.0 | `MIT` | L |
| `soup3-sys` | 0.5.0 | `MIT` | L |
| `stable_deref_trait` | 1.2.1 | `MIT OR Apache-2.0` | LWM |
| `static_vcruntime` | 3.0.0 | `MIT OR Apache-2.0 OR Zlib` | LWM |
| `string_cache` | 0.9.0 | `MIT OR Apache-2.0` | LWM |
| `string_cache_codegen` | 0.6.1 | `MIT OR Apache-2.0` | LWM |
| `strsim` | 0.11.1 | `MIT` | LWM |
| `swift-rs` | 1.0.9 | `MIT OR Apache-2.0` | - |
| `syn` | 1.0.109 | `MIT OR Apache-2.0` | L |
| `syn` | 2.0.119 | `MIT OR Apache-2.0` | LWM |
| `syn` | 3.0.9 | `MIT OR Apache-2.0` | LWM |
| `sync_wrapper` | 1.0.2 | `Apache-2.0` | - |
| `synstructure` | 0.14.0 | `MIT` | LWM |
| `system-deps` | 6.2.2 | `MIT OR Apache-2.0` | L |
| `tao` | 0.37.1 | `Apache-2.0` | LWM |
| `tao-macros` (proc-macro) | 0.1.4 | `MIT OR Apache-2.0` | - |
| `target-lexicon` | 0.12.16 | `Apache-2.0 WITH LLVM-exception` | L |
| `tauri` | 2.12.4 | `Apache-2.0 OR MIT` | LWM |
| `tauri-build` | 2.7.3 | `Apache-2.0 OR MIT` | LWM |
| `tauri-codegen` | 2.7.3 | `Apache-2.0 OR MIT` | LWM |
| `tauri-macros` (proc-macro) | 2.7.3 | `Apache-2.0 OR MIT` | LWM |
| `tauri-runtime` | 2.12.2 | `Apache-2.0 OR MIT` | LWM |
| `tauri-runtime-wry` | 2.12.2 | `Apache-2.0 OR MIT` | LWM |
| `tauri-utils` | 2.10.2 | `Apache-2.0 OR MIT` | LWM |
| `tauri-winres` | 0.3.6 | `MIT` | LWM |
| `tendril` | 0.5.1 | `MIT OR Apache-2.0` | LWM |
| `thiserror` | 1.0.69 | `MIT OR Apache-2.0` | L |
| `thiserror` | 2.0.21 | `MIT OR Apache-2.0` | LWM |
| `thiserror-impl` (proc-macro) | 1.0.69 | `MIT OR Apache-2.0` | L |
| `thiserror-impl` (proc-macro) | 2.0.21 | `MIT OR Apache-2.0` | LWM |
| `time` | 0.3.55 | `MIT OR Apache-2.0` | LWM |
| `time-core` | 0.1.9 | `MIT OR Apache-2.0` | LWM |
| `time-macros` (proc-macro) | 0.2.32 | `MIT OR Apache-2.0` | LWM |
| `tinystr` | 0.8.4 | `Unicode-3.0` | LWM |
| `tokio` | 1.53.3 | `MIT` | LWM |
| `tokio-util` | 0.7.20 | `MIT` | - |
| `toml` | 0.8.2 | `MIT OR Apache-2.0` | L |
| `toml` | 1.1.8+spec-1.1.0 | `MIT OR Apache-2.0` | LWM |
| `toml_datetime` | 0.6.3 | `MIT OR Apache-2.0` | L |
| `toml_datetime` | 1.1.2+spec-1.1.0 | `MIT OR Apache-2.0` | LWM |
| `toml_edit` | 0.19.15 | `MIT OR Apache-2.0` | L |
| `toml_edit` | 0.20.2 | `MIT OR Apache-2.0` | L |
| `toml_edit` | 0.25.17+spec-1.1.0 | `MIT OR Apache-2.0` | - |
| `toml_parser` | 1.1.5+spec-1.1.0 | `MIT OR Apache-2.0` | LWM |
| `toml_writer` | 1.1.3+spec-1.1.0 | `MIT OR Apache-2.0` | LWM |
| `tower` | 0.5.3 | `MIT` | - |
| `tower-http` | 0.6.11 | `MIT` | - |
| `tower-layer` | 0.3.3 | `MIT` | - |
| `tower-service` | 0.3.3 | `MIT` | - |
| `tracing` | 0.1.44 | `MIT` | W |
| `tracing-core` | 0.1.36 | `MIT` | W |
| `try-lock` | 0.2.5 | `MIT` | - |
| `typeid` | 1.0.3 | `MIT OR Apache-2.0` | LWM |
| `typenum` | 1.20.1 | `MIT OR Apache-2.0` | LWM |
| `unicode-ident` | 1.0.27 | `(MIT OR Apache-2.0) AND Unicode-3.0` | LWM |
| `unicode-segmentation` | 1.13.3 | `MIT OR Apache-2.0` | W |
| `url` | 2.5.8 | `MIT OR Apache-2.0` | LWM |
| `urlpattern` | 0.6.0 | `MIT` | LWM |
| `utf8_iter` | 1.0.4 | `Apache-2.0 OR MIT` | LWM |
| `uuid` | 1.28.0 | `Apache-2.0 OR MIT` | LWM |
| `version-compare` | 0.2.1 | `MIT` | L |
| `version_check` | 0.9.5 | `MIT/Apache-2.0` | LWM |
| `vswhom` | 0.1.0 | `MIT` | - |
| `vswhom-sys` | 0.1.3 | `MIT` | - |
| `walkdir` | 2.5.0 | `Unlicense/MIT` | LWM |
| `want` | 0.3.2 | `MIT` | - |
| `wasi` | 0.11.1+wasi-snapshot-preview1 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | - |
| `wasip2` | 1.0.4+wasi-0.2.12 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | - |
| `wasm-bindgen` | 0.2.129 | `MIT OR Apache-2.0` | - |
| `wasm-bindgen-futures` | 0.4.79 | `MIT OR Apache-2.0` | - |
| `wasm-bindgen-macro` (proc-macro) | 0.2.129 | `MIT OR Apache-2.0` | - |
| `wasm-bindgen-macro-support` | 0.2.129 | `MIT OR Apache-2.0` | - |
| `wasm-bindgen-shared` | 0.2.129 | `MIT OR Apache-2.0` | - |
| `wasm-streams` | 0.5.0 | `MIT OR Apache-2.0` | - |
| `web-sys` | 0.3.106 | `MIT OR Apache-2.0` | - |
| `web-time` | 1.1.0 | `MIT OR Apache-2.0` | LWM |
| `web_atoms` | 0.2.6 | `MIT OR Apache-2.0` | LWM |
| `webkit2gtk` | 2.0.2 | `MIT` | L |
| `webkit2gtk-sys` | 2.0.2 | `MIT` | L |
| `webview2-com` | 0.39.1 | `MIT` | W |
| `webview2-com-macros` (proc-macro) | 0.8.1 | `MIT` | W |
| `webview2-com-sys` | 0.39.1 | `MIT` | W |
| `winapi` | 0.3.9 | `MIT/Apache-2.0` | - |
| `winapi-i686-pc-windows-gnu` | 0.4.0 | `MIT/Apache-2.0` | - |
| `winapi-util` | 0.1.11 | `Unlicense OR MIT` | W |
| `winapi-x86_64-pc-windows-gnu` | 0.4.0 | `MIT/Apache-2.0` | - |
| `window-vibrancy` | 0.8.1 | `Apache-2.0 OR MIT` | WM |
| `windows` | 0.62.2 | `MIT OR Apache-2.0` | W |
| `windows-collections` | 0.3.2 | `MIT OR Apache-2.0` | W |
| `windows-core` | 0.62.2 | `MIT OR Apache-2.0` | W |
| `windows-future` | 0.3.2 | `MIT OR Apache-2.0` | W |
| `windows-implement` (proc-macro) | 0.60.2 | `MIT OR Apache-2.0` | W |
| `windows-interface` (proc-macro) | 0.59.3 | `MIT OR Apache-2.0` | W |
| `windows-link` | 0.2.1 | `MIT OR Apache-2.0` | W |
| `windows-numerics` | 0.3.1 | `MIT OR Apache-2.0` | W |
| `windows-result` | 0.4.1 | `MIT OR Apache-2.0` | W |
| `windows-strings` | 0.5.1 | `MIT OR Apache-2.0` | W |
| `windows-sys` | 0.45.0 | `MIT OR Apache-2.0` | - |
| `windows-sys` | 0.61.2 | `MIT OR Apache-2.0` | W |
| `windows-targets` | 0.42.2 | `MIT OR Apache-2.0` | - |
| `windows-threading` | 0.2.1 | `MIT OR Apache-2.0` | W |
| `windows-version` | 0.1.7 | `MIT OR Apache-2.0` | W |
| `windows_aarch64_gnullvm` | 0.42.2 | `MIT OR Apache-2.0` | - |
| `windows_aarch64_msvc` | 0.42.2 | `MIT OR Apache-2.0` | - |
| `windows_i686_gnu` | 0.42.2 | `MIT OR Apache-2.0` | - |
| `windows_i686_msvc` | 0.42.2 | `MIT OR Apache-2.0` | - |
| `windows_x86_64_gnu` | 0.42.2 | `MIT OR Apache-2.0` | - |
| `windows_x86_64_gnullvm` | 0.42.2 | `MIT OR Apache-2.0` | - |
| `windows_x86_64_msvc` | 0.42.2 | `MIT OR Apache-2.0` | - |
| `winnow` | 0.5.40 | `MIT` | L |
| `winnow` | 1.0.4 | `MIT` | LWM |
| `winreg` | 0.56.0 | `MIT` | - |
| `wit-bindgen` | 0.57.1 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | - |
| `writeable` | 0.6.4 | `Unicode-3.0` | LWM |
| `wry` | 0.57.0 | `Apache-2.0 OR MIT` | LWM |
| `x11` | 2.21.0 | `MIT` | L |
| `x11-dl` | 2.21.0 | `MIT` | L |
| `yoke` | 0.8.3 | `Unicode-3.0` | LWM |
| `yoke-derive` (proc-macro) | 0.8.4 | `Unicode-3.0` | LWM |
| `zerofrom` | 0.1.8 | `Unicode-3.0` | LWM |
| `zerofrom-derive` (proc-macro) | 0.1.8 | `Unicode-3.0` | LWM |
| `zerotrie` | 0.2.5 | `Unicode-3.0` | LWM |
| `zerovec` | 0.11.8 | `Unicode-3.0` | LWM |
| `zerovec-derive` (proc-macro) | 0.11.6 | `Unicode-3.0` | LWM |
| `zmij` | 1.0.23 | `MIT` | LWM |

</details>
