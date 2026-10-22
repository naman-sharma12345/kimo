//! Creator looks: one-tap grades built from grade operations (contrast S-curve, white balance, hue pulls, saturation /
//! vibrance, luma-neutral split toning, fades, dye crosstalk, a soft shoulder), display-referred sRGB. Presets: 0 teal &
//! orange, 1 golden hour, 2 moody, 3 clean bright, 4 film, 5 vintage, 6 noir, 7 neon. Every step is monotone on greys, so a
//! grey ramp never inverts, and the shoulder keeps every look inside 0..1. `protect_skin` keeps skin tones (a soft HSV key
//! round hue 22 deg) on their own hue and takes only the look's brightness: q = c + (Y(look) - Y(c)), out = mix(look, q, k).
//! out = c + intensity*(look - c): intensity 0 is an exact identity and the blend is linear. Alpha kept.
use crate::effects::{Frame, Params};

pub const NAMES: [&str; 8] = ["teal_orange", "golden_hour", "moody", "clean_bright", "film", "vintage", "noir", "neon"];

pub fn catalog() -> Vec<super::Entry> { vec![
    ("look", "Creator look in one tap: preset 0 teal&orange 1 golden hour 2 moody 3 clean bright 4 film 5 vintage 6 noir 7 neon; intensity 0..1", super::lv(vec![
        ("preset", 0.0, "0 teal_orange, 1 golden_hour, 2 moody, 3 clean_bright, 4 film, 5 vintage, 6 noir, 7 neon"), ("intensity", 0.8, "0..1 mix of the look (0 identity)"),
        ("protect_skin", 0.6, "0..1 keeps skin tones on their own hue")])),
] }

type C3 = [f32; 3];
fn y709(c: C3) -> f32 { 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2] }
fn hsv(c: C3) -> (f32, f32, f32) { let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2])); let d = mx - mn; if !(d > 0.0) || !(mx > 0.0) { return (0.0, 0.0, mx); }
    let h = if mx == c[0] { 60.0 * ((c[1] - c[2]) / d) } else if mx == c[1] { 60.0 * ((c[2] - c[0]) / d + 2.0) } else { 60.0 * ((c[0] - c[1]) / d + 4.0) }; (h.rem_euclid(360.0), d / mx, mx) }
pub(super) fn from_hsv(h: f32, s: f32, v: f32) -> C3 { let c = v * s; let hp = h.rem_euclid(360.0) / 60.0; let x = c * (1.0 - (hp % 2.0 - 1.0).abs()); let m = v - c;
    let (r, g, b) = match hp as i32 { 0 => (c, x, 0.0), 1 => (x, c, 0.0), 2 => (0.0, c, x), 3 => (0.0, x, c), 4 => (x, 0.0, c), _ => (c, 0.0, x) }; [r + m, g + m, b + m] }
fn sat(c: C3, k: f32) -> C3 { let y = y709(c); c.map(|v| y + k * (v - y)) }
fn vib(c: C3, k: f32) -> C3 { let s = hsv(c).1; sat(c, 1.0 + k * (1.0 - s)) }
/// monotone contrast curve round pivot p (gamma 1+a each side); above 1 it continues with slope 1
fn scurve(v: f32, a: f32, p: f32) -> f32 { let g = 1.0 + a; let v = v.max(0.0); if v < p { if v > 0.0 { p * (v / p).powf(g) } else { 0.0 } } else if v >= 1.0 { v } else { 1.0 - (1.0 - p) * ((1.0 - v) / (1.0 - p)).powf(g) } }
fn curve(c: C3, a: f32, p: f32) -> C3 { c.map(|v| scurve(v, a, p)) }
/// luma-neutral tint t (its Rec.709 luma removed) scaled by w
fn tint(c: C3, t: C3, w: f32) -> C3 { let ty = y709(t); [c[0] + w * (t[0] - ty), c[1] + w * (t[1] - ty), c[2] + w * (t[2] - ty)] }
fn split(c: C3, sh: C3, hi: C3) -> C3 { let y = y709(c).clamp(0.0, 1.0); let c = tint(c, sh, (1.0 - y) * (1.0 - y)); tint(c, hi, y * y) }
fn fade(c: C3, lo: C3, hi: C3) -> C3 { [lo[0] + c[0] * (hi[0] - lo[0]), lo[1] + c[1] * (hi[1] - lo[1]), lo[2] + c[2] * (hi[2] - lo[2])] }
fn bump(h: f32, center: f32, hw: f32) -> f32 { let d = ((h - center + 180.0).rem_euclid(360.0) - 180.0).abs(); if d < hw { 0.5 * (1.0 + (std::f32::consts::PI * d / hw).cos()) } else { 0.0 } }
/// pull hues within hw of `center` toward `target` by amt (cosine weight), luma kept
fn huepull(c: C3, center: f32, hw: f32, target: f32, amt: f32) -> C3 { let (h, s, v) = hsv(c); if !(s > 0.0) { return c; } let w = bump(h, center, hw) * amt; if !(w > 0.0) { return c; }
    let dh = (target - h + 180.0).rem_euclid(360.0) - 180.0; let o = from_hsv(h + w * dh, s, v); let d = y709(c) - y709(o); o.map(|x| x + d) }
