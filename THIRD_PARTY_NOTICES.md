# Third-party notices

Kimo is MIT licensed (see LICENSE). The files below under `crates/kimo-core/src/dsp/` include or derive from the projects named here. Each file carries a provenance header at the top.

## SoundCraft

- Source: https://github.com/storytold/soundcraft, `crates/dsp/src/offline.rs` (commit `eac0edd`)
- Copyright (c) 2026 ArtCraft Team and the SoundCraft contributors
- License: MIT OR Apache-2.0, used here under the MIT License (reproduced below)
- Changes: ported to Rust edition 2021 and mono slices; the kernel table and accumulation run in f64; typed errors and resource guards added. The ArtCraft name and logos are trademarks and are not used.

## beatbox-rs

- Source: https://github.com/naman-sharma12345/beatbox-rs, `src/resample.rs` (commit `907fb0a1c8a30c245facba58ea27df8366080526`), MIT, the SoundCraft adaptation above.

| Kimo file | Origin | What |
|---|---|---|
| `crates/kimo-core/src/dsp/resample.rs` | beatbox-rs `src/resample.rs`, from SoundCraft `crates/dsp/src/offline.rs` (`SincTable`, `resample_channel`, `resample_ratio`, `bessel_i0`) | Kaiser windowed-sinc resampler and true-peak helper |
| `crates/kimo-core/src/dsp/fft.rs` | Kimo's own code, moved from `perception/audio_events.rs` | Radix-2 1024-point FFT |
| `crates/kimo-core/src/dsp/mod.rs` | Kimo's own module file | Module index |

```
MIT License

Copyright (c) 2026 ArtCraft Team and the SoundCraft contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
