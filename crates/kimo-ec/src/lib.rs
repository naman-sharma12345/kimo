// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed; lib.rs keeps only the shared effect types, and adds Value and ParamUi in place of the keyframe and project crates.
//! EffectCraft's effects, vendored for kimo (see `kimo_core::ec_bridge` for how kimo calls them).
#![allow(clippy::all, dead_code, unused_imports, unused_variables, unused_mut)]
use std::collections::HashMap;
use std::sync::OnceLock;

pub mod color;
pub mod geom;
pub mod raster;
pub mod track;
pub mod textfx;
pub mod craft_exr;
pub mod segment;

/// Standard base64 with padding (the tracker's RLE export used craft-mime for this).
pub fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity((data.len() + 2) / 3 * 4);
    for c in data.chunks(3) { let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)]; let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 { if i <= c.len() { o.push(T[(n >> (18 - 6 * i) & 63) as usize] as char); } else { o.push('='); } } }
    o
}
pub use raster::{AuxChannels, Image};
pub mod util;
pub mod noise;
pub mod generate;
pub mod keying2;
pub mod channel2;
pub mod stylize2;
pub mod stylize3;
pub mod noise2;
pub mod noise3;
pub mod generate2;
pub mod perspective2;
pub mod distort2;
pub mod distort3;
pub mod distort4;
pub mod blur2;
pub mod color2;
pub mod color3;
pub mod transition2;
pub mod color_fx;
pub mod utility;
pub mod file_cache;
pub mod generate3;
pub mod transition;
pub mod sim;
pub mod sim2;
pub mod card3d;
pub mod psim;
pub mod ocio;
pub mod ocio_config;
pub mod sim3;
pub mod misc;
pub mod distort;
pub mod keying;
pub mod matte;
pub mod channel;
pub mod perspective;
pub mod controls;
pub mod time_fx;
pub mod keylight;
pub mod blur3;
pub mod vr;
pub mod utility2;
pub mod obsolete;
pub mod channel3d;
pub mod mocha_shape;
pub mod analysis_cache;
pub mod crypto_select;
pub mod roto;
pub mod object_matte;
pub mod warp_stab;
pub mod camera_tracker;
pub mod face_track;

/// A parameter value (EffectCraft's keyframe `Value`, without paths, text and gradients).
#[derive(Clone, Debug, PartialEq)]
pub enum Value { Scalar(f64), Vec2([f64; 2]), Vec3([f64; 3]), Color([f64; 4]), Bool(bool), Enum(u32), Layer(Option<u64>), Str(String) }
impl Value {
    pub fn components(&self) -> Vec<f64> { match self { Value::Scalar(v) => vec![*v], Value::Vec2(v) => v.to_vec(), Value::Vec3(v) => v.to_vec(), Value::Color(v) => v.to_vec(), Value::Bool(b) => vec![if *b { 1.0 } else { 0.0 }], Value::Enum(i) => vec![*i as f64], _ => vec![] } }
    pub fn as_f64(&self) -> f64 { self.components().first().copied().unwrap_or(0.0) }
    pub fn as_vec2(&self) -> [f64; 2] { let c = self.components(); [c.first().copied().unwrap_or(0.0), c.get(1).copied().unwrap_or(0.0)] }
    pub fn as_vec3(&self) -> [f64; 3] { let c = self.components(); [c.first().copied().unwrap_or(0.0), c.get(1).copied().unwrap_or(0.0), c.get(2).copied().unwrap_or(0.0)] }
    pub fn as_color(&self) -> [f32; 4] { let c = self.components(); let g = |i: usize, d: f64| c.get(i).copied().unwrap_or(d) as f32; [g(0, 0.0), g(1, 0.0), g(2, 0.0), g(3, 1.0)] }
    pub fn as_bool(&self) -> bool { match self { Value::Bool(b) => *b, v => v.as_f64() != 0.0 } }
    pub fn as_enum(&self) -> u32 { match self { Value::Enum(i) => *i, v => v.as_f64().max(0.0) as u32 } }
    pub fn as_layer(&self) -> Option<u64> { if let Value::Layer(l) = self { *l } else { None } }
    pub fn as_str(&self) -> &str { if let Value::Str(s) = self { s } else { "" } }
}

