// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed; only the image, blur and noise parts kept.
//! Raster images for the compositor.
//!
//! [`Image`] stores premultiplied RGBA in `f32` (values may exceed 1.0 in 32 bpc projects). Every
//! heavy operation is row-parallel with rayon.


pub mod blur;

use crate::color::{BlendMode, blend_pixel};
use rayon::prelude::*;

pub use blur::{
    RADIAL_BLUR_SAMPLES, box_blur, directional_blur, gaussian_blur, gaussian_blur_exact, gaussian_blur_unbounded, gaussian_blur_unbounded_at,
    gaussian_is_exact, radial_blur, radial_blur_spin_degrees, radial_blur_zoom_scale,
};

pub type Px = [f32; 4];

/// Maximum padded side in pixels — mirrors the project crate's `limits::MAX_SIZE` (30 000;
/// raster sits below it in the layering and may not depend on it, so the two must stay equal).
/// A padded side beyond the largest supported comp side has no render use.
pub const MAX_PADDED_SIDE: u32 = 30_000;

/// Clamp (colour ≤ alpha ≤ 1, NaN → 0) and round half up to `levels` steps (`inv` = 1 / levels):
/// one pixel of an 8/16 bpc buffer. Values are non-negative after the clamp, so the integer
/// conversion rounds like `round()` and vectorises.
#[inline(always)]
pub fn quantize_px(p: &mut Px, levels: f32, inv: f32) {
    *p = quantize_levels(*p, levels, inv).map(|n| n * inv);
}

/// [`quantize_px`]'s steps: the whole number of `levels` each channel rounds to (0..=levels).
#[allow(clippy::manual_clamp)] // max/min map NaN to 0; clamp would keep NaN
#[inline(always)]
pub fn quantize_levels(p: Px, levels: f32, inv: f32) -> [f32; 4] {
    let na = round_half_up(p[3].max(0.0).min(1.0) * levels);
    let a = na * inv;
    let hi = [a, a, a, a];
    let mut n = [0.0, 0.0, 0.0, na];
    for c in 0..3 {
        n[c] = round_half_up(p[c].max(0.0).min(hi[c]) * levels);
    }
    n
}

/// [`quantize_px`] at 8 bpc, as bytes: [`expand_u8`] gives back exactly the pixel
/// [`quantize_px`] writes, so an 8 bpc buffer can be kept in a quarter of the memory.
#[inline(always)]
pub fn quantize_px_u8(p: Px) -> [u8; 4] {
    quantize_levels(p, 255.0, 1.0 / 255.0).map(|n| n as u8)
}

/// The 8 bpc pixel of [`quantize_px_u8`]'s bytes.
#[inline(always)]
pub fn expand_u8(n: [u8; 4]) -> Px {
    n.map(|v| f32::from(v) * (1.0 / 255.0))
}

