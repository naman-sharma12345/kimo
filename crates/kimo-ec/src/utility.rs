// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
//! Utility effects: bounds growth, HDR inspection/compression, Cineon log conversion and colour
//! lookup tables (.cube).

use std::sync::Arc;

use crate::file_cache::{self, Registered};
use crate::color::luminance;
use crate::Value;
use crate::ParamUi;
use rayon::prelude::*;

use crate::util::{premul, unpremul};
use crate::{Buf, EffectCtx, EffectSpec, Params, col, num, p, popup, slider};

fn spec(id: &'static str, name: &'static str, params: Vec<crate::ParamSpec>, render: crate::RenderFn) -> EffectSpec {
    EffectSpec { id, name, category: "Utility", params, render, gpu: false, float: true }
}

fn grow_bounds(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let px = (ctx.params.f("pixels") * b.scale).round().max(0.0) as u32;
    if !ctx.adjustment {
        b.pad(px);
    }
    b
}

/// Visualise out-of-range values: in-range shown as dim grey, > 1 in the highlight colour,
/// < 0 in its complement.
fn overbrights(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let ch = ctx.params.e("channel");
    let hc = ctx.params.color("highlightColor");
    b.img.data.par_iter_mut().for_each(|px| {
        let (c, a) = unpremul(*px);
        if a <= 0.0 {
            return;
        }
        let v = match ch {
            1 => c[0],
            2 => c[1],
            3 => c[2],
            4 => a,
            5 => luminance(c[0], c[1], c[2]),
            _ => c[0].max(c[1]).max(c[2]),
        };
        let o = if v > 1.0 {
            [hc[0], hc[1], hc[2]]
        } else if v < 0.0 {
            [1.0 - hc[0], 1.0 - hc[1], 1.0 - hc[2]]
        } else {
            let g = v * 0.35;
            [g, g, g]
        };
        *px = premul(o, a);
    });
    b
}

/// Luma weights HDR Highlight Compression divides by (measured against After Effects 2026).
const HDR_LUMA: [f32; 3] = [0.30, 0.59, 0.11];
/// Exponent of the domain the Amount blend happens in (out^0.4 is mixed linearly).
const HDR_BLEND_GAMMA: f32 = 0.4;

/// `v^e` with the sign of `v` kept (negative input stays negative, as on the GPU).
#[inline]
fn hdr_spow(v: f32, e: f32) -> f32 {
    v.signum() * v.abs().powf(e)
}

/// One straight-colour pixel through HDR Highlight Compression at blend `t` (Amount / 100, > 0).
///
/// Measured behaviour (16-bit renders of a 0–4 HDR ramp and coloured patches at Amount 100,
/// 90, 84, 75, 50 and 25): every channel is divided by `1 + Y` (`Y` = 0.30 R + 0.59 G + 0.11 B, so a grey
/// value `x` lands on `x / (1 + x)`), clipped at 1, then mixed with the input as `x^0.4` values.
fn hdr_compress_pixel(c: [f32; 3], t: f32) -> [f32; 3] {
    let y = (HDR_LUMA[0] * c[0] + HDR_LUMA[1] * c[1] + HDR_LUMA[2] * c[2]).max(0.0);
    let s = 1.0 / (1.0 + y);
    let g = HDR_BLEND_GAMMA;
    c.map(|x| {
        let comp = (x * s).min(1.0);
        let mixed = (1.0 - t) * hdr_spow(x, g) + t * hdr_spow(comp, g);
        let o = hdr_spow(mixed, 1.0 / g);
        if o.is_finite() { o } else { x }
    })
}

/// HDR Highlight Compression: every straight-colour pixel through [`hdr_compress_pixel`] at
/// Amount / 100; Amount 0 or NaN returns the layer unchanged.
fn hdr_compression(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let t = (ctx.params.f("amount") / 100.0).clamp(0.0, 1.0) as f32;
    if t <= 0.0 || !t.is_finite() {
        return b;
    }
    b.img.map_straight(|c| hdr_compress_pixel(c, t));
    b
}