/// How a parameter is shown (EffectCraft's project `ParamUi`); kimo uses it to name and range parameters.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ParamUi { #[default] Number, Slider { min: f64, max: f64, slider_min: f64, slider_max: f64, decimals: u8 }, Percent, Angle, Pixels, Point, Point3, Color, Checkbox, Popup { options: Vec<String> }, Layer, Mask, Path, Text, File { filters: Vec<String> }, Gradient, Hidden }

pub const CATEGORIES: &[&str] = &[
    "3D Channel",
    "Audio",
    "Blur & Sharpen",
    "Channel",
    "Color Correction",
    "Distort",
    "Expression Controls",
    "Generate",
    "Immersive Video",
    "Keying",
    "Matte",
    "Noise & Grain",
    "Obsolete",
    "Paint",
    "Perspective",
    "Simulation",
    "Stylize",
    "Text",
    "Time",
    "Transition",
    "Utility",
];

pub struct ParamSpec {
    /// Point defaults are fractions of the layer size (0.5, 0.5 = layer centre).
    pub id: &'static str,
    pub name: &'static str,
    pub default: Value,
    pub ui: ParamUi,
}

/// Parameter values evaluated at the current time.
#[derive(Clone, Debug, Default)]
pub struct Params {
    pub values: HashMap<String, Value>,
}

impl Params {
    pub fn get(&self, id: &str) -> Option<&Value> {
        self.values.get(id)
    }
    pub fn f(&self, id: &str) -> f64 {
        self.values.get(id).map(Value::as_f64).unwrap_or(0.0)
    }
    pub fn v2(&self, id: &str) -> [f64; 2] {
        self.values.get(id).map(Value::as_vec2).unwrap_or([0.0; 2])
    }
    pub fn color(&self, id: &str) -> [f32; 4] {
        self.values.get(id).map(Value::as_color).unwrap_or([1.0; 4])
    }
    pub fn b(&self, id: &str) -> bool {
        self.values.get(id).map(Value::as_bool).unwrap_or(false)
    }
    pub fn e(&self, id: &str) -> u32 {
        self.values.get(id).map(Value::as_enum).unwrap_or(0)
    }
    /// String parameter (`Value::Str`), empty when missing or of another kind.
    pub fn s(&self, id: &str) -> &str {
        match self.values.get(id) {
            Some(Value::Str(s)) => s,
            _ => "",
        }
    }
    /// Key prefixes (`{prefix}#1/`, `{prefix}#2/`, …) of the nested groups flattened under
    /// `prefix` by [`flatten_params`], in order.
    pub fn groups(&self, prefix: &str) -> Vec<String> {
        let mut out = vec![];
        for i in 1.. {
            let pre = format!("{prefix}#{i}/");
            if !self.values.contains_key(&format!("{pre}@match")) {
                break;
            }
            out.push(pre);
        }
        out
    }
    /// The first nested group under `prefix` with match id `m`.
    pub fn group(&self, prefix: &str, m: &str) -> Option<String> {
        self.groups(prefix).into_iter().find(|g| self.s(&format!("{g}@match")) == m)
    }
}

/// Evaluate an effect instance's parameters, including nested groups (Paint strokes, Puppet
/// meshes and pins). Direct properties keep their match id as the key. The `i`-th child group
/// (1-based, counting groups only) of a group with key prefix `pre` gets the prefix
/// `{pre}#{i}/`, its properties `{pre}#{i}/{match}`, and metadata keys `@match`, `@name`
/// (`Value::Str`), `@enabled` (`Value::Bool`) and `@uid` (`Value::Scalar`). Nested properties
/// are also keyed by their match path (`group/sub/param`, the first group with each match id),

/// A layer image in flight: point `p` sits at pixel `p * scale + offset`. Points are in layer
/// space, or in effect space while the layer's effects run (see [`Buf::rebase`]).
#[derive(Clone, Debug, Default)]
pub struct Buf {
    pub img: Image,
    pub offset: [f64; 2],
    pub scale: f64,
}

