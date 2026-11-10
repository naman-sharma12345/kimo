//! Skin smoothing (Resolve Face Refinement-style), display-referred Rec.709. A soft skin-tone key in HSV (hue within skin_width
//! of skin_hue with a 10 deg soft edge, saturation 0.18..0.45 with soft edges to 0.10 / 0.6, value above 0.35 fading to 0.2;
//! feathered by a box blur of r/4) selects skin, or the
//! whole frame with skin_only 0 (for use under a tracked face mask: track_mask / track_matte + set_effect_mask). Inside it, frequency
//! separation on luma: base = self-guided filter of Y (window 2r, eps 0.02), which flattens blotches and blemishes while keeping
//! strong edges (eyes, lips, jaw line); fine = (Y - base) minus its box mean of radius max(1, r/6), the pore band, put back scaled
//! by `texture`. Chroma blotches are evened by a mask-normalised Gaussian blur of Cb/Cr (sigma r) blended by `color_even`.
//! Y' = Y + amount*m*(base + texture*fine - Y); C' = C + amount*color_even*m*(blur C - C); r = radius*height/720 px; alpha kept.
use crate::effects::{Frame, Params};

pub fn catalog() -> Vec<super::Entry> { vec![
    ("skin_smooth", "Skin smoothing (Resolve Face Refinement): skin key + frequency separation; blemishes go, pores and edges stay. amount 0..1", super::lv(vec![
        ("amount", 0.5, "0..1 strength (0 identity)"), ("radius", 10.0, "blemish size in px at 720p height, 1..80"), ("texture", 0.6, "0..1 share of fine pore texture kept"),
        ("color_even", 0.5, "0..1 evens red/blotchy skin colour"), ("skin_hue", 25.0, "skin hue centre (HSV), deg 0..360"), ("skin_width", 15.0, "half-width of the skin hue key, deg 1..90"),
        ("skin_only", 1.0, "1 = only skin tones, 0 = everywhere (use under a face mask)"), ("show_matte", 0.0, "1 = view the skin key")])),
] }

fn get(pr: &Params, k: &str, d: f64, t: f64) -> f64 { pr.get(k).map(|x| x.at(t)).unwrap_or(d) }
fn box_mean(src: &[f64], w: usize, h: usize, r: usize) -> Vec<f64> { let mut a = vec![0.0; src.len()]; let mut b = vec![0.0; src.len()]; super::boost_detail::box_pass(src, &mut a, w, h, r, true); super::boost_detail::box_pass(&a, &mut b, w, h, r, false); b }
fn smooth(e0: f64, e1: f64, x: f64) -> f64 { let u = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0); u * u * (3.0 - 2.0 * u) }

/// Soft skin key from HSV hue/saturation/value.
fn skin_key(p: [f64; 3], hue: f64, width: f64) -> f64 {
    let (mx, mn) = (p[0].max(p[1]).max(p[2]), p[0].min(p[1]).min(p[2])); let c = mx - mn; if !(mx > 0.0) || !(c > 0.0) { return 0.0; }
    let h = if mx == p[0] { 60.0 * ((p[1] - p[2]) / c) } else if mx == p[1] { 60.0 * ((p[2] - p[0]) / c + 2.0) } else { 60.0 * ((p[0] - p[1]) / c + 4.0) }.rem_euclid(360.0);
    let d = ((h - hue).rem_euclid(360.0)).min((hue - h).rem_euclid(360.0)); let wh = 1.0 - smooth(width, width + 10.0, d);
    let s = c / mx; let ws = smooth(0.10, 0.18, s) * (1.0 - smooth(0.45, 0.6, s)); let wv = smooth(0.2, 0.35, mx);
    wh * ws * wv
}