// ---- Cineon (Kodak printing-density log encoding: 0.002 density per code value, negative gamma 0.6) ----

#[derive(Clone, Copy, Debug)]
pub(crate) struct Cineon {
    pub black: f32,
    pub white: f32,
    pub ib: f32,
    pub iw: f32,
    /// Version 2: [`cineon_gamma`]. Version 1: the saved Gamma, where 1.7 gives linear output.
    pub gamma: f32,
    pub rolloff: f32,
    pub version_2: bool,
}

impl Cineon {
    fn offset(&self) -> f32 {
        10f32.powf((self.black - self.white) * 0.002 / 0.6)
    }
    fn knee(&self) -> f32 {
        1.0 - (self.rolloff / 100.0).clamp(0.0, 0.99)
    }
    /// Normalized code value (code / 1023) to linear output.
    pub fn log_to_lin(&self, v: f32) -> f32 {
        let lin = if self.version_2 {
            let offset = self.offset().powf(1.0 / self.gamma.max(0.001));
            let exposure = 10f32.powf((v * 1023.0 - self.white) * 0.002 / 0.6).powf(1.0 / self.gamma.max(0.001));
            let mut lin = ((exposure - offset) / (1.0 - offset).max(1e-6)).max(0.0);
            let k = self.knee();
            if k < 1.0 && lin > k {
                let w = 1.0 - k;
                lin = k + w * (1.0 - (-(lin - k) / w).exp());
            }
            lin
        } else {
            // Version 1 keeps its earlier arithmetic, so saved instances render the same bits.
            crate::cineon_log_to_lin(v, self.black, self.white, 1.7, self.gamma, self.rolloff)
        };
        self.ib + (self.iw - self.ib) * lin
    }
    /// Linear to normalized code value.
    pub fn lin_to_log(&self, v: f32) -> f32 {
        let off = self.offset();
        let d = self.iw - self.ib;
        let mut lin = (v - self.ib) / if d.abs() < 1e-6 { 1e-6 } else { d };
        let k = self.knee();
        if k < 1.0 && lin > k {
            let w = 1.0 - k;
            lin = k - w * (1.0 - (lin - k) / w).max(1e-6).ln();
        }
        let exposure = if self.version_2 {
            let root_offset = off.powf(1.0 / self.gamma.max(0.001));
            (lin.max(0.0) * (1.0 - root_offset) + root_offset).max(0.0).powf(self.gamma.max(0.001))
        } else {
            lin.max(0.0).powf(self.gamma.max(0.01) / 1.7) * (1.0 - off) + off
        };
        let code = self.white + exposure.max(1e-9).log10() * 0.6 / 0.002;
        code / 1023.0
    }
}

/// The Gamma that gives linear output in Cineon Converter version 1.
pub const CINEON_V1_LINEAR_GAMMA: f64 = 1.7;

/// The Gamma scale used by Cineon Converter version `version`: `1 / 1.7` for version 1, 1 for
/// version 2. Preset conversion aligns the linear points but not other Gamma values. Version 1
/// instances (saved before the hidden Version parameter existed; see
/// [`crate::migrate::legacy_default`]) raised the linear value to `1.7 / gamma`.
pub fn cineon_gamma_scale(version: f64) -> f64 {
    if version < 2.0 { 1.0 / CINEON_V1_LINEAR_GAMMA } else { 1.0 }
}

/// Cineon Converter's Gamma on the version 2 scale ([`cineon_gamma_scale`]), limited to `0.01 /
/// 1.7` or more: version 1 limited its saved Gamma to 0.01.
pub fn cineon_gamma(params: &Params) -> f64 {
    (params.f("gamma") * cineon_gamma_scale(params.f("version"))).max(0.01 * cineon_gamma_scale(1.0))
}

