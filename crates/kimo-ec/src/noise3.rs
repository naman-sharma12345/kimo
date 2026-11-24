// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
//! Fractal Noise and Turbulent Noise: one fractal value-noise engine with the parameter tree of
//! the two effects (Fractal Type, Noise Type, Overflow, the Transform / Sub Settings / Evolution
//! Options twirl-downs, Opacity and Blending Mode).
//!
//! Each "noise layer" (octave) is 3D value noise (x, y, evolution) on an integer lattice. The
//! Noise Type picks the interpolation between lattice values (Block = nearest, Linear,
//! Soft Linear = quadratic B-spline, Spline = Catmull-Rom). Successive layers are scaled by Sub
//! Scaling, weighted by Sub Influence, rotated by Sub Rotation and shifted by Sub Offset, and the
//! Fractal Type shapes each layer (turbulent types fold the signed noise; dynamic types warp the
//! domain; Max keeps the strongest layer; Strings keeps thin lines). Contrast / Brightness /
//! Overflow then map the sum to the output range. Cycle Evolution loops the evolution by
//! cross-fading two evolution offsets one cycle apart; Turbulent Noise's Turbulence Factor makes
//! finer layers evolve and drift faster than coarse ones.

use crate::color::{BlendMode, blend_pixel};
use crate::Value;
use crate::ParamUi;
use rayon::prelude::*;

use crate::generate::lattice;
use crate::{Buf, EffectCtx, EffectSpec, ParamSpec, num, p, popup, slider};

/// Fractal Type options (shared by both effects).
pub const FRACTAL_TYPES: [&str; 9] =
    ["Basic", "Turbulent Smooth", "Turbulent Basic", "Turbulent Sharp", "Dynamic", "Dynamic Twist", "Dynamic Progressive", "Max", "Strings"];
/// Noise Type options: interpolation between lattice values.
pub const NOISE_TYPES: [&str; 4] = ["Block", "Linear", "Soft Linear", "Spline"];
/// Overflow options.
pub const OVERFLOWS: [&str; 4] = ["Clip", "Soft Clamp", "Wrap Back", "Allow HDR Results"];
/// Blending Mode options: None renders the noise alone, the rest composite it over the layer.
pub const BLEND_MODES: [&str; 24] = [
    "None",
    "Normal",
    "Add",
    "Multiply",
    "Screen",
    "Overlay",
    "Soft Light",
    "Hard Light",
    "Color Dodge",
    "Color Burn",
    "Darken",
    "Lighten",
    "Difference",
    "Exclusion",
    "Hue",
    "Saturation",
    "Color",
    "Luminosity",
    "Stencil Alpha",
    "Stencil Luma",
    "Silhouette Alpha",
    "Silhouette Luma",
    "Alpha Add",
    "Luminescent Premul",
];
/// Index of "None" / "Normal" in [`BLEND_MODES`].
pub const BLEND_NONE: u32 = 0;
pub const BLEND_NORMAL: u32 = 1;
/// Index of "Soft Linear" in [`NOISE_TYPES`] (the default, and the GPU path's interpolation).
pub const NOISE_SOFT_LINEAR: u32 = 2;

/// Lattice cell of Fractal / Turbulent Noise in pixels per percent of Scale: After Effects 2026
/// draws Block noise at Scale 100 in 66.67 px cells (and Soft Linear noise has the matching
/// correlation length), not 100 px.
pub const NOISE_CELL: f64 = 2.0 / 3.0;

/// Sub Scaling (percent, After Effects' valid 10..10000) as the size ratio of each octave to the
/// one before it. Above 1 each octave is larger than the last.
pub fn sub_scaling_ratio(percent: f64) -> f64 {
    (percent / 100.0).clamp(0.1, 100.0)
}

