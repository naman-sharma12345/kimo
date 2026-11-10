//! Dehaze (Resolve FX Dehaze) by the dark channel prior (He, Sun, Tang 2009), display-referred. Haze model I = J*t + A*(1-t).
//! dark(x) = min over a (2r+1)^2 patch of min(R,G,B); the airlight A is the mean colour of the 0.1% haziest pixels (highest
//! dark channel), each channel raised to at least 0.8x its brightest channel (haze is near-neutral) and 0.05, or set by hand with
//! air_r/air_g/air_b (-1 = auto; per frame, so pin it for a whole shot when it flickers). Transmission t = 1 - amount * dark(I/A),
//! refined edge-aware by a guided filter on luma (radius 4r, eps 1e-3) and floored at 0.1; J = (I - A)/t + A, alpha kept.
//! r = radius * height / 720 px (resolution independent). amount 0..1 (0 = exact identity), radius 1..60.
use crate::effects::{Frame, Params};

pub fn catalog() -> Vec<super::Entry> { vec![
    ("dehaze", "Dehaze (Resolve FX): removes atmospheric haze by the dark channel prior; contrast and colour come back. amount 0..1, 0 identity", super::lv(vec![
        ("amount", 0.6, "0..1 share of the haze removed"), ("radius", 10.0, "patch half-width in px at 720p height, 1..60"),
        ("air_r", -1.0, "airlight red 0.05..1; -1 auto"), ("air_g", -1.0, "airlight green 0.05..1; -1 auto"), ("air_b", -1.0, "airlight blue 0.05..1; -1 auto")])),
] }

fn get(pr: &Params, k: &str, d: f64, t: f64) -> f64 { pr.get(k).map(|x| x.at(t)).unwrap_or(d) }

/// Sliding minimum of half-width r along rows or columns, clamped edges, O(n) per line (van Herk / Gil-Werman).
fn min_pass(src: &[f64], w: usize, h: usize, r: usize, horiz: bool) -> Vec<f64> {
    let (len, lines) = if horiz { (w, h) } else { (h, w) }; let at = |line: usize, i: usize| if horiz { line * w + i } else { i * w + line };
    let k = 2 * r + 1; let n = len + 2 * r; let mut out = vec![0.0; src.len()]; let (mut pad, mut g, mut s) = (vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    for line in 0..lines { if line % 256 == 0 { crate::guard::check() }
        for p in 0..n { pad[p] = src[at(line, (p as isize - r as isize).clamp(0, len as isize - 1) as usize)]; }
        for p in 0..n { g[p] = if p % k == 0 { pad[p] } else { g[p - 1].min(pad[p]) }; }
        for p in (0..n).rev() { s[p] = if p % k == k - 1 || p == n - 1 { pad[p] } else { s[p + 1].min(pad[p]) }; }
        for i in 0..len { out[at(line, i)] = s[i].min(g[i + k - 1]); } }
    out
}
fn min_filter(src: &[f64], w: usize, h: usize, r: usize) -> Vec<f64> { let a = min_pass(src, w, h, r, true); min_pass(&a, w, h, r, false) }
fn box_mean(src: &[f64], w: usize, h: usize, r: usize) -> Vec<f64> { let mut a = vec![0.0; src.len()]; let mut b = vec![0.0; src.len()]; super::boost_detail::box_pass(src, &mut a, w, h, r, true); super::boost_detail::box_pass(&a, &mut b, w, h, r, false); b }

pub fn apply(pr: &Params, f: &mut Frame, t: f64) -> Result<(), String> {
    let n = f.w * f.h; if f.d.len() < n * 4 { return Err("dehaze: frame buffer too short".into()); }
    let amount = get(pr, "amount", 0.6, t); let radius = get(pr, "radius", 10.0, t);
    if !amount.is_finite() || !(0.0..=1.0).contains(&amount) { return Err(format!("dehaze: amount must be in 0..1, got {amount}")); }
    if !radius.is_finite() || !(1.0..=60.0).contains(&radius) { return Err(format!("dehaze: radius must be in 1..60, got {radius}")); }
    let mut air = [0.0; 3];
    for (j, k) in ["air_r", "air_g", "air_b"].iter().enumerate() { let v = get(pr, k, -1.0, t); if !v.is_finite() || !(v == -1.0 || (0.05..=1.0).contains(&v)) { return Err(format!("dehaze: {k} must be -1 (auto) or 0.05..1, got {v}")); } air[j] = v; }
    if amount == 0.0 || n == 0 { return Ok(()); }
    let r = ((radius * f.h as f64 / 720.0).round() as usize).max(1);
    let px: Vec<[f64; 3]> = f.d.chunks_exact(4).take(n).map(|p| [0, 1, 2].map(|j| { let v = p[j] as f64; if v.is_finite() { v } else { 0.0 } })).collect();
    let dark = min_filter(&px.iter().map(|p| p[0].min(p[1]).min(p[2])).collect::<Vec<_>>(), f.w, f.h, r);
    let mut a = [0.0; 3];
    if air.iter().all(|v| *v > 0.0) { a = air; } else {
        let m = ((n as f64 * 0.001).round() as usize).max(1); let mut idx: Vec<usize> = (0..n).collect();
        idx.select_nth_unstable_by(n - m, |x, y| dark[*x].total_cmp(&dark[*y]));
        for &i in &idx[n - m..] { for j in 0..3 { a[j] += px[i][j] / m as f64; } }
        let top = a.iter().cloned().fold(0.0, f64::max); for j in 0..3 { a[j] = a[j].max(0.8 * top).max(0.05); if air[j] > 0.0 { a[j] = air[j]; } } }
    let tn: Vec<f64> = min_filter(&px.iter().map(|p| (p[0] / a[0]).min(p[1] / a[1]).min(p[2] / a[2])).collect::<Vec<_>>(), f.w, f.h, r).into_iter().map(|d| 1.0 - amount * d).collect();
    // guided filter (He 2010) with luma as the guide: edge-aware transmission, no block halos
    let g: Vec<f64> = px.iter().map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).collect(); let rg = (4 * r).max(2); let eps = 1e-3;
    let (mi, mp) = (box_mean(&g, f.w, f.h, rg), box_mean(&tn, f.w, f.h, rg));
    let (mip, mii) = (box_mean(&g.iter().zip(&tn).map(|(x, y)| x * y).collect::<Vec<_>>(), f.w, f.h, rg), box_mean(&g.iter().map(|x| x * x).collect::<Vec<_>>(), f.w, f.h, rg));
    let ca: Vec<f64> = (0..n).map(|i| (mip[i] - mi[i] * mp[i]) / (mii[i] - mi[i] * mi[i] + eps)).collect(); let cb: Vec<f64> = (0..n).map(|i| mp[i] - ca[i] * mi[i]).collect();
    let (ma, mb) = (box_mean(&ca, f.w, f.h, rg), box_mean(&cb, f.w, f.h, rg));
    for (i, p) in f.d.chunks_exact_mut(4).take(n).enumerate() { if i % 65536 == 0 { crate::guard::check() }
        let tt = (ma[i] * g[i] + mb[i]).clamp(0.1, 1.0); if !tt.is_finite() { continue; }
        for j in 0..3 { if !p[j].is_finite() { continue; } let v = (p[j] as f64 - a[j]) / tt + a[j]; if v.is_finite() { p[j] = v as f32; } } }
    Ok(())
}