fn cineon(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let version_2 = ctx.params.f("version") >= 2.0;
    let c = Cineon {
        black: ctx.params.f("tenBitBlackPoint") as f32,
        white: ctx.params.f("tenBitWhitePoint") as f32,
        ib: ctx.params.f("internalBlackPoint") as f32,
        iw: ctx.params.f("internalWhitePoint") as f32,
        gamma: if version_2 { cineon_gamma(ctx.params) } else { ctx.params.f("gamma") } as f32,
        rolloff: ctx.params.f("highlightRolloff") as f32,
        version_2,
    };
    let std = Cineon { black: 95.0, white: 685.0, ib: 0.0, iw: 1.0, gamma: 1.7, rolloff: c.rolloff, version_2: false };
    let kind = ctx.params.e("conversionType");
    b.img.map_straight(|v| {
        v.map(|x| match kind {
            0 => c.lin_to_log(x),
            1 => c.log_to_lin(x),
            _ if c.version_2 => {
                // Custom points use the Gamma 1 code conversion before the code-value power.
                let code = if (c.black, c.white, c.ib, c.iw) == (95.0, 685.0, 0.0, 1.0) {
                    x
                } else {
                    let unit = Cineon { gamma: 1.0, ..c };
                    std.lin_to_log(unit.log_to_lin(x))
                };
                code.max(0.0).powf(1.0 / c.gamma.max(0.001))
            }
            _ => std.lin_to_log(c.log_to_lin(x)),
        })
    });
    b
}

// ---- Colour LUTs (.cube) ----

#[derive(Clone, Debug, PartialEq)]
pub struct Lut {
    pub n1: usize,
    pub d1: Vec<[f32; 3]>,
    pub n3: usize,
    pub d3: Vec<[f32; 3]>,
    pub dmin: [f32; 3],
    pub dmax: [f32; 3],
}

// Cube LUT Specification 1.0 §§6–7 permits at most 65,536 1D rows and a 256³
// lattice. Allow their sum for combined shaper + 3D files, regardless of header order.
const MAX_CUBE_ROWS: usize = 65_536 + 256 * 256 * 256;

/// Parse `.cube` text (TITLE, LUT_1D_SIZE, LUT_3D_SIZE, DOMAIN_MIN/MAX, data red-fastest).
pub fn parse_cube(text: &str) -> Option<Lut> {
    parse_cube_limited(text, MAX_CUBE_ROWS)
}

fn parse_cube_limited(text: &str, max_rows: usize) -> Option<Lut> {
    let mut lut = Lut { n1: 0, d1: vec![], n3: 0, d3: vec![], dmin: [0.0; 3], dmax: [1.0; 3] };
    let mut rows: Vec<[f32; 3]> = Vec::new();
    let three = |it: &mut std::str::SplitWhitespace| -> Option<[f32; 3]> {
        let a = it.next()?.parse().ok()?;
        let b = it.next()?.parse().ok()?;
        let c = it.next()?.parse().ok()?;
        Some([a, b, c])
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        let key = it.clone().next()?;
        match key {
            "TITLE" => continue,
            "LUT_1D_SIZE" => {
                it.next();
                lut.n1 = it.next()?.parse().ok()?;
            }
            "LUT_3D_SIZE" => {
                it.next();
                lut.n3 = it.next()?.parse().ok()?;
            }
            "DOMAIN_MIN" => {
                it.next();
                lut.dmin = three(&mut it)?;
            }
            "DOMAIN_MAX" => {
                it.next();
                lut.dmax = three(&mut it)?;
            }
            "LUT_1D_INPUT_RANGE" | "LUT_3D_INPUT_RANGE" => {
                it.next();
                let lo: f32 = it.next()?.parse().ok()?;
                let hi: f32 = it.next()?.parse().ok()?;
                lut.dmin = [lo; 3];
                lut.dmax = [hi; 3];
            }
            k if k.starts_with(|c: char| c.is_ascii_alphabetic()) => continue,
            _ => {
                if rows.len() == max_rows {
                    return None;
                }
                let row = three(&mut it)?;
                // Make buffer growth fallible before push can allocate.
                rows.try_reserve(1).ok()?;
                rows.push(row);
            }
        }
    }
    if lut.n3 > 256 {
        return None;
    }
    let need1 = lut.n1;
    let need3 = lut.n3 * lut.n3 * lut.n3;
    if (lut.n1 == 0 && lut.n3 == 0) || lut.n1 == 1 || lut.n3 == 1 || Some(rows.len()) != need1.checked_add(need3) {
        return None;
    }
    lut.d1 = rows.get(..need1)?.to_vec();
    lut.d3 = rows.get(need1..)?.to_vec();
    Some(lut)
}