/// Bound on an octave's frequency and on its Perspective Offset depth (and their inverses).
/// Unbounded, at Sub Scaling 10000 and Complexity 20 they would reach 100^±19, and Perspective
/// Offset multiplying the offset by the depth (and by Turbulence Factor's drift) overflowed to
/// infinity (NaN noise, #3486). Sub Scaling 10..100 never reaches the bound (10^19 at most). Both
/// stay each other's inverse inside it, so an octave's offset in lattice cells is unchanged; at a
/// frequency of 1e-20 an octave spans under 1e-12 cells of any layer, so it has no detail either way.
pub const OCTAVE_SCALE_MAX: f32 = 1e20;

/// Bound on an octave's scaled Offset Turbulence in pixels, so a huge offset times the depth stays
/// finite. Times a frequency of at least 1 / [`OCTAVE_SCALE_MAX`], over cells of at most about
/// 6700 px (Scale 10000 %), it is still over 10^12 lattice cells, past where the lattice index
/// saturates (2^31), so the output is the same.
const OCTAVE_OFFSET_MAX: f32 = 1e36;

/// The next octave's (frequency, depth) from this one's, for a Sub Scaling ratio `s`.
#[inline]
fn next_octave(f: f32, depth: f32, s: f32) -> (f32, f32) {
    let lo = 1.0 / OCTAVE_SCALE_MAX;
    ((f / s).clamp(lo, OCTAVE_SCALE_MAX), (depth * s).clamp(lo, OCTAVE_SCALE_MAX))
}

fn spec(id: &'static str, name: &'static str, params: Vec<ParamSpec>, render: crate::RenderFn) -> EffectSpec {
    EffectSpec { id, name, category: "Noise & Grain", params, render, gpu: false, float: true }
}

fn params(turbulent: bool) -> Vec<ParamSpec> {
    let pct = || slider(0.0, 100.0, 0.0, 100.0, 1);
    let scale = || slider(1.0, 10000.0, 20.0, 600.0, 1);
    let mut v = vec![
        p("fractalType", "Fractal Type", Value::Enum(if turbulent { 1 } else { 0 }), popup(&FRACTAL_TYPES)),
        p("noiseType", "Noise Type", Value::Enum(NOISE_SOFT_LINEAR), popup(&NOISE_TYPES)),
        p("invert", "Invert", Value::Bool(false), ParamUi::Checkbox),
        p("contrast", "Contrast", num(100.0), slider(0.0, 10000.0, 0.0, 400.0, 1)),
        p("brightness", "Brightness", num(0.0), slider(-10000.0, 10000.0, -200.0, 200.0, 1)),
        p("overflow", "Overflow", Value::Enum(if turbulent { 0 } else { 3 }), popup(&OVERFLOWS)),
        p("transform/rotation", "Rotation", num(0.0), ParamUi::Angle),
        p("transform/uniformScaling", "Uniform Scaling", Value::Bool(true), ParamUi::Checkbox),
        p("transform/scale", "Scale", num(100.0), scale()),
        p("transform/scaleWidth", "Scale Width", num(100.0), scale()),
        p("transform/scaleHeight", "Scale Height", num(100.0), scale()),
        p("transform/offset", "Offset Turbulence", Value::Vec2([0.5, 0.5]), ParamUi::Point),
        p("transform/perspectiveOffset", "Perspective Offset", Value::Bool(false), ParamUi::Checkbox),
        p("complexity", "Complexity", num(6.0), slider(1.0, 20.0, 1.0, 10.0, 1)),
        p("subSettings/subInfluence", "Sub Influence (%)", num(70.0), slider(0.0, 100.0, 25.0, 100.0, 1)),
        p("subSettings/subScaling", "Sub Scaling", num(56.0), slider(10.0, 10000.0, 25.0, 100.0, 1)),
    ];
    if !turbulent {
        v.extend([
            p("subSettings/subRotation", "Sub Rotation", num(0.0), ParamUi::Angle),
            p("subSettings/subOffset", "Sub Offset", Value::Vec2([0.0, 0.0]), ParamUi::Point),
            p("subSettings/centerSubscale", "Center Subscale", Value::Bool(false), ParamUi::Checkbox),
        ]);
    }
    v.push(p("evolution", "Evolution", num(0.0), ParamUi::Angle));
    if turbulent {
        v.extend([
            p("evolutionOptions/turbulenceFactor", "Turbulence Factor", num(0.5), slider(0.0, 10.0, 0.0, 2.0, 2)),
            p("evolutionOptions/randomSeed", "Random Seed", num(0.0), slider(0.0, 100000.0, 0.0, 1000.0, 0)),
        ]);
    } else {
        v.extend([
            p("evolutionOptions/cycleEvolution", "Cycle Evolution", Value::Bool(false), ParamUi::Checkbox),
            p("evolutionOptions/cycle", "Cycle (in Revolutions)", num(1.0), slider(1.0, 1000.0, 1.0, 30.0, 0)),
            p("evolutionOptions/seed", "Random Seed", num(0.0), slider(0.0, 100000.0, 0.0, 1000.0, 0)),
        ]);
    }
    v.extend([
        p("opacity", "Opacity", num(100.0), pct()),
        p("blendingMode", "Blending Mode", Value::Enum(BLEND_NORMAL), popup(&BLEND_MODES)),
        // Not in After Effects: a mix with the untouched layer (kept for older projects).
        p("blend", "Blend With Original", num(0.0), pct()),
    ]);
    v
}

