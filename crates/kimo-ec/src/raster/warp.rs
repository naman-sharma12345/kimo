// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
//! Projective warps composited straight into a destination (the layer transform step).

use crate::color::{BlendMode, blend_pixel};
use crate::geom::{Mat3, Rect, vec2};
use rayon::prelude::*;

use crate::raster::{Image, Px, hash_noise};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sampling {
    Nearest,
    #[default]
    Bilinear,
    Bicubic,
}

#[derive(Clone, Copy, Debug)]
pub struct WarpOpts {
    pub sampling: Sampling,
    pub opacity: f32,
    pub mode: BlendMode,
    pub seed: u32,
    /// Only touch destination pixels inside this rect (dst pixel coords).
    pub clip: Option<Rect>,
}

impl Default for WarpOpts {
    fn default() -> Self {
        WarpOpts { sampling: Sampling::Bilinear, opacity: 1.0, mode: BlendMode::Normal, seed: 0, clip: None }
    }
}

/// Box-filter halving (for minification).
fn half(src: &Image) -> Image {
    let w = src.width.div_ceil(2).max(1);
    let h = src.height.div_ceil(2).max(1);
    let mut out = Image::new(w, h);
    out.rows_mut().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let (sx, sy) = (2 * x as i64, 2 * y as i64);
            let mut acc = [0.0f32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = src.get_clamped(sx + dx, sy + dy);
                for c in 0..4 {
                    acc[c] += p[c];
                }
            }
            *px = acc.map(|v| v * 0.25);
        }
    });
    out
}

/// Draw `src` transformed by `m` (src pixel space → dst pixel space) onto `dst` with blending.
///
/// Affine transforms with non-stencil modes take a fast path (per-row span clipping, interior
/// bilinear without bounds checks, integer-translation copies, inlined Normal blending); it
/// matches [`composite_warp_reference`] to float rounding.
pub fn composite_warp(dst: &mut Image, src: &Image, m: &Mat3, opts: &WarpOpts) {
    composite_warp_impl(dst, src, m, opts, true, None, None);
}

/// [`composite_warp`], also returning the destination pixels it changed when it knows them:
/// `Some([x0, y0, x1, y1])` (half-open, empty when nothing was drawn) on the fast affine path,
/// `None` otherwise. With `quantize` (an 8/16 bpc destination's levels), the fast path also
/// quantises each pixel it blends ([`crate::raster::quantize_px`]): exactly when this returns `Some`.
pub fn composite_warp_changed(dst: &mut Image, src: &Image, m: &Mat3, opts: &WarpOpts, quantize: Option<f32>) -> Option<[u32; 4]> {
    composite_warp_impl(dst, src, m, opts, true, None, quantize)
}

/// `dst += warp(src) · weight` (plain sum, no blending): motion-blur sub-samples accumulate
/// straight into one buffer instead of one full-size image per sample.
pub fn accumulate_warp(dst: &mut Image, src: &Image, m: &Mat3, sampling: Sampling, weight: f32) {
    let opts = WarpOpts { sampling, ..Default::default() };
    if m.is_affine() && m.inverse().is_some_and(|i| i.is_affine()) {
        composite_warp_impl(dst, src, m, &opts, true, Some(weight), None);
        return;
    }
    let mut tmp = Image::new(dst.width, dst.height);
    composite_warp(&mut tmp, src, m, &opts);
    dst.data.par_iter_mut().zip(tmp.data.par_iter()).for_each(|(d, s)| {
        for c in 0..4 {
            d[c] += s[c] * weight;
        }
    });
}

/// The straightforward per-pixel implementation (reference for the fast path's tests).
pub fn composite_warp_reference(dst: &mut Image, src: &Image, m: &Mat3, opts: &WarpOpts) {
    composite_warp_impl(dst, src, m, opts, false, None, None);
}