impl Buf {
    pub fn to_px(&self, p: [f64; 2]) -> (f64, f64) {
        (p[0] * self.scale + self.offset[0], p[1] * self.scale + self.offset[1])
    }
    /// Grow the buffer so it covers a `size`-layer-pixel region starting at its offset (see
    /// [`cover_plan`]): the whole layer rectangle a sized source's buffer already is. Content
    /// keeps its place; the new area is transparent. No-op when the buffer already covers it.
    pub fn cover(&mut self, size: [f64; 2]) {
        let Some((dx, dy, w, h)) = cover_plan(self.offset, self.scale, size, self.img.width, self.img.height) else { return };
        let src = std::mem::replace(&mut self.img, Image::new(w, h));
        let (sw, sh) = (src.width as usize, src.height as usize);
        for y in 0..sh {
            let s = y * sw;
            let d = (y + dy as usize) * w as usize + dx as usize;
            self.img.data[d..d + sw].copy_from_slice(&src.data[s..s + sw]);
        }
        self.offset[0] += dx as f64;
        self.offset[1] += dy as f64;
    }
    /// Measure from `origin` (in the current coordinates): a point `p` becomes `p − origin`.
    /// Moves between layer space and effect space (the renderer's `effect_bounds`).
    pub fn rebase(&mut self, origin: [f64; 2]) {
        self.offset[0] += origin[0] * self.scale;
        self.offset[1] += origin[1] * self.scale;
    }
    /// Grow by transparent pixels: `sides` = left, top, right, bottom. Content keeps its place.
    pub fn pad_sides(&mut self, sides: [u32; 4]) {
        let [l, t, r, b] = sides;
        if l == 0 && t == 0 && r == 0 && b == 0 {
            return;
        }
        let (Some(w), Some(h)) = (self.img.width.checked_add(l).and_then(|v| v.checked_add(r)), self.img.height.checked_add(t).and_then(|v| v.checked_add(b)))
        else {
            return;
        };
        let mut out = Image::new(w, h);
        let (sw, ow) = (self.img.width as usize, w as usize);
        for y in 0..self.img.height as usize {
            let at = (y + t as usize) * ow + l as usize;
            let (Some(src), Some(dst)) = (self.img.data.get(y * sw..(y + 1) * sw), out.data.get_mut(at..at + sw)) else { continue };
            dst.copy_from_slice(src);
        }
        self.img = out;
        self.offset[0] += l as f64;
        self.offset[1] += t as f64;
    }
    /// Grow by `left` and `top` transparent pixels on those sides only.
    pub fn grow_left_top(&mut self, left: u32, top: u32) {
        self.pad_sides([left, top, 0, 0]);
    }
    /// Give the buffer the pixel-grid phase (modulo [`GRID_ALIGN`]) that [`Buf::cover`] of
    /// `size` would, by growing left/top only ([`align_plan`]). A downscaled layer is resampled
    /// through a pyramid whose 2x2 blocks follow the buffer's grid, so a buffer that starts at
    /// its content's corner instead of the cover's resamples to different pixels (#1205).
    pub fn align(&mut self, size: [f64; 2]) {
        if let Some((l, t)) = align_plan(self.offset, self.scale, size, self.img.width, self.img.height) {
            self.grow_left_top(l, t);
        }
    }
    /// Grow by `pad` transparent pixels on every side, within [`pad_within_budget`] and as
    /// clamped by [`Image::effective_pad`].
    pub fn pad(&mut self, pad: u32) {
        let eff = self.img.effective_pad(pad_within_budget(self.img.width, self.img.height, pad));
        if eff == 0 {
            return;
        }
        self.img = self.img.padded(eff);
        self.offset[0] += eff as f64;
        self.offset[1] += eff as f64;
    }
}

/// The most pixels a `w` × `h` effect buffer may grow to: 16 Mpx (256 MiB of f32 RGBA) or four
/// times its own area, whichever is larger. Shared by every effect that grows its buffer.
fn pixel_budget(w: f64, h: f64) -> f64 {
    const MIN_BUDGET: f64 = 16_777_216.0;
    MIN_BUDGET.max(4.0 * w * h)
}

/// `pad` on every side of a `w` × `h` effect buffer, cut so the padded buffer stays within
/// [`pixel_budget`]: a small layer far from an effect's centre can't ask for gigabytes (#2261).
/// Shared with the GPU path.
pub fn pad_within_budget(w: u32, h: u32, pad: u32) -> u32 {
    let (w, h) = (f64::from(w), f64::from(h));
    let budget = pixel_budget(w, h);
    if (w + 2.0 * f64::from(pad)) * (h + 2.0 * f64::from(pad)) <= budget {
        return pad;
    }
    // (w + 2p)(h + 2p) = budget, solved for p.
    let p = (((w - h).powi(2) + 4.0 * budget).sqrt() - (w + h)) / 4.0;
    (p.floor().max(0.0) as u32).min(pad)
}