pub fn specs() -> Vec<EffectSpec> {
    vec![spec("ec.noise.fractal", "Fractal Noise", params(false), fractal_noise), spec("ec.noise.turbulent", "Turbulent Noise", params(true), turbulent_noise)]
}

// ---------------------------------------------------------------- noise kernels

/// Soft Linear's gain about mid-grey. Averaging 3 × 3 lattice values leaves the B-spline 0.55² of
/// their variance, against 0.784² (181/231 per axis) for the quintic fade Soft Linear used before
/// (#259); the gain keeps the noise's contrast.
const SOFT_LINEAR_GAIN: f32 = (181.0 / 231.0) / 0.55;

#[inline]
fn catmull(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t)
}

/// Uniform quadratic B-spline weights of the lattice values before, at and after the nearest one,
/// for `t` = position − nearest + 0.5 in 0..1.
#[inline]
fn bspline(t: f32) -> [f32; 3] {
    let s = 1.0 - t;
    [0.5 * s * s, 0.5 + t * s, 0.5 * t * t]
}

/// 3D value noise in about 0..1 with the given [`NOISE_TYPES`] interpolation in x and y (the
/// evolution axis z always interpolates linearly, so evolution moves at a constant rate).
///
/// Soft Linear is linear interpolation softened by a one-cell box filter, which is the quadratic
/// B-spline: smooth like Spline from 9 lattice values instead of 16. A fade between two values
/// has zero slope across every lattice line, which the turbulent types and high contrast turned
/// into a visible grid (#259); the B-spline's slope there is half the neighbours' difference.
pub fn typed_noise(x: f32, y: f32, z: f32, seed: u32, noise_type: u32) -> f32 {
    // Soft Linear's taps are centred on the nearest lattice point, the others start at the cell's.
    let h = if noise_type == NOISE_SOFT_LINEAR { 0.5 } else { 0.0 };
    let (x0, y0, z0) = ((x + h).floor(), (y + h).floor(), z.floor());
    let (tx, ty, fz) = (x + h - x0, y + h - y0, z - z0);
    let (ix, iy, iz) = (x0 as i32, y0 as i32, z0 as i32);
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let slice = |dz: i32| -> f32 {
        // Coordinates past the i32 range saturate; the neighbours wrap, as on the GPU (#3486).
        let c = |dx: i32, dy: i32| lattice(ix.wrapping_add(dx), iy.wrapping_add(dy), iz.wrapping_add(dz), seed);
        match noise_type {
            0 => c(0, 0),
            1 => l(l(c(0, 0), c(1, 0), tx), l(c(0, 1), c(1, 1), tx), ty),
            NOISE_SOFT_LINEAR => {
                let (wx, wy) = (bspline(tx), bspline(ty));
                let row = |dy: i32| wx[0] * c(-1, dy) + wx[1] * c(0, dy) + wx[2] * c(1, dy);
                wy[0] * row(-1) + wy[1] * row(0) + wy[2] * row(1)
            }
            _ => {
                let row = |dy: i32| catmull(c(-1, dy), c(0, dy), c(1, dy), c(2, dy), tx);
                catmull(row(-1), row(0), row(1), row(2), ty)
            }
        }
    };
    let n = l(slice(0), slice(1), fz);
    if noise_type == NOISE_SOFT_LINEAR { 0.5 + (n - 0.5) * SOFT_LINEAR_GAIN } else { n }
}

