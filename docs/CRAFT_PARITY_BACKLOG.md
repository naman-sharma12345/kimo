# Parity backlog: EffectCraft and FilmCraft

What [EffectCraft](https://github.com/storytold/effectcraft) (After Effects class) and
[FilmCraft](https://github.com/storytold/filmcraft) (Premiere class) do that kimo does not yet do.
Both are Apache-2.0. kimo implements these itself for its own model; where their code is ported, the
file keeps their notice and says what changed.

Sources: their `docs/effects.md` (308 effects), `docs/target-app-parity.md`, `docs/gaps.md` and READMEs,
compared with kimo's `list_effects` (189 kinds) and tool registry (about 575 tools) on 2026-11-23.

Order agreed: effects first, GPU and codecs last.

`[x]` done, `[ ]` to do, `(n)` = kimo batch that delivered it.

## Done before this list

- [x] Lottie export and import (5)
- [x] Cube spin and flip over transitions (5)
- [x] Audio: de-hum, delay, band-pass, chorus, flanger, phaser, de-click, tremolo (5)
- [x] Marker export: CSV, YouTube chapters, JSON (5)

## 1. Effects

kimo already has many After Effects effects under its own names (blur, glow, curves, levels, keyer,
choker, wave, ripple, twirl, bulge, polar, offset, motion_tile, four_color_gradient, fractal_noise,
caustics, cc_glass, cc_kaleida, lens_flare, light_rays, write_on, audio_spectrum, posterize, threshold,
find_edges, emboss, mosaic, scatter, roughen_edges, gradient_ramp, fill, stroke, drop_shadow,
outer_glow, spill_suppress, luma_key, chroma_key, ...). These are the ones still missing.

### Distort
- [x] Wave Warp (6) - [x] Spherize (6) - [x] Magnify (6) - [x] Optics Compensation (6)
- [x] Mesh Warp (6) - [x] Transform (6)
- [ ] Bezier Warp - [ ] Liquify - [ ] Reshape - [ ] Puppet (pins, starch, overlap, recording)
- [ ] Smear - [ ] Directional Smear - [ ] Flexible Bend - [ ] Two-Point Bend
- [ ] Split Warp / Dual Split Warp / Dual Lens Warp - [ ] Scaled Tiles / Tile Grid
- [ ] Rolling Shutter Repair - [ ] Detail-preserving Upscale
- [ ] Warp Stabilizer as an effect (kimo has the stabilize tool)

### Perspective
- [x] Cylinder Wrap (6) - [x] Sphere Wrap (6) - [x] Bevel Alpha (6)
- [ ] Bevel Edges - [ ] Radial Shadow - [ ] Environment Map - [ ] Surface Spotlight - [ ] 3D Glasses
- [ ] 3D Camera Tracker (solve, track points, create camera/solids)

### Keying and matte
- [x] Linear Color Key (6) - [x] Color Range (6) - [x] Extract (6) - [x] Unmult (6) - [x] Key Cleaner (6)
- [ ] Color Difference Key - [ ] Inner/Outer Key - [ ] Difference Matte - [ ] Line Removal
- [ ] Screen Key / Screen Color Key - [ ] Refine Soft Matte / Refine Hard Matte
- [ ] Object Matte (click/box AI matte as an effect; kimo has roto_subject)
- [ ] Tracked Shape Matte - [ ] Set Matte (alpha from another layer)

### Colour
- [x] Color Balance (6) - [x] Color Balance (HLS) (6) - [x] Selective Color (6) - [x] Photo Filter (6)
- [x] Colorama (6) - [x] Change Color (6) - [x] Change to Color (6) - [x] Leave Color (6)
- [x] Equalize (6) - [x] Auto Color (6) - [x] Broadcast Colors / Video Limiter (6)
- [x] Black & White (6) - [x] Tritone (6) - [x] Vibrance (6) - [x] Gamma/Pedestal/Gain (6)
- [ ] Color Stabilizer - [ ] Color Link - [ ] Color Neutralizer - [ ] Multi-Tone Tint
- [ ] PS Arbitrary Map - [ ] OCIO CDL / Display / Look / File transforms as effects

### Channel
- [x] Arithmetic (6) - [x] Minimax (6) - [x] Channel Combiner (6)
- [ ] Calculations - [ ] Compound Arithmetic - [ ] Blend (with another layer) - [ ] Set Channels
- [ ] Source Composite

### Noise, grain and generate
- [x] Noise HLS (6) - [x] Noise Alpha (6) - [x] Circle (6) - [x] Ellipse (6)
- [ ] Turbulent Noise - [ ] Curl Noise - [ ] Paint Beads - [ ] Radial Light Burst - [ ] Woven Threads

### Simulation
- [ ] Particle Emitter 2D / 3D (kimo has emitters and particles3d layers; not as effects)
- [ ] Particle Playground - [ ] Card Dance - [ ] Shatter (effect) - [ ] Pixel Shatter - [ ] Wave World
- [ ] Foam - [ ] Bubbles - [ ] Rainfall - [ ] Snowfall - [ ] Starfield - [ ] Ball Grid - [ ] Surface Fibers
- [ ] Liquid Particles

### Time
- [ ] Timewarp (as an effect) - [ ] Pixel Motion Blur - [ ] Temporal Blend - [ ] Temporal Motion Blur
- [ ] Time Difference - [ ] Time Displacement

### Blur
- [ ] Camera Lens Blur with a depth map layer - [ ] Compound Blur - [ ] Cross Blur
- [ ] Variable Radial Blur - [ ] Vector Blur (effect) - [ ] Unsharp Mask (separate from sharpen)

### 3D channel
- [ ] Cryptomatte - [ ] ID Matte - [ ] Depth Matte - [ ] Depth of Field - [ ] Fog 3D - [ ] EXtractoR / IDentifier

### Stylize, text, utility, VR
- [ ] Hexagonal Tiles - [ ] Palette Map - [ ] Block Reveal - [ ] Edge Tile Repeat - [ ] Plastic Shading
- [ ] Numbers - [ ] Timecode (as effects; kimo has burn_in_timecode)
- [ ] HDR Highlight Compression - [ ] Cineon Converter - [ ] Out-of-Range Display - [ ] Grow Bounds
- [ ] Face Track Points / Face Measurements
- [ ] VR Sphere to Plane - [ ] VR Color Gradients - [ ] VR Digital Glitch

### Presets
- [ ] Animation presets: text animators, transitions, backgrounds, behaviours (EffectCraft 76, After Effects 621)

## 2. Animation and expressions
- [ ] JavaScript expressions (wiggle, loopOut, valueAtTime, thisComp.layer(...), vector maths) and pick-whip links
- [ ] Roving keys, continuous/auto Bezier, separate speed graph
- [ ] Puppet tool with recording, pins driven by nulls
- [ ] Motion paths editable in the viewer
- [ ] Scripting API (ExtendScript-like) for batch jobs

## 3. Editing (FilmCraft)
- [ ] Integer tick time base; exact 23.976 / 29.97 drop-frame / 59.94
- [ ] Three-point editing: source In/Out, insert, overwrite, patch and target tracks
- [ ] Trim modes: ripple, roll, slip, slide, rate stretch as one trim engine; Trim and Slip monitors; JKL dynamic trim
- [ ] Sync locks and linked selection
- [ ] Multicam with audio sync and angle switching
- [ ] Auto-save with rolling versions; crash-recovery journal
- [ ] Nesting and paste-insert parity, effects on a whole track

## 4. Interchange and project files
- [ ] FCP 7 XML (xmeml) import and export
- [ ] FCPXML import (kimo exports only)
- [ ] AAF and OMF (audio post handoff)
- [ ] OTIO import (kimo exports only); ALE
- [ ] EDL: drop-frame, transitions, M2 speed, one EDL per track
- [ ] PSD import as layers; PDF and AI import
- [ ] After Effects `.aep` import (large)
- [ ] Relative paths and relinking when a project moves

## 5. Audio engine
- [ ] Own DSP chain (no ffmpeg filters): parametric EQ, dynamics, multiband, reverbs, pitch, restoration
- [ ] Mixer: track faders and pan, buses, sends, submixes, insert slots, latency compensation
- [ ] Automation modes (Read, Latch, Touch, Write) recorded from the mixer
- [ ] 5.1 surround mixing and export
- [ ] Voice-over recording
- [ ] Remix (retime music to a length at musical boundaries)
- [ ] BS.1770 meters matching ffmpeg ebur128 to 0.1 LU; true peak

## 6. Captions and graphics
- [ ] Caption tracks: CEA-608, CEA-708, Teletext; SCC import and export; 608/708 embedded in exports
- [ ] Rolls and crawls; editable graphic templates

## 7. Export
- [ ] ProRes 422 HQ to 4444 XQ with alpha; DNxHR; MXF OP1a and OP-Atom
- [ ] EXR, DPX and TGA sequences; WebM VP9 with alpha; AV1; HEVC Main 10 with HDR metadata
- [ ] Hardware encoding (NVENC, VideoToolbox, Quick Sync)
- [ ] Render queue with templates; render previews cached by content

## 8. The app
- [ ] Timeline with properties that open per layer; drag to reorder; property shortcuts
- [ ] Graph editor (value and speed graphs)
- [ ] Effect Controls with keyframe lanes
- [ ] On-screen transform handles, mask and path editing in the viewer
- [ ] Audio mixer, colour panel with scopes, Essential Sound
- [ ] Shortcut editor, workspaces and docking
- [ ] Localization

## 9. Last: performance, GPU and codecs
- [ ] GPU compositor (wgpu) with CPU parity tests
- [ ] Own decoders and encoders where ffmpeg falls short
- [ ] Hardware decode