impl Lut {
    fn norm(&self, c: [f32; 3]) -> [f32; 3] {
        [0, 1, 2].map(|i| ((c[i] - self.dmin[i]) / (self.dmax[i] - self.dmin[i]).max(1e-9)).clamp(0.0, 1.0))
    }
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let mut c = c;
        if self.n1 > 1 {
            let t = self.norm(c);
            c = [0, 1, 2].map(|i| {
                let x = t[i] * (self.n1 - 1) as f32;
                let i0 = (x.floor() as usize).min(self.n1 - 2);
                let f = x - i0 as f32;
                self.d1[i0][i] + (self.d1[i0 + 1][i] - self.d1[i0][i]) * f
            });
        }
        if self.n3 > 1 {
            c = self.tetra(self.norm(c));
        }
        c
    }
    /// Apply with a choice of 3D interpolation: 0 nearest, 1 trilinear, otherwise tetrahedral.
    pub fn apply_interp(&self, c: [f32; 3], interp: u32) -> [f32; 3] {
        let mut c = c;
        if self.n1 > 1 {
            let t = self.norm(c);
            c = [0, 1, 2].map(|i| {
                let x = t[i] * (self.n1 - 1) as f32;
                if interp == 0 {
                    return self.d1[(x.round() as usize).min(self.n1 - 1)][i];
                }
                let i0 = (x.floor() as usize).min(self.n1 - 2);
                let f = x - i0 as f32;
                self.d1[i0][i] + (self.d1[i0 + 1][i] - self.d1[i0][i]) * f
            });
        }
        if self.n3 > 1 {
            let t = self.norm(c);
            c = match interp {
                0 => self.nearest(t),
                1 => self.trilinear(t),
                _ => self.tetra(t),
            };
        }
        c
    }
    fn nearest(&self, t: [f32; 3]) -> [f32; 3] {
        let n = self.n3;
        let s = (n - 1) as f32;
        let i = |x: f32| ((x * s).round() as usize).min(n - 1);
        self.d3[i(t[0]) + i(t[1]) * n + i(t[2]) * n * n]
    }
    /// Trilinear interpolation on the lattice (red fastest).
    pub fn trilinear(&self, t: [f32; 3]) -> [f32; 3] {
        let n = self.n3;
        let s = (n - 1) as f32;
        let idx = |x: f32| {
            let v = x * s;
            let i = (v.floor() as usize).min(n - 2);
            (i, v - i as f32)
        };
        let (r, fr) = idx(t[0]);
        let (g, fg) = idx(t[1]);
        let (b, fb) = idx(t[2]);
        let at = |dr: usize, dg: usize, db: usize| self.d3[(r + dr) + (g + dg) * n + (b + db) * n * n];
        let l = |a: [f32; 3], b: [f32; 3], f: f32| [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f];
        let c00 = l(at(0, 0, 0), at(1, 0, 0), fr);
        let c10 = l(at(0, 1, 0), at(1, 1, 0), fr);
        let c01 = l(at(0, 0, 1), at(1, 0, 1), fr);
        let c11 = l(at(0, 1, 1), at(1, 1, 1), fr);
        l(l(c00, c10, fg), l(c01, c11, fg), fb)
    }
    fn tetra(&self, t: [f32; 3]) -> [f32; 3] {
        let n = self.n3;
        let s = (n - 1) as f32;
        let idx = |x: f32| {
            let v = x * s;
            let i = (v.floor() as usize).min(n - 2);
            (i, v - i as f32)
        };
        let (r, fr) = idx(t[0]);
        let (g, fg) = idx(t[1]);
        let (b, fb) = idx(t[2]);
        let at = |dr: usize, dg: usize, db: usize| self.d3[(r + dr) + (g + dg) * n + (b + db) * n * n];
        let c000 = at(0, 0, 0);
        let c111 = at(1, 1, 1);
        let (w1, w2, w3, p1, p2) = if fr > fg {
            if fg > fb {
                (fr, fg, fb, at(1, 0, 0), at(1, 1, 0))
            } else if fr > fb {
                (fr, fb, fg, at(1, 0, 0), at(1, 0, 1))
            } else {
                (fb, fr, fg, at(0, 0, 1), at(1, 0, 1))
            }
        } else if fb > fg {
            (fb, fg, fr, at(0, 0, 1), at(0, 1, 1))
        } else if fb > fr {
            (fg, fb, fr, at(0, 1, 0), at(0, 1, 1))
        } else {
            (fg, fr, fb, at(0, 1, 0), at(1, 1, 0))
        };
        // c = c000 + w1 (p1 - c000) + w2 (p2 - p1) + w3 (c111 - p2), w1 ≥ w2 ≥ w3.
        [0, 1, 2].map(|i| c000[i] + w1 * (p1[i] - c000[i]) + w2 * (p2[i] - p1[i]) + w3 * (c111[i] - p2[i]))
    }
}