fn wb(c: C3, g: C3) -> C3 { let yg = y709(g); [c[0] * g[0] / yg, c[1] * g[1] / yg, c[2] * g[2] / yg] }
fn shoulder(v: f32, k: f32) -> f32 { if v <= k { v } else { k + (1.0 - k) * (1.0 - (-(v - k) / (1.0 - k)).exp()) } }
fn finish(c: C3, k: f32) -> C3 { c.map(|v| shoulder(v, k).clamp(0.0, 1.0)) }
fn cross(c: C3, a: f32) -> C3 { [0usize, 1, 2].map(|j| (1.0 - 2.0 * a) * c[j] + a * c[(j + 1) % 3] + a * c[(j + 2) % 3]) }

/// The look `preset` applied at full strength to one display-referred pixel.
pub fn look(preset: usize, c: C3) -> C3 {
    match preset {
        0 => { let c = curve(c, 0.25, 0.5); let c = huepull(c, 180.0, 85.0, 188.0, 0.6); let c = huepull(c, 25.0, 45.0, 32.0, 0.35); let c = sat(c, 1.12);
            finish(split(c, [-0.05, 0.015, 0.045], [0.05, 0.01, -0.06]), 0.85) }
        1 => { let c = wb(c, [1.10, 1.02, 0.84]); let c = curve(c, -0.08, 0.5); let c = split(c, [0.03, 0.012, -0.03], [0.04, 0.025, -0.05]); let c = fade(c, [0.025, 0.015, 0.0], [1.0; 3]);
            finish(sat(vib(c, 0.25), 1.08), 0.82) }
        2 => { let c = c.map(|v| v * 0.86); let c = huepull(c, 110.0, 60.0, 150.0, 0.3); let c = sat(c, 0.55); let c = curve(c, 0.3, 0.45);
            let c = split(c, [-0.02, 0.0, 0.03], [0.0, 0.005, 0.01]); finish(fade(c, [0.02, 0.03, 0.045], [0.97, 0.97, 0.98]), 0.85) }
        3 => { let c = c.map(|v| v.max(0.0).powf(0.85) * 1.05); let c = sat(vib(c, 0.45), 1.06); finish(wb(c, [0.995, 1.0, 1.02]), 0.9) }
        4 => { let c = curve(cross(c, 0.05), 0.35, 0.42); let c = sat(split(c, [-0.02, 0.015, 0.01], [0.03, 0.012, -0.03]), 0.9); finish(fade(c, [0.04, 0.045, 0.04], [0.95, 0.94, 0.91]), 0.85) }
        5 => { let c = sat(curve(c, -0.1, 0.5), 0.68); let c = split(c, [0.025, -0.015, 0.025], [0.04, 0.025, -0.05]); finish(fade(c, [0.08, 0.065, 0.06], [0.94, 0.92, 0.86]), 0.85) }
        6 => { let y = 0.35 * c[0] + 0.55 * c[1] + 0.10 * c[2]; let v = 0.02 + scurve(y, 0.45, 0.5) * 0.96; finish([v * 0.995, v, v * 1.01], 0.9) }
        _ => { let c = sat(curve(c, 0.2, 0.5), 1.35); finish(split(c, [0.03, -0.05, 0.08], [-0.03, 0.02, 0.03]), 0.85) }
    }
}
fn smooth(e0: f32, e1: f32, x: f32) -> f32 { let u = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0); u * u * (3.0 - 2.0 * u) }
/// soft HSV skin key: hue within 20 deg of 22 (soft to 32), saturation 0.15..0.65 (soft 0.08 / 0.78), value above 0.22 (soft 0.12)
fn skin_key(c: C3) -> f32 { let (h, s, v) = hsv(c); if !(s > 0.0) { return 0.0; } let d = ((h - 22.0 + 180.0).rem_euclid(360.0) - 180.0).abs();
    (1.0 - smooth(20.0, 32.0, d)) * smooth(0.08, 0.15, s) * (1.0 - smooth(0.65, 0.78, s)) * smooth(0.12, 0.22, v) }

pub fn apply(pr: &Params, f: &mut Frame, t: f64) -> Result<(), String> {
    let get = |k: &str, d: f64| pr.get(k).map(|x| x.at(t)).unwrap_or(d);
    let (preset, intensity, protect) = (get("preset", 0.0), get("intensity", 0.8), get("protect_skin", 0.6));
    if !preset.is_finite() || preset.fract() != 0.0 || !(0.0..=(NAMES.len() - 1) as f64).contains(&preset) { return Err(format!("look: preset must be an integer 0..{} ({})", NAMES.len() - 1, NAMES.join(", "))); }
    if !intensity.is_finite() || !(0.0..=1.0).contains(&intensity) { return Err("look: intensity must be in 0..1".into()); }
    if !protect.is_finite() || !(0.0..=1.0).contains(&protect) { return Err("look: protect_skin must be in 0..1".into()); }
    if intensity == 0.0 { return Ok(()); }
    let (p, k, ps) = (preset as usize, intensity as f32, protect as f32);
    for (i, px) in f.d.chunks_exact_mut(4).enumerate() { if i % 65536 == 0 { crate::guard::check() }
        let c = [px[0], px[1], px[2]]; if c.iter().any(|v| !v.is_finite()) { continue; }
        let mut o = look(p, c);
        if ps > 0.0 { let w = ps * skin_key(c); if w > 0.0 { let d = y709(o) - y709(c); for j in 0..3 { o[j] += w * ((c[j] + d).clamp(0.0, 1.0) - o[j]); } } }
        for j in 0..3 { px[j] = c[j] + k * (o[j] - c[j]); } }
    Ok(())
}