pub fn apply(pr: &Params, f: &mut Frame, t: f64) -> Result<(), String> {
    let n = f.w * f.h; if f.d.len() < n * 4 { return Err("skin_smooth: frame buffer too short".into()); }
    let (amount, radius, texture, even) = (get(pr, "amount", 0.5, t), get(pr, "radius", 10.0, t), get(pr, "texture", 0.6, t), get(pr, "color_even", 0.5, t));
    let (hue, width, only, show) = (get(pr, "skin_hue", 25.0, t), get(pr, "skin_width", 15.0, t), get(pr, "skin_only", 1.0, t), get(pr, "show_matte", 0.0, t));
    for (k, v, lo, hi) in [("amount", amount, 0.0, 1.0), ("radius", radius, 1.0, 80.0), ("texture", texture, 0.0, 1.0), ("color_even", even, 0.0, 1.0), ("skin_hue", hue, 0.0, 360.0), ("skin_width", width, 1.0, 90.0), ("skin_only", only, 0.0, 1.0), ("show_matte", show, 0.0, 1.0)] {
        if !v.is_finite() || v < lo || v > hi { return Err(format!("skin_smooth: {k} must be in {lo}..{hi}, got {v}")); } }
    if (amount == 0.0 && show < 0.5) || n == 0 { return Ok(()); }
    let (w, h) = (f.w, f.h); let r = ((radius * h as f64 / 720.0).round() as usize).max(1);
    let px: Vec<[f64; 3]> = f.d.chunks_exact(4).take(n).map(|p| [0, 1, 2].map(|j| { let v = p[j] as f64; if v.is_finite() { v } else { 0.0 } })).collect();
    let m: Vec<f64> = if only < 0.5 { vec![1.0; n] } else { let raw: Vec<f64> = px.iter().map(|p| skin_key(*p, hue, width)).collect(); let rf = (r / 4).max(1); box_mean(&raw, w, h, rf) };
    if show >= 0.5 { for (i, p) in f.d.chunks_exact_mut(4).take(n).enumerate() { for j in 0..3 { p[j] = m[i] as f32; } } return Ok(()); }
    let y: Vec<f64> = px.iter().map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).collect();
    // self-guided filter (He 2010): q = mean(a)*Y + mean(b), a = var/(var+eps) is ~0 on low-contrast skin and ~1 on strong edges
    let rg = 2 * r; let eps = 0.02; let my = box_mean(&y, w, h, rg); let myy = box_mean(&y.iter().map(|v| v * v).collect::<Vec<_>>(), w, h, rg);
    let a: Vec<f64> = (0..n).map(|i| { let var = (myy[i] - my[i] * my[i]).max(0.0); var / (var + eps) }).collect(); let b: Vec<f64> = (0..n).map(|i| my[i] - a[i] * my[i]).collect();
    let (ma, mb) = (box_mean(&a, w, h, rg), box_mean(&b, w, h, rg)); let base: Vec<f64> = (0..n).map(|i| ma[i] * y[i] + mb[i]).collect();
    let detail: Vec<f64> = (0..n).map(|i| y[i] - base[i]).collect(); let dm = box_mean(&detail, w, h, ((r as f64 / 6.0).round() as usize).max(1));
    // chroma: mask-normalised Gaussian blur so neighbouring non-skin colour never bleeds in
    let (cb, cr): (Vec<f64>, Vec<f64>) = (0..n).map(|i| ((px[i][2] - y[i]) / 1.8556, (px[i][0] - y[i]) / 1.5748)).unzip();
    let sig = r as f64; let mw = super::boost_detail::blur(&m, w, h, sig);
    let (bcb, bcr) = (super::boost_detail::blur(&(0..n).map(|i| cb[i] * m[i]).collect::<Vec<_>>(), w, h, sig), super::boost_detail::blur(&(0..n).map(|i| cr[i] * m[i]).collect::<Vec<_>>(), w, h, sig));
    for (i, p) in f.d.chunks_exact_mut(4).take(n).enumerate() { if i % 65536 == 0 { crate::guard::check() }
        let k = amount * m[i]; if !(k > 0.0) || !p[0].is_finite() || !p[1].is_finite() || !p[2].is_finite() { continue; }
        let ys = base[i] + texture * (detail[i] - dm[i]); let y1 = y[i] + k * (ys - y[i]);
        let (scb, scr) = if mw[i] > 1e-6 { (bcb[i] / mw[i], bcr[i] / mw[i]) } else { (cb[i], cr[i]) }; let ke = k * even;
        let (cb1, cr1) = (cb[i] + ke * (scb - cb[i]), cr[i] + ke * (scr - cr[i]));
        let (r1, b1) = (y1 + 1.5748 * cr1, y1 + 1.8556 * cb1); let g1 = (y1 - 0.2126 * r1 - 0.0722 * b1) / 0.7152;
        if r1.is_finite() && g1.is_finite() && b1.is_finite() { p[0] = r1 as f32; p[1] = g1 as f32; p[2] = b1 as f32; } }
    Ok(())
}
