# Display scope images

This is a separate opt-in analyzer, not a GUI or a change to the render pipeline.
Legacy `render_scope` remains unchanged. `render_display_scope` is registered in
the command registry and therefore exposed through the MCP server.

## API

`color::scope_image::render(&Frame, Options) -> Result<ScopeImage, String>` returns
an RGB8 PNG byte vector and JSON metadata. Frame RGB must match the explicitly
declared gamut and encoding. Rec709 and Rec2020 are supported. Linear, BT709,
sRGB, PQ and HLG encodings are supported; linear input requires nits mode.

Image size is fixed at 768x448. PNG encoding is native deterministic RGB8 with
stored zlib blocks, so there is no FFmpeg subprocess or new crate dependency.
The tradeoff is approximately 1 MB per PNG, rather than compressed-file size.

## MCP command

```
render_display_scope {
  "time": 0.5,
  "scope": "waveform",
  "path": "/tmp/scope.png",
  "units": "nits",
  "ceiling": 1000
}
```

Kinds: waveform (luma/luminance), parade (separate RGB panels), vectorscope
(encoded CbCr), histogram (RGB, count-normalized to global channel peak).
Optional `clip_id` selects an isolated clip, including its effects.

For managed projects, the command applies the configured output transform once,
then analyzes the output gamut/encoding. It rejects unsupported output gamuts
or transfer functions rather than relabeling them. Legacy assumes BT709 RGB.
It does not infer media metadata or change project defaults.

IRE means full-range encoded signal multiplied by 100. It is not legal-range
YUV or a broadcast-compliance measurement. Waveform/parade show labeled
horizontal graticules; histogram shows labeled signal/nits x graticules and
relative-count y graticules. Vectorscope has Cb/Cr axes, rings, and 100% primary
and secondary RGB targets computed for the selected gamut. Positive Cr is up.
Pure Rec709 red has Cb=-0.114572106, Cr=+0.5, angle about 102.907 degrees
(counterclockwise from positive Cb). Vectorscope is encoded-signal chroma,
including for PQ/HLG, not a perceptual HDR color-volume plot.

PQ nits use ST2084 absolute scaling. HLG nits use caller-declared
`hlg_peak_nits` (default 1000) and `hlg_system_gamma` (default 1.2): decoded scene
RGB times `peak * Y_scene^(gamma-1)`. This is a zero-black declared-display OOTF
assumption, not a measured/calibrated display. HLG IRE remains encoded HLG.
SDR/linear nits use the declared reference white.

Default units are nits for PQ/HLG/linear and IRE otherwise; vectorscope always
requires IRE. Default ceilings are 100 IRE, 10000 PQ nits, HLG declared peak,
or SDR/linear reference white. A narrower explicit ceiling improves visibility.

All valid visible pixels count once. Transparent pixels and invalid samples
are excluded and counted. Partial alpha is not weighted. Nonfinite RGB/alpha,
alpha outside [0,1], and PQ/HLG code values outside [0,1] are invalid. Finite
SDR/linear excursions are counted before plot-boundary clamping. Metadata
includes minima/maxima, under/over counts, histogram peak and vector excursions.

## Verification

Eight module tests and two integration tests cover primary vector angle and
Rec709/2020 luma, actual raster placement for all four kinds, known PQ and HLG
values, repeatable PNG bytes, invalid frame shape/overflow/scales, transparent
and nonfinite pixels, cancellation, registry command output, and project
nonmutation. The command integration test verifies legacy and managed PQ output.
Fixture PNGs were independently decoded with Pillow and visually inspected.

Run:

```
cargo test --workspace
KIMO_SCOPE_FIXTURES=/tmp/scopes cargo test -p kimo-core --lib scope_image
```

The opt-in fixtures include SDR bars+ramp, Rec2020 PQ 0..1000-nit ramp, and
Rec2020 HLG encoded ramp. Normal tests do not write fixture files.

No calibrated mastering, HDR certification, professional-editor parity,
legal-range compliance, skin classification, or scene exposure advice is claimed.
