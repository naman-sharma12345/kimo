//! Resolve primaries-bar Color Boost and Midtone Detail as ordinary grade effects (clip effects, grade nodes, under any mask).
//! color_boost (vibrance): in Rec.709 Y'CbCr, chroma *= max(0, 1 + amount*(1-min(S,1))^3) with S = 2*|(Cb,Cr)|; luma is kept,
//! so weak colour moves and already-strong colour stays. Pointwise. amount -1..2 (negative mutes weak colour); 0 = exact identity.
//! midtone_detail (local contrast): Y' = Y + amount * w(Y) * (Y - blur(Y)), w = 4Y(1-Y) clamped >= 0 so shadows and highlights
//! are protected; blur is a 3-pass box approximation of a Gaussian with sigma = radius * height / 720 (resolution independent).
//! The luma change is added equally to R, G and B, so Cb/Cr are kept. amount -2..2 (negative softens), radius 0.5..200.
use crate::effects::{Frame, Params};

pub fn catalog() -> Vec<super::Entry> { vec![
    ("color_boost", "Color Boost (vibrance, Resolve primaries bar): boosts weak colour only, strong colour and luma kept. amount -1..2, 0 identity", super::lv(vec![("amount", 0.5, "-1..2; negative mutes weak colour")])),
    ("midtone_detail", "Midtone Detail (local contrast, Resolve primaries bar): Y += amount*4Y(1-Y)*(Y-blur Y), chroma kept. amount -2..2, radius 0.5..200", super::lv(vec![("amount", 0.5, "-2..2; negative softens"), ("radius", 20.0, "blur sigma in px at 720p height, 0.5..200")])),
] }

fn get(pr: &Params, k: &str, d: f64, t: f64) -> f64 { pr.get(k).map(|x| x.at(t)).unwrap_or(d) }
fn luma(p: &[f32]) -> f64 { 0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64 }

/// One box-blur pass of half-width r along rows (stride 1) or columns, clamped edges, O(n).
fn box_pass(src: &[f64], dst: &mut [f64], w: usize, h: usize, r: usize, horiz: bool) {
    let (len, lines) = if horiz { (w, h) } else { (h, w) }; let at = |line: usize, i: usize| if horiz { line * w + i } else { i * w + line };
    let norm = 1.0 / (2 * r + 1) as f64;
    for line in 0..lines { if line % 256 == 0 { crate::guard::check() }
        let mut acc = 0.0; for k in 0..=2 * r { let i = (k as isize - r as isize).clamp(0, len as isize - 1) as usize; acc += src[at(line, i)]; }
        for i in 0..len { dst[at(line, i)] = acc * norm;
            let out = (i as isize - r as isize).clamp(0, len as isize - 1) as usize; let inn = (i + r + 1).min(len - 1); acc += src[at(line, inn)] - src[at(line, out)]; } }
}

/// Gaussian-like blur of a plane via three box passes per axis (box widths from the standard boxes-for-gauss fit).
fn blur(y: &[f64], w: usize, h: usize, sigma: f64) -> Vec<f64> {
    let n = 3.0; let wi = ((12.0 * sigma * sigma / n) + 1.0).sqrt().floor(); let wl = if (wi as i64) % 2 == 0 { wi - 1.0 } else { wi }; let wu = wl + 2.0;
    let m = ((12.0 * sigma * sigma - n * wl * wl - 4.0 * n * wl - 3.0 * n) / (-4.0 * wl - 4.0)).round();
    let radii: Vec<usize> = (0..3).map(|i| ((if (i as f64) < m { wl } else { wu }).max(1.0) as usize - 1) / 2).collect();
    let mut a = y.to_vec(); let mut b = vec![0.0; y.len()];
    for r in radii { if r == 0 { continue; } box_pass(&a, &mut b, w, h, r, true); box_pass(&b, &mut a, w, h, r, false); }
    a
}

pub fn apply(kind: &str, pr: &Params, f: &mut Frame, t: f64) -> Result<(), String> {
    let n = f.w * f.h; if f.d.len() < n * 4 { return Err(format!("{kind}: frame buffer too short")); }
    match kind {
        "color_boost" => {
            let a = get(pr, "amount", 0.5, t); if !a.is_finite() || !(-1.0..=2.0).contains(&a) { return Err(format!("color_boost: amount must be in -1..2, got {a}")); }
            if a == 0.0 { return Ok(()); }
            for (i, p) in f.d.chunks_exact_mut(4).take(n).enumerate() { if i % 65536 == 0 { crate::guard::check() }
                let (r, g, b) = (p[0] as f64, p[1] as f64, p[2] as f64); let y = 0.2126 * r + 0.7152 * g + 0.0722 * b; let (cb, cr) = ((b - y) / 1.8556, (r - y) / 1.5748);
                let s = (2.0 * (cb * cb + cr * cr).sqrt()).min(1.0); let k = (1.0 + a * (1.0 - s).powi(3)).max(0.0); if !k.is_finite() { continue; }
                let (r1, b1) = (y + 1.5748 * cr * k, y + 1.8556 * cb * k); let g1 = (y - 0.2126 * r1 - 0.0722 * b1) / 0.7152;
                if r1.is_finite() && g1.is_finite() && b1.is_finite() { p[0] = r1 as f32; p[1] = g1 as f32; p[2] = b1 as f32; } }
            Ok(())
        }
        "midtone_detail" => {
            let a = get(pr, "amount", 0.5, t); let rad = get(pr, "radius", 20.0, t);
            if !a.is_finite() || !(-2.0..=2.0).contains(&a) { return Err(format!("midtone_detail: amount must be in -2..2, got {a}")); }
            if !rad.is_finite() || !(0.5..=200.0).contains(&rad) { return Err(format!("midtone_detail: radius must be in 0.5..200, got {rad}")); }
            if a == 0.0 || n == 0 { return Ok(()); }
            let y: Vec<f64> = f.d.chunks_exact(4).take(n).map(|p| { let v = luma(p); if v.is_finite() { v } else { 0.0 } }).collect();
            let sigma = (rad * f.h as f64 / 720.0).max(0.3); let bl = blur(&y, f.w, f.h, sigma);
            for (i, p) in f.d.chunks_exact_mut(4).take(n).enumerate() { if i % 65536 == 0 { crate::guard::check() }
                let w = (4.0 * y[i] * (1.0 - y[i])).max(0.0); let d = (a * w * (y[i] - bl[i])) as f32; if d.is_finite() { for j in 0..3 { p[j] += d; } } }
            Ok(())
        }
        other => Err(format!("unknown effect '{other}'")),
    }
}