/// At most this many parsed LUTs (files and inline texts) are kept, and at most
/// [`MAX_CACHED_BYTES`] of them; the least recently used go first.
const MAX_CACHED_LUTS: usize = 64;
const MAX_CACHED_BYTES: usize = 512 << 20;

/// [`load_lut`]'s cache, by parameter text.
static LUTS: Registered<Option<Arc<Lut>>> = Registered::new(MAX_CACHED_LUTS, MAX_CACHED_BYTES);

/// The parameters (paths in the effect's group) of `effect` that name a file an effect reads
/// through a cache that follows it ([`load_lut`], the OCIO file transforms and configs).
pub fn lut_params(effect: &str) -> &'static [&'static str] {
    match effect {
        "ec.utility.applylut" => &["lut"],
        "ec.color.lumetri" => &["basicCorrection/inputLutFile", "creative/lookFile"],
        "ec.color.ociofile" => &["file"],
        "ec.color.ociocolorspace" | "ec.color.ociodisplay" => &["configFile"],
        _ => &[],
    }
}

/// The file a LUT parameter names; `None` for inline `.cube` text or an empty parameter.
pub fn lut_file(src: &str) -> Option<&str> {
    // `.cube` text always spans lines (a size header, then data rows); a path never does, even one
    // whose file name contains `LUT_` (#3454). Trimmed, so a trailing newline is still a path.
    let path = src.trim();
    (!path.is_empty() && !path.contains('\n')).then_some(path)
}

/// The files a [`lut_params`] parameter set to `src` reads: the ones its cached result was read
/// from (an OCIO config's also the LUTs it names, and the places it looked for missing ones; an
/// `.ocio` config not read yet is read), else the file it names.
pub fn lut_files(src: &str) -> Vec<String> {
    // A config not read yet is read now: the LUTs it names are its files from the first render.
    if crate::ocio_config::cached_files(src).is_none()
        && lut_file(src).is_some_and(|p| std::path::Path::new(p).extension().is_some_and(|e| e.eq_ignore_ascii_case("ocio")))
    {
        let _ = crate::ocio_config::load_config(src);
    }
    crate::ocio_config::cached_files(src)
        .or_else(|| crate::ocio::cached_files(src))
        .or_else(|| file_cache::lock(LUTS.get()).files(src))
        .unwrap_or_else(|| lut_file(src).map(str::to_string).into_iter().collect())
}