/// Everything the per-pixel evaluation needs, resolved from the parameters.
pub(crate) struct Fractal {
    pub kind: u32,
    pub noise_type: u32,
    pub contrast: f32,
    pub brightness: f32,
    pub overflow: u32,
    pub invert: bool,
    /// Rotation (sin, cos).
    pub rot: (f32, f32),
    /// Noise cell size in pixels along the rotated x / y axes.
    pub size: [f32; 2],
    /// Offset Turbulence in pixels, and the layer centre (for Perspective Offset).
    pub origin: [f32; 2],
    pub center: [f32; 2],
    pub perspective: bool,
    pub octaves: f32,
    pub influence: f32,
    /// Sub Scaling as a size ratio ([`sub_scaling_ratio`]: each octave is this much the size of the
    /// one before).
    pub sub_scaling: f32,
    pub sub_rotation: f32,
    /// Sub Offset in pixels.
    pub sub_offset: [f32; 2],
    pub center_subscale: bool,
    /// Evolution in revolutions.
    pub evolution: f32,
    /// Cycle length in revolutions when Cycle Evolution is on.
    pub cycle: Option<f32>,
    pub turbulence: f32,
    pub seed: u32,
}

#[inline]
fn shape(kind: u32, n: f32) -> f32 {
    let s = n * 2.0 - 1.0;
    match kind {
        1 => 1.0 - s * s,
        2 => 1.0 - s.abs(),
        3 => (1.0 - s.abs()).powi(2),
        8 => 1.0 - (s.abs() / 0.12).clamp(0.0, 1.0).powi(2),
        _ => n,
    }
}