/// Bilinear blend of four taps (same operation order as [`Image::sample_bilinear`]).
#[inline(always)]
fn bilerp(a: Px, b: Px, c: Px, d: Px, tx: f32, ty: f32) -> Px {
    let mut o = [0.0; 4];
    for i in 0..4 {
        let top = a[i] + (b[i] - a[i]) * tx;
        let bot = c[i] + (d[i] - c[i]) * tx;
        o[i] = top + (bot - top) * ty;
    }
    o
}

/// Side of the square source tiles of [`ClearTiles`].
const TILE: usize = 16;

/// Which `TILE`×`TILE` source tiles are fully transparent, the one-pixel apron to their right
/// and below included: the four taps of a bilinear sample whose top-left tap lies in such a
/// tile all have zero alpha, so the sample is transparent and draws nothing.
struct ClearTiles {
    cols: usize,
    clear: Vec<bool>,
}

impl ClearTiles {
    fn new(img: &Image) -> ClearTiles {
        let (w, h) = (img.width as usize, img.height as usize);
        let (cols, rows) = (w.div_ceil(TILE), h.div_ceil(TILE));
        let clear = (0..rows)
            .into_par_iter()
            .flat_map_iter(|ty| {
                let mut row_clear = vec![true; cols];
                for y in ty * TILE..(ty * TILE + TILE + 1).min(h) {
                    let row = &img.data[y * w..(y + 1) * w];
                    for (tx, c) in row_clear.iter_mut().enumerate() {
                        if *c {
                            *c = row[tx * TILE..(tx * TILE + TILE + 1).min(w)].iter().all(|p| p[3] == 0.0);
                        }
                    }
                }
                row_clear
            })
            .collect();
        ClearTiles { cols, clear }
    }

    #[inline(always)]
    fn is_clear(&self, x: usize, y: usize) -> bool {
        self.clear.get((y / TILE) * self.cols + x / TILE).copied().unwrap_or(false)
    }
}

/// Bilinear sample with an unchecked interior fast path; `None` when every interior tap is in a
/// [`ClearTiles`] tile (the sample would have zero alpha).
#[inline(always)]
fn sample_bilinear_fast(img: &Image, clear: &ClearTiles, x: f64, y: f64) -> Option<Px> {
    let fx = x - 0.5;
    let fy = y - 0.5;
    let x0 = fx.floor();
    let y0 = fy.floor();
    let (xi, yi) = (x0 as i64, y0 as i64);
    let (w, h) = (img.width as i64, img.height as i64);
    if xi >= 0 && yi >= 0 && xi + 1 < w && yi + 1 < h {
        if clear.is_clear(xi as usize, yi as usize) {
            return None;
        }
        let tx = (fx - x0) as f32;
        let ty = (fy - y0) as f32;
        let i = yi as usize * w as usize + xi as usize;
        let row0 = &img.data[i..i + 2];
        let row1 = &img.data[i + w as usize..i + w as usize + 2];
        return Some(bilerp(row0[0], row0[1], row1[0], row1[1], tx, ty));
    }
    Some(img.sample_bilinear(x, y))
}

/// Index range `[lo, hi)` of `i` in `0..n` for which `p0 + i·s` may lie in `[min, max]`
/// (conservative by one step on each side; callers still check per pixel).
fn span(p0: f64, s: f64, min: f64, max: f64, n: usize) -> (usize, usize) {
    if s.abs() < 1e-12 {
        return if p0 >= min - 1e-9 && p0 <= max + 1e-9 { (0, n) } else { (0, 0) };
    }
    let (a, b) = ((min - p0) / s, (max - p0) / s);
    let (a, b) = if a < b { (a, b) } else { (b, a) };
    let lo = (a.floor() - 1.0).max(0.0);
    let hi = (b.ceil() + 2.0).min(n as f64);
    if hi <= lo { (0, 0) } else { (lo as usize, hi as usize) }
}