/// The most pixels on a side that [`grow_sides`] grows a buffer to.
const MAX_GROWN_SIDE: f64 = 16384.0;

/// The transparent pixels to add (left, top, right, bottom; for [`Buf::pad_sides`]) so a
/// `width` × `height` buffer also spans the pixel box `lo`..`hi`, in the buffer's own coordinates.
/// When the full growth would pass [`MAX_GROWN_SIDE`] or the [`pixel_budget`], every side is cut
/// by the same factor instead of giving up, so the picture shrinks smoothly rather than popping.
/// `None` when the buffer already spans the box, or the box is not finite.
pub fn grow_sides(lo: [f64; 2], hi: [f64; 2], width: u32, height: u32) -> Option<[u32; 4]> {
    if !lo.iter().chain(hi.iter()).all(|v| v.is_finite()) {
        return None;
    }
    let (w, h) = (f64::from(width), f64::from(height));
    let want = |v: f64| v.clamp(0.0, MAX_GROWN_SIDE);
    let [l, t] = [want(-lo[0].floor()), want(-lo[1].floor())];
    let [r, b] = [want(hi[0].ceil() - w), want(hi[1].ceil() - h)];
    let (sx, sy) = (l + r, t + b);
    if sx == 0.0 && sy == 0.0 {
        return None;
    }
    let mut f = 1.0_f64;
    if sx > 0.0 {
        f = f.min((MAX_GROWN_SIDE - w).max(0.0) / sx);
    }
    if sy > 0.0 {
        f = f.min((MAX_GROWN_SIDE - h).max(0.0) / sy);
    }
    let budget = pixel_budget(w, h);
    if (w + sx * f) * (h + sy * f) > budget {
        // (w + sx·f)(h + sy·f) = budget, solved for f.
        let (a, bq, c) = (sx * sy, w * sy + h * sx, w * h - budget);
        let q = if a > 0.0 { (-bq + (bq * bq - 4.0 * a * c).max(0.0).sqrt()) / (2.0 * a) } else { -c / bq };
        f = f.min(q);
    }
    let f = if f.is_finite() { f.clamp(0.0, 1.0) } else { 0.0 };
    let sides = [l, t, r, b].map(|v| (v * f).floor() as u32);
    sides.iter().any(|&v| v != 0).then_some(sides)
}

/// The pixel box (min, max) enclosing the four points of a quad.
pub fn quad_bounds(q: &[[f64; 2]; 4]) -> ([f64; 2], [f64; 2]) {
    let f = |i: usize, init: f64, g: fn(f64, f64) -> f64| q.iter().fold(init, |a, p| g(a, p[i]));
    ([f(0, f64::INFINITY, f64::min), f(1, f64::INFINITY, f64::min)], [f(0, f64::NEG_INFINITY, f64::max), f(1, f64::NEG_INFINITY, f64::max)])
}

/// Motion Tile's output frame for a buffer with layer origin `offset` (pixels) at `scale`:
/// `(width, height, offset)`. Output Width / Height grow (or crop) the layer rectangle about its
/// centre, on adjustment layers too, where the rectangle is the comp's and starts at the comp
/// origin in the buffer below (After Effects; a Transform after it brings the tiles into view,
/// #1727). An adjustment at 100 % keeps the buffer it was given; one whose frame would pass
/// 16 384 pixels a side does too.
pub fn motion_tile_frame(ctx: &EffectCtx, offset: [f64; 2], size: (u32, u32), scale: f64) -> (u32, u32, [f64; 2]) {
    let (lw, lh) = (ctx.layer_size[0], ctx.layer_size[1]);
    let (pw, ph) = (ctx.params.f("outputWidth"), ctx.params.f("outputHeight"));
    let ow = (lw * pw / 100.0).max(1.0 / scale);
    let oh = (lh * ph / 100.0).max(1.0 / scale);
    let (nw, nh) = ((ow * scale).round().max(1.0), (oh * scale).round().max(1.0));
    let grow = [(ow - lw) * 0.5 * scale, (oh - lh) * 0.5 * scale];
    if !ctx.adjustment {
        return (nw as u32, nh as u32, grow);
    }
    // The grown rectangle's top-left in the buffer below, on whole pixels so the result lines
    // up with the comp again when the stack ends.
    let (x0, y0) = ((offset[0] - grow[0]).round(), (offset[1] - grow[1]).round());
    let fits = [x0, y0, nw, nh].iter().all(|v| v.is_finite()) && nw <= 16384.0 && nh <= 16384.0;
    if (pw == 100.0 && ph == 100.0) || !fits {
        return (size.0, size.1, offset);
    }
    (nw as u32, nh as u32, [offset[0] - x0, offset[1] - y0])
}