impl Fractal {
    pub(crate) fn from_params(ctx: &EffectCtx, b: &Buf, turbulent: bool) -> Fractal {
        let pr = ctx.params;
        let uniform = pr.get("transform/uniformScaling").is_none_or(Value::as_bool);
        let s = pr.f("transform/scale");
        let (sw, sh) = if uniform { (s, s) } else { (pr.f("transform/scaleWidth"), pr.f("transform/scaleHeight")) };
        let rot = (pr.f("transform/rotation") as f32).to_radians();
        let (ox, oy) = b.to_px(pr.v2("transform/offset"));
        let (cx, cy) = b.to_px([ctx.layer_size[0] / 2.0, ctx.layer_size[1] / 2.0]);
        let so = pr.v2("subSettings/subOffset");
        let cycle = (!turbulent && pr.b("evolutionOptions/cycleEvolution")).then(|| pr.f("evolutionOptions/cycle").round().max(1.0) as f32);
        let seed = if turbulent { (pr.f("evolutionOptions/randomSeed") as i64 as u32) ^ 0x7a3d } else { pr.f("evolutionOptions/seed") as u32 ^ 0x51ed };
        Fractal {
            kind: pr.e("fractalType"),
            noise_type: pr.get("noiseType").map(Value::as_enum).unwrap_or(NOISE_SOFT_LINEAR),
            contrast: pr.f("contrast") as f32 / 100.0,
            brightness: pr.f("brightness") as f32 / 100.0,
            overflow: pr.e("overflow"),
            invert: pr.b("invert"),
            rot: rot.sin_cos(),
            size: [(sw * NOISE_CELL * b.scale).max(1.0) as f32, (sh * NOISE_CELL * b.scale).max(1.0) as f32],
            origin: [ox as f32, oy as f32],
            center: [cx as f32, cy as f32],
            perspective: pr.b("transform/perspectiveOffset"),
            octaves: pr.f("complexity").clamp(1.0, 20.0) as f32,
            influence: (pr.f("subSettings/subInfluence") as f32 / 100.0).clamp(0.0, 1.0),
            sub_scaling: sub_scaling_ratio(pr.f("subSettings/subScaling")) as f32,
            sub_rotation: (pr.f("subSettings/subRotation") as f32).to_radians(),
            sub_offset: [(so[0] * b.scale) as f32, (so[1] * b.scale) as f32],
            center_subscale: pr.b("subSettings/centerSubscale"),
            evolution: pr.f("evolution") as f32 / 360.0,
            cycle,
            turbulence: if turbulent { pr.f("evolutionOptions/turbulenceFactor").max(0.0) as f32 } else { 0.0 },
            seed,
        }
    }

    /// One noise layer at lattice coordinates (u, v) for octave `o` (evolution offset `z`), with
    /// Cycle Evolution's loop. With turbulence, finer layers evolve faster.
    #[inline]
    fn layer(&self, u: f32, v: f32, o: f32, z: f32, seed: u32) -> f32 {
        match self.cycle {
            Some(c) => {
                let t = self.evolution.rem_euclid(c);
                let w = t / c;
                let a = typed_noise(u, v, t + z, seed, self.noise_type);
                let b = typed_noise(u, v, t - c + z, seed, self.noise_type);
                a * (1.0 - w) + b * w
            }
            None => typed_noise(u, v, self.evolution * (1.0 + self.turbulence * o) + z, seed, self.noise_type),
        }
    }

    /// Octave `of`'s origin in pixels, at Perspective Offset `depth`: Perspective Offset moves finer
    /// (farther) layers less; turbulence makes them drift faster. The scaled offset stays within
    /// ±[`OCTAVE_OFFSET_MAX`] (#3486).
    #[inline]
    fn octave_origin(&self, of: f32, depth: f32) -> [f32; 2] {
        let k = if self.perspective { depth } else { 1.0 } * (1.0 + self.turbulence * of);
        let mut org = self.origin;
        if k != 1.0 {
            let d = |i: usize| ((self.origin[i] - self.center[i]) * k).clamp(-OCTAVE_OFFSET_MAX, OCTAVE_OFFSET_MAX);
            org = [self.center[0] + d(0), self.center[1] + d(1)];
        }
        if !self.center_subscale {
            org = [org[0] + self.sub_offset[0] * of, org[1] + self.sub_offset[1] * of];
        }
        org
    }

