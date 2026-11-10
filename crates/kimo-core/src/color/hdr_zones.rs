//! HDR zones (Resolve HDR palette style), on display-referred sRGB pixels decoded to linear light. Zones are luminance ranges in
//! stops s = log2(Y/0.18) around 18 % grey: dark and shadow act below their boundary, light, highlight and specular above it, with a
//! smoothstep falloff `falloff` stops wide centred on the boundary (zones nest: deep shadows get dark + shadow). Each zone has an
//! exposure in stops and a saturation; `global` moves every tone. The tone map s -> s + sum(e_i*w_i(s)) is sampled on a 1/64-stop
//! grid and kept non-decreasing: a pull against a zone's direction too strong for its falloff widens that falloff away from the zone
//! (slope >= 0.1; the zone itself keeps its full exposure), and a running maximum guards what nested zones add up to. Applied as one
//! luminance gain per pixel (linear RGB ratios, i.e. hue, kept), then saturation 1 + sum((sat_i - 1)*w_i) around the new luminance,
//! then re-encoded (signed, unclipped). All exposures 0 and saturations 1 is an exact identity. Display-referred; alpha kept.
use crate::effects::{Frame, Params};

const ZONES: [(&str, f64, bool); 5] = [("dark", -3.5, false), ("shadow", -1.5, false), ("light", 0.5, true), ("highlight", 1.6, true), ("specular", 2.2, true)];
pub fn catalog() -> Vec<super::Entry> { vec![
    ("hdr_zones", "HDR zones (Resolve HDR palette): exposure in stops + saturation per tone zone (dark..specular), soft falloff, hue kept", super::lv(vec![
        ("global", 0.0, "stops, -4..4, every tone"), ("dark", 0.0, "stops, -4..4, below dark_range"), ("shadow", 0.0, "stops, -4..4, below shadow_range"),
        ("light", 0.0, "stops, -4..4, above light_range"), ("highlight", 0.0, "stops, -4..4, above highlight_range"), ("specular", 0.0, "stops, -4..4, above specular_range"),
        ("dark_range", -3.5, "zone boundary, stops from 18% grey, -8..8"), ("shadow_range", -1.5, "zone boundary, stops, -8..8"), ("light_range", 0.5, "zone boundary, stops, -8..8"),
        ("highlight_range", 1.6, "zone boundary, stops, -8..8"), ("specular_range", 2.2, "zone boundary, stops, -8..8"), ("falloff", 1.0, "zone edge softness in stops, 0.05..4"),
        ("dark_sat", 1.0, "saturation in the dark zone, 0..2"), ("shadow_sat", 1.0, "saturation in the shadow zone, 0..2"), ("light_sat", 1.0, "saturation in the light zone, 0..2"),
        ("highlight_sat", 1.0, "saturation in the highlight zone, 0..2"), ("specular_sat", 1.0, "saturation in the specular zone, 0..2")])),
] }

fn get(pr: &Params, k: &str, d: f64, t: f64) -> f64 { pr.get(k).map(|x| x.at(t)).unwrap_or(d) }
fn dec(v: f64) -> f64 { let a = v.abs(); let l = if a <= 0.04045 { a / 12.92 } else { ((a + 0.055) / 1.055).powf(2.4) }; l.copysign(v) }
fn enc(l: f64) -> f64 { let a = l.abs(); let v = if a <= 0.0031308 { a * 12.92 } else { 1.055 * a.powf(1.0 / 2.4) - 0.055 }; v.copysign(l) }
const S0: f64 = -20.0; const S1: f64 = 12.0; const STEP: f64 = 1.0 / 64.0;