/// Where an adjustment stack's result (comp origin at `offset`, `size` pixels) lies on the canvas
/// it came from (comp origin at `canvas_offset`): the result pixel at canvas pixel (0, 0).
/// `None` when it already is the canvas, frame for frame.
pub fn adjustment_crop(offset: [f64; 2], canvas_offset: [f64; 2], size: (u32, u32), canvas: (u32, u32)) -> Option<(i64, i64)> {
    let (dx, dy) = ((offset[0] - canvas_offset[0]).round(), (offset[1] - canvas_offset[1]).round());
    if !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0 && size == canvas) {
        return None;
    }
    Some((dx as i64, dy as i64))
}

/// Effects that grow their own buffer by what they need (blur and glow margins, Repeat Edge
/// Pixels through [`Buf::cover`], displacement through [`cover_pad`]): a stack of only these
/// skips the renderer's cover of the whole layer rectangle (#1205).
/// Drop Shadow also pads by its distance and softness on both CPU and GPU: covering the
/// layer rectangle first only inflates working images on highly scaled text and shapes (#3357).
pub const SKIPS_COVER: &[&str] = &["ec.blur.gaussian", "ec.blur.fastbox", "ec.stylize.glow", "ec.distort.turbulentdisplace", "ec.perspective.dropshadow"];

/// The grid phase [`Buf::align`] keeps and the step [`cover_pad`] grows in: covers the levels
/// of the resampling pyramid used for downscaled layers.
pub const GRID_ALIGN: u32 = 64;

/// Left/top growth giving a buffer [`cover_plan`]'s grid phase modulo [`GRID_ALIGN`]; `None`
/// when there is nothing to add.
pub fn align_plan(offset: [f64; 2], scale: f64, size: [f64; 2], width: u32, height: u32) -> Option<(u32, u32)> {
    let (dx, dy, _, _) = cover_plan(offset, scale, size, width, height)?;
    let (l, t) = (dx % GRID_ALIGN, dy % GRID_ALIGN);
    (l != 0 || t != 0).then_some((l, t))
}

/// Padding for an effect that moves pixels up to `want` past the content (not an adjustment
/// layer): `want`, capped at what reaches the layer rectangle, in whole [`GRID_ALIGN`] steps so
/// the grid phase stays. 0 once the buffer covers the layer.
pub fn cover_pad(ctx: &EffectCtx, size: [u32; 2], offset: [f64; 2], scale: f64, want: u32) -> u32 {
    if ctx.adjustment {
        return 0;
    }
    want.min(util::layer_cover_pad(ctx, size, offset, scale)).div_ceil(GRID_ALIGN) * GRID_ALIGN
}

/// Where [`Buf::cover`] would take a buffer: shift its pixels by `(dx, dy)` and resize it to
/// `width` × `height` so it spans the `size`-layer-pixel region that starts at its offset — the
/// whole layer rectangle, which a sized source's buffer already is. `None` when the buffer
/// already covers that region unchanged (footage, solids, the comp below an adjustment).
pub fn cover_plan(offset: [f64; 2], scale: f64, size: [f64; 2], width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    if scale <= 0.0 || !scale.is_finite() || !offset[0].is_finite() || !offset[1].is_finite() || !size[0].is_finite() || !size[1].is_finite() {
        return None;
    }
    // Region and content, both as pixel ranges in the buffer's own coordinates.
    let (rw, rh) = (offset[0] + (size[0] * scale).ceil(), offset[1] + (size[1] * scale).ceil());
    let (lo_x, lo_y) = (offset[0].min(0.0).floor(), offset[1].min(0.0).floor());
    let (hi_x, hi_y) = (rw.max(width as f64).ceil(), rh.max(height as f64).ceil());
    let (nw_f, nh_f) = (hi_x - lo_x, hi_y - lo_y);
    if !(nw_f <= 16384.0 && nh_f <= 16384.0) {
        return None;
    }
    let (dx, dy) = ((-lo_x) as u32, (-lo_y) as u32);
    let (nw, nh) = (nw_f.round().max(1.0) as u32, nh_f.round().max(1.0) as u32);
    (dx != 0 || dy != 0 || nw != width || nh != height).then_some((dx, dy, nw, nh))
}