    /// Raw fractal value (before contrast / overflow) at pixel centre (x, y).
    pub(crate) fn raw(&self, x: f32, y: f32) -> f32 {
        let n_oct = self.octaves.ceil() as usize;
        let frac = self.octaves - self.octaves.floor();
        let (sr, cr) = self.rot;
        let (mut sum, mut norm, mut amp, mut f, mut best) = (0.0f32, 0.0f32, 1.0f32, 1.0f32, 0.0f32);
        let mut depth = 1.0f32;
        for o in 0..n_oct {
            let w = if o + 1 == n_oct && frac > 0.0 { frac } else { 1.0 };
            let of = o as f32;
            let org = self.octave_origin(of, depth);
            let (dx, dy) = (x - org[0], y - org[1]);
            let (mut u, mut v) = ((dx * cr + dy * sr) / self.size[0], (-dx * sr + dy * cr) / self.size[1]);
            if self.sub_rotation != 0.0 {
                let (s, c) = (self.sub_rotation * of).sin_cos();
                (u, v) = (u * c + v * s, -u * s + v * c);
            }
            let (mut u, mut v) = (u * f, v * f);
            let z = of * 7.31;
            let seed = self.seed.wrapping_add(o as u32);
            if (4..=6).contains(&self.kind) {
                // Dynamic: domain warp by a coarse field (Twist rotates, Progressive grows with depth).
                let k = if self.kind == 6 { 0.5 + of * 0.35 } else { 1.0 };
                let a = self.layer(u * 0.5, v * 0.5, of, z + 3.1, seed ^ 0x55) - 0.5;
                let bb = self.layer(u * 0.5, v * 0.5, of, z + 5.7, seed ^ 0xaa) - 0.5;
                if self.kind == 5 {
                    let (s, c) = (a * 3.0).sin_cos();
                    (u, v) = (u * c - v * s, u * s + v * c);
                } else {
                    u += a * 1.5 * k;
                    v += bb * 1.5 * k;
                }
            }
            let n = shape(self.kind, self.layer(u, v, of, z, seed));
            if self.kind == 7 {
                best = best.max(n * amp);
            }
            sum += n * amp * w;
            norm += amp * w;
            amp *= self.influence;
            (f, depth) = next_octave(f, depth, self.sub_scaling);
        }
        if self.kind == 7 { best } else { sum / norm.max(1e-6) }
    }

    /// Final grey value: contrast, brightness, overflow and invert applied.
    pub(crate) fn value(&self, x: f32, y: f32) -> f32 {
        let v = (self.raw(x, y) - 0.5) * self.contrast + 0.5 + self.brightness;
        let v = overflow(v, self.overflow);
        if self.invert { 1.0 - v } else { v }
    }
}

/// Fractal / Turbulent Noise resolved for the GPU kernel (effectcraft-gpu `fx_noise`): the
/// per-pixel constants and, per octave, the terms `Fractal::raw` computes before touching the
/// pixel, in the same f32 order.
pub struct FractalGpu {
    pub kind: u32,
    pub noise_type: u32,
    pub seed: u32,
    pub invert: bool,
    pub overflow: u32,
    pub contrast: f32,
    pub brightness: f32,
    /// Rotation (sin, cos).
    pub rot: (f32, f32),
    pub size: [f32; 2],
    /// Cycle Evolution: (t, cycle, t / cycle).
    pub cycle: Option<[f32; 3]>,
    pub sub_rotation: bool,
    /// Σ amplitude × weight (the normaliser).
    pub norm: f32,
    /// Per octave: origin x, y, sub-rotation sin, cos, frequency, amplitude, weight, evolution
    /// (turbulence applied), z offset, Dynamic Progressive warp factor.
    pub octaves: Vec<[f32; 10]>,
    /// Blending Mode (`None` = the noise alone), Opacity and Blend With Original.
    pub mode: Option<BlendMode>,
    pub opacity: f32,
    pub blend: f32,
}