/// `v` (0..=65 535) rounded half up. The same either way; the fastest on each target: x86-64's
/// baseline converts to integers in one instruction but has no `floor` (SSE4.1), while in wasm
/// `floor` is one instruction and converting back from `u32` several (about 1.5× faster there).
#[inline(always)]
fn round_half_up(v: f32) -> f32 {
    if cfg!(target_arch = "wasm32") { (v + 0.5).floor() } else { (v + 0.5) as u32 as f32 }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: Vec<Px>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Image {
        Image { width, height, data: vec![[0.0; 4]; width as usize * height as usize] }
    }
    pub fn filled(width: u32, height: u32, px: Px) -> Image {
        Image { width, height, data: vec![px; width as usize * height as usize] }
    }
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
    #[inline]
    pub fn idx(&self, x: u32, y: u32) -> usize {
        y as usize * self.width as usize + x as usize
    }
    #[inline]
    pub fn get(&self, x: i64, y: i64) -> Px {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 { [0.0; 4] } else { self.data[y as usize * self.width as usize + x as usize] }
    }
    #[inline]
    pub fn get_clamped(&self, x: i64, y: i64) -> Px {
        let x = x.clamp(0, self.width as i64 - 1);
        let y = y.clamp(0, self.height as i64 - 1);
        self.data[y as usize * self.width as usize + x as usize]
    }
    #[inline]
    pub fn set(&mut self, x: u32, y: u32, px: Px) {
        let i = self.idx(x, y);
        self.data[i] = px;
    }
    pub fn rows_mut(&mut self) -> impl IndexedParallelIterator<Item = (usize, &mut [Px])> {
        let w = self.width.max(1) as usize;
        self.data.par_chunks_mut(w).enumerate()
    }
    /// Bilinear sample at continuous pixel coordinates (pixel centres at +0.5); transparent outside.
    #[inline]
    pub fn sample_bilinear(&self, x: f64, y: f64) -> Px {
        let fx = x - 0.5;
        let fy = y - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = (fx - x0) as f32;
        let ty = (fy - y0) as f32;
        let (x0, y0) = (x0 as i64, y0 as i64);
        let a = self.get(x0, y0);
        let b = self.get(x0 + 1, y0);
        let c = self.get(x0, y0 + 1);
        let d = self.get(x0 + 1, y0 + 1);
        let mut o = [0.0; 4];
        for i in 0..4 {
            let top = a[i] + (b[i] - a[i]) * tx;
            let bot = c[i] + (d[i] - c[i]) * tx;
            o[i] = top + (bot - top) * ty;
        }
        o
    }
    /// Bilinear with edge pixels repeated outside the image.
    #[inline]
    pub fn sample_bilinear_clamped(&self, x: f64, y: f64) -> Px {
        let fx = x - 0.5;
        let fy = y - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = (fx - x0) as f32;
        let ty = (fy - y0) as f32;
        let (x0, y0) = (x0 as i64, y0 as i64);
        let a = self.get_clamped(x0, y0);
        let b = self.get_clamped(x0 + 1, y0);
        let c = self.get_clamped(x0, y0 + 1);
        let d = self.get_clamped(x0 + 1, y0 + 1);
        let mut o = [0.0; 4];
        for i in 0..4 {
            let top = a[i] + (b[i] - a[i]) * tx;
            let bot = c[i] + (d[i] - c[i]) * tx;
            o[i] = top + (bot - top) * ty;
        }
        o
    }
    /// Catmull-Rom bicubic sample; transparent outside.
    pub fn sample_bicubic(&self, x: f64, y: f64) -> Px {
        let fx = x - 0.5;
        let fy = y - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let w = |t: f64| -> [f32; 4] {
            let t2 = t * t;
            let t3 = t2 * t;
            [(-0.5 * t3 + t2 - 0.5 * t) as f32, (1.5 * t3 - 2.5 * t2 + 1.0) as f32, (-1.5 * t3 + 2.0 * t2 + 0.5 * t) as f32, (0.5 * t3 - 0.5 * t2) as f32]
        };
        let wx = w(tx);
        let wy = w(ty);
        let mut o = [0.0f32; 4];
        for j in 0..4 {
            for i in 0..4 {
                let p = self.get(x0 as i64 - 1 + i as i64, y0 as i64 - 1 + j as i64);
                let k = wx[i] * wy[j];
                for c in 0..4 {
                    o[c] += p[c] * k;
                }
            }
        }
        // Keep premultiplied invariants after ringing.
        o[3] = o[3].max(0.0);
        for c in 0..3 {
            o[c] = o[c].max(0.0);
        }
        o
    }
    /// The pad [`Image::padded`] applies for a requested `pad`: clamped so neither output side
    /// exceeds [`MAX_PADDED_SIDE`]. At most `(MAX_PADDED_SIDE - side) / 2`, so
    /// `side + 2 · pad ≤ MAX_PADDED_SIDE` and overflow is unreachable by construction (a side
    /// already past the cap gets pad 0). Callers that track where the pixels went (a buffer
    /// offset) must move by this, not by the requested pad.
    pub fn effective_pad(&self, pad: u32) -> u32 {
        pad.min(MAX_PADDED_SIDE.saturating_sub(self.width) / 2).min(MAX_PADDED_SIDE.saturating_sub(self.height) / 2)
    }
    /// Copy with [`Image::effective_pad`] transparent pixels on every side.
    ///
    /// The clamp keeps the output within the largest image the engine supports (a max-size comp
    /// frame), so an unbounded pad cannot overflow or size a buffer past that. It is not a memory
    /// budget: callers that pad by a derived, possibly large amount bound the area themselves
    /// (`DOF_PAD_BUDGET` in render's `three_d/compose.rs`, `fits()` on the GPU path) or reject an
    /// oversized pad first (`padded_size`); legitimate bounded pads are byte-identical.
    pub fn padded(&self, pad: u32) -> Image {
        let eff = self.effective_pad(pad);
        let mut out = Image::new(self.width + 2 * eff, self.height + 2 * eff);
        let ow = out.width as usize;
        for y in 0..self.height as usize {
            let src = &self.data[y * self.width as usize..(y + 1) * self.width as usize];
            let o = (y + eff as usize) * ow + eff as usize;
            out.data[o..o + self.width as usize].copy_from_slice(src);
        }
        out
    }
    /// Sub-rectangle copy (clipped).
    pub fn crop(&self, x: i64, y: i64, w: u32, h: u32) -> Image {
        let mut out = Image::new(w, h);
        out.rows_mut().for_each(|(oy, row)| {
            for (ox, px) in row.iter_mut().enumerate() {
                *px = self.get(x + ox as i64, y + oy as i64);
            }
        });
        out
    }
    /// Multiply every pixel (premultiplied) by `k`.
    pub fn scale_alpha(&mut self, k: f32) {
        if (k - 1.0).abs() < 1e-7 {
            return;
        }
        self.data.par_iter_mut().for_each(|p| {
            for c in p.iter_mut() {
                *c *= k;
            }
        });
    }
    /// Multiply each pixel by a coverage value from `mask` (same size).
    pub fn mul_mask(&mut self, mask: &[f32]) {
        self.data.par_iter_mut().zip(mask.par_iter()).for_each(|(p, &m)| {
            for c in p.iter_mut() {
                *c *= m;
            }
        });
    }
    /// Clamp to [0, 1] (8/16 bpc working space).
    pub fn clamp01(&mut self) {
        self.data.par_iter_mut().for_each(|p| {
            for c in p.iter_mut() {
                *c = c.clamp(0.0, 1.0);
            }
            for c in 0..3 {
                p[c] = p[c].min(p[3]);
            }
        });
    }
    /// Quantise to `levels` per channel (simulates 8/16 bpc output).
    pub fn quantize(&mut self, levels: f32) {
        self.data.par_iter_mut().for_each(|p| {
            for c in p.iter_mut() {
                *c = (*c * levels).round() / levels;
            }
        });
    }
    /// Straight-alpha 8-bit RGBA (sRGB-encoded values as stored).
    pub fn to_rgba8(&self) -> Vec<u8> {
        self.map_rgba8(|o, p, _, _| {
            let a = p[3].clamp(0.0, 1.0);
            let inv = if a > 0.0 { 1.0 / p[3] } else { 0.0 };
            for c in 0..3 {
                o[c] = ((p[c] * inv).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            o[3] = (a * 255.0 + 0.5) as u8;
        })
    }
    /// [`Image::to_rgba8`] with the colour dithered ([`dither_u8`]); alpha is rounded.
    pub fn to_rgba8_dithered(&self) -> Vec<u8> {
        self.map_rgba8(|o, p, x, y| {
            let a = p[3].clamp(0.0, 1.0);
            let inv = if a > 0.0 { 1.0 / p[3] } else { 0.0 };
            for c in 0..3 {
                o[c] = dither_u8(p[c] * inv, x, y);
            }
            o[3] = (a * 255.0 + 0.5) as u8;
        })
    }
    /// Composite over an opaque background colour and return opaque 8-bit RGBA.
    pub fn to_rgba8_over(&self, bg: [f32; 3]) -> Vec<u8> {
        self.map_rgba8(|o, p, _, _| {
            let k = 1.0 - p[3].clamp(0.0, 1.0);
            for c in 0..3 {
                o[c] = ((p[c] + bg[c] * k).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            o[3] = 255;
        })
    }
    /// [`Image::to_rgba8_over`] with the colour dithered ([`dither_u8`]).
    pub fn to_rgba8_over_dithered(&self, bg: [f32; 3]) -> Vec<u8> {
        self.map_rgba8(|o, p, x, y| {
            let k = 1.0 - p[3].clamp(0.0, 1.0);
            for c in 0..3 {
                o[c] = dither_u8(p[c] + bg[c] * k, x, y);
            }
            o[3] = 255;
        })
    }
    /// 8-bit RGBA with each pixel written by `f` (given its x and y), row-parallel.
    fn map_rgba8(&self, f: impl Fn(&mut [u8; 4], &Px, u32, u32) + Sync) -> Vec<u8> {
        let mut out = vec![0u8; self.data.len() * 4];
        let w = self.width.max(1) as usize;
        out.par_chunks_mut(w * 4).zip(self.data.par_chunks(w)).enumerate().for_each(|(y, (o, row))| {
            for (x, (o, p)) in o.as_chunks_mut::<4>().0.iter_mut().zip(row).enumerate() {
                f(o, p, x as u32, y as u32);
            }
        });
        out
    }
    /// From straight-alpha 8-bit RGBA.
    pub fn from_rgba8(width: u32, height: u32, rgba: &[u8]) -> Image {
        let mut img = Image::new(width, height);
        img.data.par_iter_mut().zip(rgba.par_chunks(4)).for_each(|(p, s)| {
            let a = s[3] as f32 / 255.0;
            *p = [s[0] as f32 / 255.0 * a, s[1] as f32 / 255.0 * a, s[2] as f32 / 255.0 * a, a];
        });
        img
    }
    /// Straight-alpha colour at a pixel.
    pub fn straight(&self, x: i64, y: i64) -> Px {
        let p = self.get(x, y);
        if p[3] > 0.0 { [p[0] / p[3], p[1] / p[3], p[2] / p[3], p[3]] } else { [0.0; 4] }
    }
    /// Map every pixel through `f` on straight colour (keeps alpha).
    pub fn map_straight(&mut self, f: impl Fn([f32; 3]) -> [f32; 3] + Sync) {
        self.data.par_iter_mut().for_each(|p| {
            let a = p[3];
            if a <= 0.0 {
                return;
            }
            let c = f([p[0] / a, p[1] / a, p[2] / a]);
            *p = [c[0] * a, c[1] * a, c[2] * a, a];
        });
    }
    /// Blend `src` (same size) onto self with a mode and opacity.
    pub fn blend_from(&mut self, src: &Image, mode: BlendMode, opacity: f32, seed: u32) {
        let w = self.width as usize;
        self.data.par_chunks_mut(w.max(1)).zip(src.data.par_chunks(w.max(1))).enumerate().for_each(|(y, (d, s))| {
            for x in 0..d.len() {
                let mut sp = s[x];
                if sp[3] <= 0.0 && !mode.is_stencil() {
                    continue;
                }
                for c in sp.iter_mut() {
                    *c *= opacity;
                }
                let n = if matches!(mode, BlendMode::Dissolve | BlendMode::DancingDissolve) { hash_noise(x as u32, y as u32, seed) } else { 0.5 };
                d[x] = blend_pixel(mode, d[x], sp, n);
            }
        });
    }
    /// Bounding box of non-transparent pixels (x0, y0, x1, y1), None if empty.
    pub fn alpha_bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let w = self.width as usize;
        let mut b: Option<(u32, u32, u32, u32)> = None;
        for (y, row) in self.data.chunks(w.max(1)).enumerate() {
            if let Some(first) = row.iter().position(|p| p[3] > 1e-6) {
                let last = row.iter().rposition(|p| p[3] > 1e-6).unwrap_or(first);
                let (y, f, l) = (y as u32, first as u32, last as u32 + 1);
                b = Some(match b {
                    None => (f, y, l, y + 1),
                    Some((x0, y0, x1, _)) => (x0.min(f), y0, x1.max(l), y + 1),
                });
            }
        }
        b
    }
}

/// Deterministic per-pixel noise in 0..1.
#[inline]
pub fn hash_noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Seed of the output dither: fixed, so a frame converts to the same bytes every time.
const DITHER_SEED: u32 = 0x5bd1_e995;

/// A colour value in 0..1 as an 8-bit code, dithered by pixel position (#2022): the value is
/// scaled to 0..255 and rounded up or down against a per-pixel hash threshold, so the codes of
/// a smooth gradient average to the gradient instead of stepping in bands. A value already on
/// an 8-bit code (anything rendered at 8 bpc) comes out unchanged. Deterministic: depends only
/// on the value and the pixel position, never on a clock or a global generator.
#[inline]
pub fn dither_u8(v: f32, x: u32, y: u32) -> u8 {
    let s = v.clamp(0.0, 1.0) * 255.0;
    // The threshold stays inside (1e-4, 1 - 1e-4) so float error on an exact code can't flip it.
    let t = hash_noise(x, y, DITHER_SEED) * (1.0 - 2e-4) + 1e-4;
    (s + t).floor() as u8
}

/// A single-channel coverage mask.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mask {
    pub width: u32,
    pub height: u32,
    pub data: Vec<f32>,
}

impl Mask {
    pub fn new(width: u32, height: u32, v: f32) -> Mask {
        Mask { width, height, data: vec![v; width as usize * height as usize] }
    }
    pub fn to_image(&self) -> Image {
        Image { width: self.width, height: self.height, data: self.data.iter().map(|&a| [a, a, a, a]).collect() }
    }
    pub fn from_alpha(img: &Image) -> Mask {
        Mask { width: img.width, height: img.height, data: img.data.iter().map(|p| p[3]).collect() }
    }
}