/// A layer mask at the current time, flattened to a polyline in layer coordinates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaskShape {
    pub name: String,
    pub points: Vec<[f64; 2]>,
    pub closed: bool,
    pub inverted: bool,
}

/// Pixels of another layer referenced by a layer parameter: its buffer (layer space, see
/// [`Buf`]) and its source size in layer pixels.
#[derive(Clone, Debug, Default)]
pub struct LayerPixels {
    pub buf: Buf,
    pub size: [f64; 2],
}

/// Services the renderer offers effects that look beyond their own pixels.
pub trait EffectHost: Sync {
    /// Pixels entering the effect stack, in effect space, before any effects. For an
    /// adjustment layer this is the composition below it, in the effect working space.
    fn original(&self) -> Option<&Buf> {
        None
    }

    /// Layer `id` of the same composition at the current comp time. `masks_and_effects`
    /// selects "Effects & Masks" over "Source". `None` for missing/recursive references; own Source reads are supported.
    fn layer(&self, id: u64, masks_and_effects: bool) -> Option<LayerPixels>;
    /// `frames` stereo sample frames (interleaved L R …) of layer `id`'s audio starting at comp
    /// time `start` seconds, at `rate` Hz. `None` when the layer has no audio.
    fn audio(&self, id: u64, start: f64, frames: usize, rate: u32) -> Option<Vec<f32>>;
    /// Layer `id` with its masks but none of its effects (a layer parameter's "Masks" choice).
    /// Defaults to the plain source.
    fn layer_masks(&self, id: u64) -> Option<LayerPixels> {
        self.layer(id, false)
    }
    /// The effect's own layer at another **layer time** `layer_time` (seconds): its source with
    /// masks applied, followed by the first `effects` effects of its stack (0 = none; the host
    /// never goes past the effect being rendered). `None` when unavailable (no host, recursion
    /// limit, adjustment layers). Time effects use this to read neighbouring frames.
    fn self_at(&self, _layer_time: f64, _effects: usize) -> Option<Buf> {
        None
    }
    /// Layer `id` of the same composition at comp time `comp_time` (seconds), like
    /// [`EffectHost::layer`] but at another time.
    fn layer_at(&self, _id: u64, _comp_time: f64, _masks_and_effects: bool) -> Option<LayerPixels> {
        None
    }
    /// Auxiliary 3D channels (depth, object / material IDs, normals, Cryptomatte, any named
    /// EXR channel) of the effect's own layer source at the current time, in layer space (see
    /// [`AuxChannels`]). `None` when the source has none (the 3D Channel effects then pass the
    /// layer through).
    fn aux(&self) -> Option<std::sync::Arc<AuxChannels>> {
        None
    }
    /// The composition's camera and first light relative to the effect's layer (Card
    /// Dance / Shatter / Card Wipe's Comp Camera and First Comp Light). `None` when unknown.
    fn comp_scene(&self) -> Option<CompScene> {
        None
    }
    /// The running effect's own parameters evaluated at another **layer time** (Radio Waves'
    /// Parameters Are Set At: Birth). `None` when unavailable.
    fn params_at(&self, _layer_time: f64) -> Option<Params> {
        None
    }
    /// Parameter `id` (a [`flatten_params`] id such as `birthRate`) of effect `effect_index` in
    /// the layer's stack (the running effect's [`EffectEnv::effect_index`]) at another **layer
    /// time**: Particle Emitter 3D samples its Birth Rate at every step it simulates (#1373).
    /// `None` when unavailable.
    fn param_at(&self, _effect_index: usize, _id: &str, _layer_time: f64) -> Option<Value> {
        None
    }
    /// Parameter `id` of effect `effect_index` has one value at every time (no keyframes, no
    /// expression), so sampling it over time can be skipped. `false` when unknown.
    fn param_is_static(&self, _effect_index: usize, _id: &str) -> bool {
        false
    }
    /// A particle simulation backend (GPU particles; see [`psim`]). `None` = simulate on the
    /// CPU.
    fn particles(&self) -> Option<&dyn psim::ParticleSim> {
        None
    }
}

