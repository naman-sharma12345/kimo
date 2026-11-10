//! Camera Lens Blur (After Effects "Camera Lens Blur", Premiere "Lens Blur"): a defocus that looks like a lens instead of a
//! Gaussian. Each pixel gathers its neighbours over the iris shape (a flat disc or an n-blade polygon with rotation and
//! roundness) in linear light, after boosting highlights, so bright points open into crisp bokeh shapes instead of dim smears.
//! The blur radius can fall off around a focus point (cx, cy, focus_radius, falloff) so a subject stays exactly sharp; effect
//! masks (set_effect_mask) limit it to any region. Large radii gather on a box-reduced copy (factor radius/12) and are
//! upsampled, so cost stays bounded (taps <= 384 per pixel at the work resolution).
//! A depth (blur) map (AE Camera Lens Blur "Blur Map"): add_lens_blur depth_clip_id names another clip; layerfx.rs reads it per pixel and
//! hands apply_map a 0..1 scale per pixel (|map - focal_depth| normalised), so the radius follows the map and a scale of 0 keeps the pixel exact.
use crate::effects::{p, Frame, Params};

fn eotf(v: f32) -> f32 { if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) } }
fn oetf(l: f32) -> f32 { let l = l.max(0.0); if l <= 0.0031308 { 12.92 * l } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 } }

/// Unit-iris taps: a golden-angle spiral filling the disc (even area density), each pushed onto the polygon when blades >= 3.
/// Each tap carries weight s^2 (s = radial stretch onto the polygon) so the iris is evenly lit, not brighter toward its flats.
pub fn taps(n: usize, blades: u32, rotation_deg: f32, roundness: f32) -> Vec<(f32, f32, f32)> {
    let ga = std::f32::consts::PI * (3.0 - 5f32.sqrt()); let rot = rotation_deg.to_radians();
    (0..n).map(|i| {
        let r = ((i as f32 + 0.5) / n as f32).sqrt(); let th = i as f32 * ga;
        let s = if blades >= 3 { let seg = std::f32::consts::TAU / blades as f32; let a = (th - rot - std::f32::consts::FRAC_PI_2).rem_euclid(seg) - seg * 0.5;
            let poly = (seg * 0.5).cos() / a.cos(); poly + (1.0 - poly) * roundness.clamp(0.0, 1.0) } else { 1.0 };
        (r * s * th.cos(), r * s * th.sin(), s * s) }).collect()
}

#[derive(Clone, Copy, Debug)]
pub struct Lens { pub radius: f32, pub blades: u32, pub rotation: f32, pub roundness: f32, pub gain: f32, pub threshold: f32, pub cx: f32, pub cy: f32, pub focus_radius: f32, pub falloff: f32 }

/// Parameter names the effect reads (add_lens_blur writes exactly these), with defaults.
pub const PARAMS: &[(&str, f64)] = &[("radius", 12.0), ("blades", 6.0), ("rotation", 0.0), ("roundness", 0.0), ("highlight_gain", 2.0), ("highlight_threshold", 0.75), ("cx", 0.5), ("cy", 0.5), ("focus_radius", 0.0), ("falloff", 0.1)];

pub fn from_params(pr: &Params, t: f64) -> Lens {
    let g = |k: &str| p(pr, k, PARAMS.iter().find(|x| x.0 == k).map_or(0.0, |x| x.1), t);
    Lens { radius: g("radius").max(0.0), blades: g("blades").round().max(0.0) as u32, rotation: g("rotation"), roundness: g("roundness"), gain: g("highlight_gain").max(0.0),
        threshold: g("highlight_threshold").clamp(0.0, 0.999), cx: g("cx"), cy: g("cy"), focus_radius: g("focus_radius").max(0.0), falloff: g("falloff").max(1e-3) }
}

pub fn apply_params(pr: &Params, f: &mut Frame, t: f64) { apply(f, &from_params(pr, t)); }

/// Per-pixel blur radius in full-resolution pixels: full outside the focus area, smoothstep up over `falloff` (fractions of the short side),
/// times the depth map's scale at that pixel when there is one (w*h values 0..1).
fn radius_at(l: &Lens, w: usize, h: usize, x: f32, y: f32, depth: Option<&[f32]>) -> f32 {
    let r = if l.focus_radius <= 0.0 { l.radius } else {
        let md = w.min(h) as f32; let d = ((x - l.cx * w as f32).powi(2) + (y - l.cy * h as f32).powi(2)).sqrt() / md;
        let k = ((d - l.focus_radius) / l.falloff).clamp(0.0, 1.0); l.radius * k * k * (3.0 - 2.0 * k) };
    match depth { None => r, Some(dm) => { let (xi, yi) = ((x.max(0.0) as usize).min(w - 1), (y.max(0.0) as usize).min(h - 1)); r * dm[yi * w + xi].clamp(0.0, 1.0) } }
}

pub fn apply(f: &mut Frame, l: &Lens) { apply_map(f, l, None) }