/// Load (and cache) a LUT from inline `.cube` text or a file path. A file is stamped (one
/// `metadata` call) on every load and read again once its size or modification time moved (see
/// [`crate::file_cache`]); inline text is its own content.
pub fn load_lut(src: &str) -> Option<Arc<Lut>> {
    if src.trim().is_empty() {
        return None;
    }
    file_cache::cached(LUTS.get(), src, || {
        let lut = match lut_file(src) {
            Some(path) => file_cache::read(path).ok().and_then(|t| parse_cube(&t)),
            None => parse_cube(src),
        }
        .map(Arc::new);
        let bytes = lut.as_ref().map_or(0, |l| lut_bytes(l));
        (lut, bytes)
    })
}

/// The size of a LUT's tables.
pub(crate) fn lut_bytes(l: &Lut) -> usize {
    l.d1.len().saturating_add(l.d3.len()).saturating_mul(std::mem::size_of::<[f32; 3]>())
}

fn apply_lut(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let Some(lut) = load_lut(ctx.params.s("lut")) else {
        return b;
    };
    b.img.map_straight(|c| lut.apply(c));
    b
}

pub fn specs() -> Vec<EffectSpec> {
    vec![
        spec("ec.utility.growbounds", "Grow Bounds", vec![p("pixels", "Pixels", num(0.0), slider(0.0, 4000.0, 0.0, 500.0, 0))], grow_bounds),
        spec(
            "ec.utility.ccoverbrights",
            "Out-of-Range Display",
            vec![
                p("channel", "Channel", Value::Enum(0), popup(&["Max RGB", "Red", "Green", "Blue", "Alpha", "Luminance"])),
                p("highlightColor", "Highlight Color", col(1.0, 0.0, 0.0), ParamUi::Color),
            ],
            overbrights,
        ),
        spec(
            "ec.utility.hdrcompression",
            "HDR Highlight Compression",
            vec![p("amount", "Amount", num(100.0), slider(0.0, 100.0, 0.0, 100.0, 1))],
            hdr_compression,
        ),
        spec(
            "ec.utility.cineon",
            "Cineon Converter",
            vec![
                p("conversionType", "Conversion Type", Value::Enum(1), popup(&["Linear to Log", "Log to Linear", "Log to Log"])),
                p("tenBitBlackPoint", "10 Bit Black Point", num(95.0), slider(0.0, 1023.0, 0.0, 1023.0, 0)),
                p("internalBlackPoint", "Internal Black Point", num(0.0), slider(-1.0, 2.0, 0.0, 1.0, 3)),
                p("tenBitWhitePoint", "10 Bit White Point", num(685.0), slider(0.0, 1023.0, 0.0, 1023.0, 0)),
                p("internalWhitePoint", "Internal White Point", num(1.0), slider(-1.0, 2.0, 0.0, 1.0, 3)),
                p("gamma", "Gamma", num(1.7), slider(0.05, 5.0, 0.1, 5.0, 2)),
                p("highlightRolloff", "Highlight Rolloff", num(20.0), slider(0.0, 100.0, 0.0, 100.0, 0)),
                // 2: Gamma 1 gives linear output (`cineon_gamma`).
                p("version", "Version", num(2.0), ParamUi::Hidden),
            ],
            cineon,
        ),
        // `lut`: a path to a `.cube` file (Effect Controls' Choose…, asked for when the effect is
        // applied, as in After Effects), or the `.cube` text itself (commands/automation).
        spec("ec.utility.applylut", "Apply Color LUT", vec![p("lut", "LUT", Value::Str(String::new()), crate::file_ui(&["cube"]))], apply_lut),
    ]
}


