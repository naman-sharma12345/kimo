# Code clips (Source::Code, codeclip.rs): provenance

- `src/codeclip.rs`, `src/codeclip_sidecar.py`, `src/ext/codeclip_tools.rs`: original kimo code (MIT). The renderFrame(t)
  contract and the Playwright + SwiftShader capture approach follow kimo's own music-video renderer; nothing was copied
  from a third party. FNV-1a (64- and 128-bit) is public-domain; its published offset basis and prime are used.
- Runtime dependencies, not vendored into the binary: Python 3, Playwright for Python (Apache-2.0), Pillow (MIT-CMU /
  HPND), and a Chromium build (BSD-3-Clause and others). Kimo calls them as an external process.
- `examples/codeclip_orbit/vendor/three/`: three.js r186 (`build/three.module.js`, `build/three.core.js`), unmodified,
  MIT licence, Copyright 2010-2026 three.js authors. The licence text ships next to it (`vendor/three/LICENSE`).