pub fn apply(pr: &Params, f: &mut Frame, t: f64) -> Result<(), String> {
    let n = f.w * f.h; if f.d.len() < n * 4 { return Err("hdr_zones: frame buffer too short".into()); }
    let global = get(pr, "global", 0.0, t); let fall = get(pr, "falloff", 1.0, t);
    if !global.is_finite() || !(-4.0..=4.0).contains(&global) { return Err(format!("hdr_zones: global must be in -4..4 stops, got {global}")); }
    if !fall.is_finite() || !(0.05..=4.0).contains(&fall) { return Err(format!("hdr_zones: falloff must be in 0.05..4 stops, got {fall}")); }
    let mut z = Vec::new(); let mut ident = global == 0.0;
    for (name, b0, above) in ZONES {
        let (e, b, s) = (get(pr, name, 0.0, t), get(pr, &format!("{name}_range"), b0, t), get(pr, &format!("{name}_sat"), 1.0, t));
        if !e.is_finite() || !(-4.0..=4.0).contains(&e) { return Err(format!("hdr_zones: {name} must be in -4..4 stops, got {e}")); }
        if !b.is_finite() || !(-8.0..=8.0).contains(&b) { return Err(format!("hdr_zones: {name}_range must be in -8..8 stops, got {b}")); }
        if !s.is_finite() || !(0.0..=2.0).contains(&s) { return Err(format!("hdr_zones: {name}_sat must be in 0..2, got {s}")); }
        if e != 0.0 || s != 1.0 { ident = false; } z.push((e, b, s, above)); }
    if ident || n == 0 { return Ok(()); }
    // A pull against the zone's direction (darkening an upper zone, lifting a lower one) steepens nothing but flattens the curve across the
    // falloff: smoothstep's peak slope is 1.5/width, so a width under 1.5*|e|/0.9 would invert tones. Such a zone's falloff is widened
    // away from the zone (the zone itself still gets its full exposure from b +/- falloff/2 on), keeping the curve's slope >= 0.1.
    let zw: Vec<(f64, f64)> = z.iter().map(|&(e, b, _, above)| { let f = if (above && e < 0.0) || (!above && e > 0.0) { fall.max(1.5 * e.abs() / 0.9) } else { fall };
        (if above { b + fall / 2.0 - f / 2.0 } else { b - fall / 2.0 + f / 2.0 }, f) }).collect();
    let wgt = |s: f64, i: usize, above: bool| { let (c, f) = zw[i]; let u = ((s - c) / f + 0.5).clamp(0.0, 1.0); let w = u * u * (3.0 - 2.0 * u); if above { w } else { 1.0 - w } };
    // monotone tone map on a fine grid of input stops
    let m = ((S1 - S0) / STEP) as usize + 1; let mut out = Vec::with_capacity(m); let mut sat = Vec::with_capacity(m); let mut hi = f64::NEG_INFINITY;
    for k in 0..m { let s = S0 + k as f64 * STEP; let (mut g, mut st) = (global, 1.0); for (i, &(e, _, sv, above)) in z.iter().enumerate() { let w = wgt(s, i, above); g += e * w; st += (sv - 1.0) * w; }
        hi = hi.max(s + g); out.push(hi); sat.push(st.max(0.0)); }
    let look = |s: f64| { let x = ((s.clamp(S0, S1) - S0) / STEP).min((m - 1) as f64); let k = (x.floor() as usize).min(m - 2); let fr = x - k as f64;
        let o = out[k] + (out[k + 1] - out[k]) * fr + (s - s.clamp(S0, S1)); (o, sat[k] + (sat[k + 1] - sat[k]) * fr) };
    for (i, p) in f.d.chunks_exact_mut(4).take(n).enumerate() { if i % 65536 == 0 { crate::guard::check() }
        if !p[0].is_finite() || !p[1].is_finite() || !p[2].is_finite() { continue; }
        let l = [dec(p[0] as f64), dec(p[1] as f64), dec(p[2] as f64)]; let y = 0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2];
        let s = if y > 1e-9 { (y / 0.18).log2() } else { S0 }; let (o, st) = look(s); let gain = 2f64.powf(o - s); let y1 = y * gain;
        let q = l.map(|v| { let v = v * gain; y1 + st * (v - y1) }); let e = q.map(enc);
        if e.iter().all(|v| v.is_finite()) { for j in 0..3 { p[j] = e[j] as f32; } } }
    Ok(())
}