/// Extra context the renderer may supply (all optional; `Default` is "nothing known").
#[derive(Clone, Copy, Default)]
pub struct EffectEnv<'a> {
    /// The layer's masks (enabled ones, top to bottom).
    pub masks: &'a [MaskShape],
    pub host: Option<&'a dyn EffectHost>,
    /// Composition time in seconds.
    pub comp_time: f64,
    /// Composition frame rate (0 when unknown).
    pub frame_rate: f64,
    /// Index of the running effect in its layer's stack (for [`EffectHost::self_at`]).
    pub effect_index: usize,
    /// The project's working colour space (`None` = unmanaged, treated as sRGB).
    pub working_space: Option<crate::color::ColorSpace>,
    /// Working-space pixels are linear light (Linearize Working Space).
    pub working_linear: bool,
    /// The project works in 32 bpc float (Levels' "Off for 32 bpc Color" clips only below it).
    pub float_depth: bool,
    /// The composition's shutter (angle and phase in degrees, samples per frame) when both the
    /// comp's and the layer's motion blur switches are on (Transform's Use Composition's
    /// Shutter Angle).
    pub shutter: Option<(f64, f64, u32)>,
    /// The layer's source time range in **layer time** seconds as (first, end): the layer time of
    /// the first source frame and the time just past the last one (so the source lasts
    /// `end - first` seconds and its last frame starts one frame before `end`). `None` when the
    /// source has no duration (stills, solids, text, shapes) or the renderer does not know it.
    /// Retimers clamp their source requests to it.
    pub layer_span: Option<(f64, f64)>,
}

/// What an effect gets to render with.
pub struct EffectCtx<'a> {
    pub params: &'a Params,
    /// Layer time in seconds.
    pub time: f64,
    /// Layer source size in layer pixels (the effect's "layer bounds").
    pub layer_size: [f64; 2],
    /// Instance seed (stable per effect instance).
    pub seed: u32,
    /// Rendering for an adjustment layer (operating on the comp below).
    pub adjustment: bool,
    /// Masks, other layers, audio, comp timing.
    pub env: EffectEnv<'a>,
}

impl EffectCtx<'_> {
    /// The layer chosen in layer parameter `id`, via the host.
    /// `masks_and_effects` is the effect's built-in choice, used when the instance has no
    /// source companion parameter (see [`layer_source_id`]).
    pub fn layer_param(&self, id: &str, masks_and_effects: bool) -> Option<LayerPixels> {
        let lid = self.params.get(id)?.as_layer()?;
        let host = self.env.host?;
        match self.params.get(&layer_source_id(id)).map(Value::as_enum) {
            Some(0) => host.layer(lid, false),
            Some(1) => host.layer_masks(lid),
            Some(_) => host.layer(lid, true),
            None => host.layer(lid, masks_and_effects),
        }
    }
    /// Frame rate for frame-based maths (comp rate, else 30).
    pub fn fps(&self) -> f64 {
        if self.env.frame_rate > 0.0 { self.env.frame_rate } else { 30.0 }
    }
}

pub type RenderFn = fn(&EffectCtx, Buf) -> Buf;

pub struct EffectSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub params: Vec<ParamSpec>,
    pub render: RenderFn,
    /// Implemented on the GPU path too (shown with a badge).
    pub gpu: bool,
    /// Supports 32 bpc float (badge).
    pub float: bool,
}

impl EffectSpec {
    /// True when this effect grows its own buffer (blur/glow margins, Repeat Edge
    /// [`Buf::cover`], displacement [`cover_pad`]) so a stack of only these can skip the
    /// renderer's full-layer cover and keep the pyramid grid phase (#1205).
    pub fn skips_cover(&self) -> bool {
        SKIPS_COVER.contains(&self.id)
    }
}

pub(crate) fn p(id: &'static str, name: &'static str, default: Value, ui: ParamUi) -> ParamSpec {
    ParamSpec { id, name, default, ui }
}
pub(crate) fn slider(min: f64, max: f64, smin: f64, smax: f64, decimals: u8) -> ParamUi {
    ParamUi::Slider { min, max, slider_min: smin, slider_max: smax, decimals }
}
/// A file parameter offering files with these extensions (Effect Controls' Choose…).
pub(crate) fn file_ui(exts: &[&str]) -> ParamUi {
    ParamUi::File { filters: exts.iter().map(|s| s.to_string()).collect() }
}
pub(crate) fn popup(opts: &[&str]) -> ParamUi {
    ParamUi::Popup { options: opts.iter().map(|s| s.to_string()).collect() }
}
pub(crate) fn num(v: f64) -> Value {
    Value::Scalar(v)
}
pub(crate) fn col(r: f64, g: f64, b: f64) -> Value {
    Value::Color([r, g, b, 1.0])
}