/// apply with an optional per-pixel radius scale (depth map, w*h, 0..1; layerfx.rs lens_depth builds it).
pub fn apply_map(f: &mut Frame, l: &Lens, depth: Option<&[f32]>) {
    let (w, h) = (f.w, f.h); if w == 0 || h == 0 || l.radius < 0.5 { return; }
    let depth = depth.filter(|d| d.len() == w * h);
    // linear, premultiplied, highlight-boosted source at full resolution
    let mut src = vec![0f32; w * h * 4];
    for i in 0..w * h { let a = f.d[i * 4 + 3]; let (r, g, b) = (f.d[i * 4], f.d[i * 4 + 1], f.d[i * 4 + 2]);
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b; let k = ((luma - l.threshold) / (1.0 - l.threshold).max(1e-3)).clamp(0.0, 1.0); let boost = 1.0 + l.gain * k * k;
        src[i * 4] = eotf(r) * boost * a; src[i * 4 + 1] = eotf(g) * boost * a; src[i * 4 + 2] = eotf(b) * boost * a; src[i * 4 + 3] = a; }
    // work resolution: box-reduce by s so the iris is at most ~12 px there; energy-preserving (mean), so bokeh brightness holds
    let s = ((l.radius / 12.0).floor() as usize).max(1); let (sw, sh) = (w.div_ceil(s), h.div_ceil(s));
    let small: Vec<f32> = if s == 1 { src.clone() } else { let mut v = vec![0f32; sw * sh * 4];
        for y in 0..sh { for x in 0..sw { let mut acc = [0f32; 4]; let mut n = 0f32;
            for yy in y * s..((y + 1) * s).min(h) { for xx in x * s..((x + 1) * s).min(w) { let i = (yy * w + xx) * 4; for j in 0..4 { acc[j] += src[i + j]; } n += 1.0; } }
            let o = (y * sw + x) * 4; for j in 0..4 { v[o + j] = acc[j] / n; } } } v };
    let rmax = l.radius / s as f32;
    let n = ((std::f32::consts::PI * rmax * rmax / 2.0) as usize).clamp(24, 384);
    let tp = taps(n, l.blades, l.rotation, l.roundness); let wsum: f32 = tp.iter().map(|t| t.2).sum(); let inv = 1.0 / wsum;
    let (smallr, tpr) = (&small, &tp); let mut blur = vec![0f32; sw * sh * 4];
    crate::par::rows_mut(&mut blur, sw * 4, |y, row| {
        for x in 0..sw { let o = x * 4;
            let r = radius_at(l, w, h, (x as f32 + 0.5) * s as f32, (y as f32 + 0.5) * s as f32, depth) / s as f32;
            if r < 0.5 / s as f32 { let i = (y * sw + x) * 4; row[o..o + 4].copy_from_slice(&smallr[i..i + 4]); continue; }
            let mut acc = [0f32; 4];
            for &(tx, ty, tw) in tpr.iter() {
                // bilinear sample so sparse taps on a large iris still read as a continuous shape
                let (sx, sy) = ((x as f32 + tx * r).clamp(0.0, sw as f32 - 1.0), (y as f32 + ty * r).clamp(0.0, sh as f32 - 1.0));
                let (x0, y0) = (sx as usize, sy as usize); let (x1, y1) = ((x0 + 1).min(sw - 1), (y0 + 1).min(sh - 1)); let (ax, ay) = (sx - x0 as f32, sy - y0 as f32);
                let (i00, i10, i01, i11) = ((y0 * sw + x0) * 4, (y0 * sw + x1) * 4, (y1 * sw + x0) * 4, (y1 * sw + x1) * 4);
                for j in 0..4 { let top = smallr[i00 + j] + (smallr[i10 + j] - smallr[i00 + j]) * ax; let bot = smallr[i01 + j] + (smallr[i11 + j] - smallr[i01 + j]) * ax; acc[j] += (top + (bot - top) * ay) * tw; } }
            for j in 0..4 { row[o + j] = acc[j] * inv; } } });
    // back to full resolution; where the full-res radius is under half a pixel the original pixel is kept bit-exact
    for y in 0..h { for x in 0..w { let i = (y * w + x) * 4;
        let rf = radius_at(l, w, h, x as f32 + 0.5, y as f32 + 0.5, depth);
        if rf < 0.5 { continue; }
        let mut px = [0f32; 4];
        if s == 1 { px.copy_from_slice(&blur[i..i + 4]); } else {
            let (sx, sy) = (((x as f32 + 0.5) / s as f32 - 0.5).clamp(0.0, sw as f32 - 1.0), ((y as f32 + 0.5) / s as f32 - 0.5).clamp(0.0, sh as f32 - 1.0));
            let (x0, y0) = (sx as usize, sy as usize); let (x1, y1) = ((x0 + 1).min(sw - 1), (y0 + 1).min(sh - 1)); let (ax, ay) = (sx - x0 as f32, sy - y0 as f32);
            for j in 0..4 { let q = |xx: usize, yy: usize| blur[(yy * sw + xx) * 4 + j]; let top = q(x0, y0) + (q(x1, y0) - q(x0, y0)) * ax; let bot = q(x0, y1) + (q(x1, y1) - q(x0, y1)) * ax; px[j] = top + (bot - top) * ay; }
            // a pixel inside the falloff ramp but under s px of blur mixes towards the sharp source in proportion
            if rf < s as f32 { let k = rf / s as f32; for j in 0..4 { px[j] = src[i + j] + (px[j] - src[i + j]) * k; } } }
        let a = px[3]; f.d[i + 3] = a.clamp(0.0, 1.0);
        for j in 0..3 { f.d[i + j] = if a > 1e-6 { oetf(px[j] / a).min(1.0) } else { 0.0 }; } } }
}