/// [`FractalGpu`] for a buffer with `offset` / `scale`.
pub fn fractal_gpu(ctx: &EffectCtx, offset: [f64; 2], scale: f64, turbulent: bool) -> FractalGpu {
    let b = Buf { img: crate::raster::Image::new(0, 0), offset, scale };
    let fr = Fractal::from_params(ctx, &b, turbulent);
    let n_oct = fr.octaves.ceil() as usize;
    let frac = fr.octaves - fr.octaves.floor();
    let (mut norm, mut amp, mut f, mut depth) = (0.0f32, 1.0f32, 1.0f32, 1.0f32);
    let mut octaves = Vec::with_capacity(n_oct);
    for o in 0..n_oct {
        let w = if o + 1 == n_oct && frac > 0.0 { frac } else { 1.0 };
        let of = o as f32;
        let org = fr.octave_origin(of, depth);
        let (s, c) = (fr.sub_rotation * of).sin_cos();
        let evo = fr.evolution * (1.0 + fr.turbulence * of);
        let dk = if fr.kind == 6 { 0.5 + of * 0.35 } else { 1.0 };
        octaves.push([org[0], org[1], s, c, f, amp, w, evo, of * 7.31, dk]);
        norm += amp * w;
        amp *= fr.influence;
        (f, depth) = next_octave(f, depth, fr.sub_scaling);
    }
    let cycle = fr.cycle.map(|c| {
        let t = fr.evolution.rem_euclid(c);
        [t, c, t / c]
    });
    FractalGpu {
        kind: fr.kind,
        noise_type: fr.noise_type,
        seed: fr.seed,
        invert: fr.invert,
        overflow: fr.overflow,
        contrast: fr.contrast,
        brightness: fr.brightness,
        rot: fr.rot,
        size: fr.size,
        cycle,
        sub_rotation: fr.sub_rotation != 0.0,
        norm,
        octaves,
        mode: blend_mode(ctx.params.get("blendingMode").map(Value::as_enum).unwrap_or(BLEND_NONE)),
        opacity: (ctx.params.f("opacity") as f32 / 100.0).clamp(0.0, 1.0),
        blend: ctx.params.f("blend") as f32 / 100.0,
    }
}

/// Resolve a Blending Mode index: None → `None`, unknown → Normal.
fn blend_mode(m: u32) -> Option<BlendMode> {
    match m {
        BLEND_NONE => None,
        m => Some(BLEND_MODES.get(m as usize).and_then(|l| BlendMode::from_name(l)).unwrap_or(BlendMode::Normal)),
    }
}

/// Map a value by an [`OVERFLOWS`] option.
pub fn overflow(v: f32, mode: u32) -> f32 {
    match mode {
        1 => (0.5 + 0.5 * (2.0 * (v - 0.5)).tanh() / 1f32.tanh()).clamp(0.0, 1.0),
        2 => {
            let t = v.rem_euclid(2.0);
            if t > 1.0 { 2.0 - t } else { t }
        }
        3 => v,
        _ => v.clamp(0.0, 1.0),
    }
}

/// Composite grey noise `val` over the original pixel by Blending Mode, Opacity and (ours) Blend
/// With Original. The noise stays inside the layer's alpha, so a masked solid shows it only within
/// its masks (#939).
pub(crate) fn composite(orig: [f32; 4], val: f32, opacity: f32, mode: Option<BlendMode>, blend_orig: f32) -> [f32; 4] {
    let src = [val * opacity, val * opacity, val * opacity, opacity];
    let out = match mode {
        None => src,
        Some(bm) => blend_pixel(bm, orig, src, 0.5),
    };
    let k = 1.0 - blend_orig;
    [0, 1, 2, 3].map(|c| orig[c] * blend_orig + out[c] * orig[3] * k)
}

fn render(ctx: &EffectCtx, mut b: Buf, turbulent: bool) -> Buf {
    let fr = Fractal::from_params(ctx, &b, turbulent);
    let opacity = (ctx.params.f("opacity") as f32 / 100.0).clamp(0.0, 1.0);
    let mode = blend_mode(ctx.params.get("blendingMode").map(Value::as_enum).unwrap_or(BLEND_NONE));
    let blend = ctx.params.f("blend") as f32 / 100.0;
    b.img.rows_mut().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let v = fr.value(x as f32 + 0.5, y as f32 + 0.5);
            *px = composite(*px, v, opacity, mode, blend);
        }
    });
    b
}

fn fractal_noise(ctx: &EffectCtx, b: Buf) -> Buf {
    render(ctx, b, false)
}

fn turbulent_noise(ctx: &EffectCtx, b: Buf) -> Buf {
    render(ctx, b, true)
}