/// A parameter's default value on a layer of `layer_size` (point defaults are fractions of the
/// layer size).
pub fn default_value(ps: &ParamSpec, layer_size: [f64; 2]) -> Value {
    match (&ps.ui, &ps.default) {
        (ParamUi::Point, Value::Vec2(f)) => Value::Vec2([f[0] * layer_size[0], f[1] * layer_size[1]]),
        _ => ps.default.clone(),
    }
}

/// Options of a layer parameter's source popup (After Effects shows it next to the layer
/// popup): what of the chosen layer the effect sees.
pub const LAYER_SOURCE_OPTIONS: [&str; 3] = ["Source", "Masks", "Effects & Masks"];

/// Id of the hidden companion parameter holding layer parameter `id`'s source choice (an index
/// into [`LAYER_SOURCE_OPTIONS`]).
pub fn layer_source_id(id: &str) -> String {
    format!("{id}Source")
}



/// Every vendored effect.
pub fn registry() -> &'static [EffectSpec] {
    static R: OnceLock<Vec<EffectSpec>> = OnceLock::new();
    R.get_or_init(|| {
        let mut v = Vec::new();
        v.extend(noise::specs());
        v.extend(generate::specs());
        v.extend(keying2::specs());
        v.extend(channel2::specs());
        v.extend(stylize2::specs());
        v.extend(stylize3::specs());
        v.extend(noise2::specs());
        v.extend(noise3::specs());
        v.extend(generate2::specs());
        v.extend(perspective2::specs());
        v.extend(distort2::specs());
        v.extend(distort3::specs());
        v.extend(distort4::specs());
        v.extend(blur2::specs());
        v.extend(color2::specs());
        v.extend(color3::specs());
        v.extend(transition2::specs());
        v.extend(color_fx::specs());
        v.extend(utility::specs());
        v.extend(generate3::specs());
        v.extend(transition::specs());
        v.extend(sim::specs());
        v.extend(sim2::specs());
        v.extend(ocio::specs());
        v.extend(sim3::specs());
        v.extend(misc::specs());
        v.extend(distort::specs());
        v.extend(keying::specs());
        v.extend(matte::specs());
        v.extend(channel::specs());
        v.extend(perspective::specs());
        v.extend(controls::specs());
        v.extend(time_fx::specs());
        v.extend(keylight::specs());
        v.extend(blur3::specs());
        v.extend(vr::specs());
        v.extend(utility2::specs());
        v.extend(obsolete::specs());
        v.extend(channel3d::specs());
        v.extend(mocha_shape::specs());
        v.extend(roto::specs());
        v.extend(object_matte::specs());
        v.extend(warp_stab::specs());
        v.extend(camera_tracker::specs());
        v.extend(face_track::specs());
        v.sort_by(|a, b| a.category.cmp(b.category).then(a.name.cmp(b.name)));
        v
    })
}
/// From EffectCraft's project crate (footage.rs).
pub fn cineon_log_to_lin(v: f32, black: f32, white: f32, scale: f32, gamma: f32, rolloff: f32) -> f32 {
    // Kodak printing density: 0.002 density per code value, negative gamma 0.6.
    let exposure = |code: f32| 10f32.powf((code - white) * 0.002 / 0.6);
    let offset = exposure(black);
    let mut lin = ((exposure(v * 1023.0) - offset) / (1.0 - offset).max(1e-6)).max(0.0);
    lin = lin.powf(scale / gamma.max(0.01));
    let knee = 1.0 - (rolloff / 100.0).clamp(0.0, 0.99);
    if knee < 1.0 && lin > knee {
        let w = 1.0 - knee;
        lin = knee + w * (1.0 - (-(lin - knee) / w).exp());
    }
    lin
}

pub use card3d::CompScene;
pub use color2::Curve;
pub fn find(id: &str) -> Option<&'static EffectSpec> { registry().iter().find(|s| s.id == id) }
pub fn apply(spec: &EffectSpec, ctx: &EffectCtx, buf: Buf) -> Buf { (spec.render)(ctx, buf) }