/// Columns `[a, b)` of one destination row that a warp changed (`a == b`: none).
#[derive(Clone, Copy)]
struct RowSpan {
    a: usize,
    b: usize,
}

impl RowSpan {
    const NONE: RowSpan = RowSpan { a: usize::MAX, b: 0 };
    #[inline(always)]
    fn add(&mut self, x: usize) {
        self.a = self.a.min(x);
        self.b = x + 1;
    }
}

/// Union of changed pixels as `[x0, y0, x1, y1]` (half-open; `x0 >= x1` when empty).
fn union(r: [usize; 4], q: [usize; 4]) -> [usize; 4] {
    match (r[0] < r[2], q[0] < q[2]) {
        (false, _) => q,
        (_, false) => r,
        _ => [r[0].min(q[0]), r[1].min(q[1]), r[2].max(q[2]), r[3].max(q[3])],
    }
}

/// Sampling, blending and destination bounds for an affine row operation.
struct AffineRows {
    sampling: Sampling,
    opacity: f32,
    quantize: Option<f32>,
    bounds: [usize; 4],
}

/// Blend visible source samples and return the rectangle of changed pixels.
fn affine_rows(dst: &mut Image, img: &Image, inv: &Mat3, rows: AffineRows, blend: impl Fn(Px, Px, u32, u32) -> Px + Sync) -> [usize; 4] {
    let AffineRows { sampling, opacity: op, quantize, bounds: [x0, x1, y0, y1] } = rows;
    let levels = quantize.map(|l| (l, 1.0 / l));
    let w = dst.width as usize;
    let m = inv.0;
    let (iw, ih) = (img.width as f64, img.height as f64);
    let step = vec2(m[0][0], m[1][0]);
    // Pure integer translation: every sample lands on a pixel centre, so all filters return the
    // source pixel itself (bicubic still clamps negatives, as in the reference).
    let int_tx = m[0][0] == 1.0 && m[1][1] == 1.0 && m[0][1] == 0.0 && m[1][0] == 0.0 && m[0][2].fract() == 0.0 && m[1][2].fract() == 0.0;
    // Bilinear samples skip the transparent parts of the source (most of a thin shape's or a
    // glow-padded layer's buffer) without reading them.
    let clear = (sampling == Sampling::Bilinear && !int_tx).then(|| ClearTiles::new(img));
    let empty = [usize::MAX, usize::MAX, 0, 0];
    dst.data
        .par_chunks_mut(w)
        .enumerate()
        .skip(y0)
        .take(y1 - y0)
        .map(|(y, row)| {
            let mut changed = RowSpan::NONE;
            let fy = y as f64 + 0.5;
            let p0 = vec2(m[0][0] * (x0 as f64 + 0.5) + m[0][1] * fy + m[0][2], m[1][0] * (x0 as f64 + 0.5) + m[1][1] * fy + m[1][2]);
            let n = x1 - x0;
            let (ax, bx) = span(p0.x, step.x, -1.0, iw + 1.0, n);
            let (ay, by) = span(p0.y, step.y, -1.0, ih + 1.0, n);
            let (lo, hi) = (ax.max(ay), bx.min(by));
            if lo >= hi {
                return empty;
            }
            if int_tx {
                let sy = y as i64 + m[1][2] as i64;
                if sy < 0 || sy >= img.height as i64 {
                    return empty;
                }
                let dx = m[0][2] as i64;
                let srow = &img.data[sy as usize * img.width as usize..(sy as usize + 1) * img.width as usize];
                let xa = (x0 as i64 + lo as i64).max(-dx).max(0) as usize;
                let xb = ((x0 + hi) as i64).min(img.width as i64 - dx).max(0) as usize;
                for x in xa..xb.max(xa) {
                    let mut s = srow[(x as i64 + dx) as usize];
                    if sampling == Sampling::Bicubic {
                        s = [s[0].max(0.0), s[1].max(0.0), s[2].max(0.0), s[3].max(0.0)];
                    }
                    if s[3] <= 0.0 {
                        continue;
                    }
                    if op < 1.0 {
                        for c in s.iter_mut() {
                            *c *= op;
                        }
                    }
                    row[x] = blend(row[x], s, x as u32, y as u32);
                    if let Some((l, inv)) = levels {
                        crate::raster::quantize_px(&mut row[x], l, inv);
                    }
                    changed.add(x);
                }
            } else {
                for i in lo..hi {
                    let x = x0 + i;
                    let p = vec2(p0.x + step.x * i as f64, p0.y + step.y * i as f64);
                    if p.x < -1.0 || p.y < -1.0 || p.x > iw + 1.0 || p.y > ih + 1.0 {
                        continue;
                    }
                    let mut s = match (sampling, &clear) {
                        (Sampling::Nearest, _) => img.get(p.x.floor() as i64, p.y.floor() as i64),
                        (Sampling::Bilinear, Some(clear)) => match sample_bilinear_fast(img, clear, p.x, p.y) {
                            Some(s) => s,
                            None => continue,
                        },
                        (Sampling::Bilinear, None) => img.sample_bilinear(p.x, p.y),
                        (Sampling::Bicubic, _) => img.sample_bicubic(p.x, p.y),
                    };
                    if s[3] <= 0.0 {
                        continue;
                    }
                    if op < 1.0 {
                        for c in s.iter_mut() {
                            *c *= op;
                        }
                    }
                    row[x] = blend(row[x], s, x as u32, y as u32);
                    if let Some((l, inv)) = levels {
                        crate::raster::quantize_px(&mut row[x], l, inv);
                    }
                    changed.add(x);
                }
            }
            if changed.a < changed.b { [changed.a, y, changed.b, y + 1] } else { empty }
        })
        .reduce(|| empty, union)
}

