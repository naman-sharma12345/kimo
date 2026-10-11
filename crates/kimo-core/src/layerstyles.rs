//! Layer styles beyond drop_shadow/outline/outer_glow: inner shadow, bevel & emboss, stroke (inside/center/outside).
//! Effect kinds `ls_*`; params are animatable Props. Alpha-driven, so use on text, shapes, stickers, precomps.
use crate::effects::{box_blur3, p, Frame, Params};
use std::collections::BTreeMap;

pub const STYLES: &[(&str, &str, &[(&str, f64)])] = &[
    ("ls_inner_shadow", "Inner shadow: dx,dy px, blur px, choke px, opacity 0..1, color r,g,b", &[("dx", 4.0), ("dy", 4.0), ("blur", 6.0), ("choke", 0.0), ("opacity", 0.6), ("r", 0.0), ("g", 0.0), ("b", 0.0)]),
    ("ls_bevel", "Bevel & emboss: depth 0..4, size px (soften radius), angle deg (light direction, 0 = from right, 90 = from below-screen top is -90), highlight/shadow opacity 0..1", &[("depth", 1.5), ("size", 5.0), ("angle", -45.0), ("highlight", 0.7), ("shadow", 0.6)]),
    ("ls_stroke", "Stroke around alpha: width px, strings.position inside|center|outside, opacity, color r,g,b", &[("width", 4.0), ("opacity", 1.0), ("r", 1.0), ("g", 1.0), ("b", 1.0)]),
];

fn over(o: &mut [f32], c: [f32; 3], a: f32) { let na = a + o[3] * (1.0 - a); if na <= 0.0 { return; } for j in 0..3 { o[j] = (c[j] * a + o[j] * o[3] * (1.0 - a)) / na; } o[3] = na; }

/// Separable square max (grow) or min (shrink) of the alpha channel.
fn morph(a: &[f32], w: usize, h: usize, r: usize, grow: bool) -> Vec<f32> {
    let r = r.min(64); let f = |x: f32, y: f32| if grow { x.max(y) } else { x.min(y) };
    let init = if grow { 0.0 } else { 1.0 }; let mut t = vec![0f32; w * h]; let mut o = vec![0f32; w * h];
    for y in 0..h { for x in 0..w { let mut v = init; for k in x.saturating_sub(r)..=(x + r).min(w - 1) { v = f(v, a[y * w + k]); } t[y * w + x] = v; } }
    for y in 0..h { for x in 0..w { let mut v = init; for k in y.saturating_sub(r)..=(y + r).min(h - 1) { v = f(v, t[k * w + x]); } o[y * w + x] = v; } }
    o
}

pub fn apply(kind: &str, params: &Params, strings: &BTreeMap<String, String>, f: &mut Frame, t: f64) -> Result<(), String> {
    let (w, h) = (f.w, f.h); let sc = h as f32 / 720.0; let alpha: Vec<f32> = (0..w * h).map(|i| f.d[i * 4 + 3]).collect();
    match kind {
        "ls_inner_shadow" => {
            let (dx, dy, bl, ch, op) = ((p(params, "dx", 4.0, t) * sc) as isize, (p(params, "dy", 4.0, t) * sc) as isize, (p(params, "blur", 6.0, t) * sc).max(0.0) as usize, (p(params, "choke", 0.0, t) * sc).max(0.0) as usize, p(params, "opacity", 0.6, t).clamp(0.0, 1.0));
            let c = [p(params, "r", 0.0, t), p(params, "g", 0.0, t), p(params, "b", 0.0, t)];
            let src = if ch > 0 { morph(&alpha, w, h, ch, false) } else { alpha.clone() };
            let mut sh = Frame::new(w, h); let (wi, hi) = (w as isize, h as isize);
            for y in 0..hi { for x in 0..wi { let (sx, sy) = (x - dx, y - dy); let a = if sx >= 0 && sy >= 0 && sx < wi && sy < hi { src[(sy * wi + sx) as usize] } else { 0.0 }; sh.d[((y * wi + x) * 4 + 3) as usize] = 1.0 - a; } }
            box_blur3(&mut sh, bl);
            for i in 0..w * h { let a = sh.d[i * 4 + 3] * op * alpha[i]; if a > 0.0 { let o = &mut f.d[i * 4..i * 4 + 4]; let oa = o[3]; for j in 0..3 { o[j] = o[j] * (1.0 - a) + c[j] * a; } o[3] = oa; } }
        }
        "ls_bevel" => {
            let (depth, size, ang, hi, sh) = (p(params, "depth", 1.5, t), (p(params, "size", 5.0, t) * sc).max(1.0) as usize, p(params, "angle", -45.0, t).to_radians(), p(params, "highlight", 0.7, t), p(params, "shadow", 0.6, t));
            let mut hm = Frame::new(w, h); for i in 0..w * h { hm.d[i * 4 + 3] = alpha[i]; } box_blur3(&mut hm, size); let hm: Vec<f32> = (0..w * h).map(|i| hm.d[i * 4 + 3]).collect();
            let (lx, ly) = (ang.cos(), ang.sin());
            for y in 1..h.saturating_sub(1) { for x in 1..w.saturating_sub(1) { let i = y * w + x; if alpha[i] <= 0.0 { continue; }
                let gx = (hm[i + 1] - hm[i - 1]) * 0.5; let gy = (hm[i + w] - hm[i - w]) * 0.5; // height gradient (height rises inward)
                let s = (-(gx * lx + gy * ly) * depth * size as f32 * 0.5).clamp(-1.0, 1.0); // facing the light => positive
                let o = &mut f.d[i * 4..i * 4 + 4]; let oa = o[3];
                if s > 0.0 { over(o, [1.0; 3], s * hi * alpha[i]); } else { over(o, [0.0; 3], -s * sh * alpha[i]); } o[3] = oa; } }
        }
        "ls_stroke" => {
            let (wd, op) = ((p(params, "width", 4.0, t) * sc).max(1.0) as usize, p(params, "opacity", 1.0, t).clamp(0.0, 1.0)); let c = [p(params, "r", 1.0, t), p(params, "g", 1.0, t), p(params, "b", 1.0, t)];
            let pos = strings.get("position").map(|s| s.as_str()).unwrap_or("outside");
            let (outer, inner) = match pos { "inside" => (alpha.clone(), morph(&alpha, w, h, wd, false)), "center" => (morph(&alpha, w, h, (wd + 1) / 2, true), morph(&alpha, w, h, wd / 2, false)), "outside" => (morph(&alpha, w, h, wd, true), alpha.clone()), o => return Err(format!("ls_stroke position must be inside|center|outside, got '{o}'")) };
            for i in 0..w * h { let a = (outer[i] - inner[i]).max(0.0) * op; if a > 0.0 { over(&mut f.d[i * 4..i * 4 + 4], c, a); } }
        }
        o => return Err(format!("unknown layer style '{o}'")),
    }
    Ok(())
}