fn composite_warp_impl(
    dst: &mut Image,
    src: &Image,
    m: &Mat3,
    opts: &WarpOpts,
    allow_fast: bool,
    accumulate: Option<f32>,
    quantize: Option<f32>,
) -> Option<[u32; 4]> {
    const NOTHING: Option<[u32; 4]> = Some([0; 4]);
    if src.is_empty() || dst.is_empty() || opts.opacity <= 0.0 {
        return NOTHING;
    }
    let Some(inv) = m.inverse() else { return NOTHING };
    // Minification: pick a pre-filtered level so sampling doesn't alias.
    let scale = m.mean_scale();
    let mut level_img;
    let mut img = src;
    let mut inv = inv;
    if scale < 0.5 && m.is_affine() {
        let mut s = scale;
        let mut f = 1.0;
        level_img = half(src);
        f *= 0.5;
        s *= 2.0;
        while s < 0.5 && level_img.width > 1 && level_img.height > 1 {
            level_img = half(&level_img);
            f *= 0.5;
            s *= 2.0;
        }
        img = &level_img;
        inv = Mat3::scale(vec2(f, f)) * inv;
    }
    let bounds = m.map_rect(&Rect::from_size(src.width as f64, src.height as f64)).inflate(1.0).round_out();
    let full = Rect::from_size(dst.width as f64, dst.height as f64);
    let mut area = bounds.intersect(&full);
    if let Some(c) = opts.clip {
        area = area.intersect(&c);
    }
    if area.is_empty() {
        return NOTHING;
    }
    let (x0, x1) = (area.x0.max(0.0) as usize, area.x1.min(dst.width as f64) as usize);
    let (y0, y1) = (area.y0.max(0.0) as usize, area.y1.min(dst.height as f64) as usize);
    let w = dst.width as usize;
    let affine = inv.is_affine();
    let mode = opts.mode;
    let op = opts.opacity;
    let changed = |r: [usize; 4]| Some(r.map(|v| v.min(u32::MAX as usize) as u32));
    if let Some(k) = accumulate
        && affine
    {
        let r = affine_rows(dst, img, &inv, AffineRows { sampling: opts.sampling, opacity: 1.0, quantize: None, bounds: [x0, x1, y0, y1] }, |d, s, _, _| {
            [d[0] + s[0] * k, d[1] + s[1] * k, d[2] + s[2] * k, d[3] + s[3] * k]
        });
        return changed(r);
    }
    if allow_fast && affine && !mode.is_stencil() {
        let r = match mode {
            BlendMode::Normal => {
                affine_rows(dst, img, &inv, AffineRows { sampling: opts.sampling, opacity: op, quantize, bounds: [x0, x1, y0, y1] }, |d, s, _, _| {
                    let k = 1.0 - s[3];
                    [s[0] + d[0] * k, s[1] + d[1] * k, s[2] + d[2] * k, s[3] + d[3] * k]
                })
            }
            BlendMode::Dissolve | BlendMode::DancingDissolve => {
                let seed = opts.seed;
                affine_rows(dst, img, &inv, AffineRows { sampling: opts.sampling, opacity: op, quantize, bounds: [x0, x1, y0, y1] }, |d, s, x, y| {
                    blend_pixel(mode, d, s, hash_noise(x, y, seed))
                })
            }
            _ => affine_rows(dst, img, &inv, AffineRows { sampling: opts.sampling, opacity: op, quantize, bounds: [x0, x1, y0, y1] }, |d, s, _, _| {
                blend_pixel(mode, d, s, 0.5)
            }),
        };
        return changed(r);
    }
    dst.data.par_chunks_mut(w).enumerate().skip(y0).take(y1 - y0).for_each(|(y, row)| {
        let fy = y as f64 + 0.5;
        let mut sp = inv.apply(vec2(x0 as f64 + 0.5, fy));
        let step = inv.apply_vec(vec2(1.0, 0.0));
        for x in x0..x1 {
            let p = if affine {
                let p = sp;
                sp += step;
                p
            } else {
                inv.apply(vec2(x as f64 + 0.5, fy))
            };
            if p.x < -1.0 || p.y < -1.0 || p.x > img.width as f64 + 1.0 || p.y > img.height as f64 + 1.0 {
                if mode.is_stencil() {
                    row[x] = blend_pixel(mode, row[x], [0.0; 4], 0.5);
                }
                continue;
            }
            let mut s = match opts.sampling {
                Sampling::Nearest => img.get(p.x.floor() as i64, p.y.floor() as i64),
                Sampling::Bilinear => img.sample_bilinear(p.x, p.y),
                Sampling::Bicubic => img.sample_bicubic(p.x, p.y),
            };
            if s[3] <= 0.0 && !mode.is_stencil() {
                continue;
            }
            if op < 1.0 {
                for c in s.iter_mut() {
                    *c *= op;
                }
            }
            let n = if matches!(mode, BlendMode::Dissolve | BlendMode::DancingDissolve) { hash_noise(x as u32, y as u32, opts.seed) } else { 0.5 };
            row[x] = blend_pixel(mode, row[x], s, n);
        }
    });
    // Stencil modes also clear everything outside the layer bounds.
    if mode.is_stencil() && matches!(mode, BlendMode::StencilAlpha | BlendMode::StencilLuma) {
        for (y, row) in dst.data.chunks_mut(w).enumerate() {
            for (x, px) in row.iter_mut().enumerate() {
                if x < x0 || x >= x1 || y < y0 || y >= y1 {
                    *px = [0.0; 4];
                }
            }
        }
    }
    None
}

/// Resize with bilinear filtering (box pre-filter when shrinking).
pub fn resample(src: &Image, w: u32, h: u32) -> Image {
    let mut out = Image::new(w, h);
    if src.is_empty() || w == 0 || h == 0 {
        return out;
    }
    let m = Mat3::scale(vec2(w as f64 / src.width as f64, h as f64 / src.height as f64));
    composite_warp(&mut out, src, &m, &WarpOpts::default());
    out
}


