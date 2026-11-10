//! Color Warper (Resolve's hue-saturation warper), display-referred. Up to four pins: pin k picks a colour at HSV hue
//! p{k}_hue / saturation p{k}_sat and moves it to p{k}_to_hue / p{k}_to_sat; its reach is a Gaussian in hue (p{k}_hue_range,
//! deg) and saturation (p{k}_sat_range). A pixel moves in polar hue-sat by the weighted pin moves (hue along the short arc),
//! normalised by max(1, sum of weights) so overlapping pins share one move instead of adding up. Moves fade out below
//! saturation 0.1 (smoothstep), so greys, black and white never move. keep_luma 1 rescales the result to the pixel's
//! Rec.709 luma (and, if that would pass the input's peak, desaturates toward that luma: hue and luma kept). A pin whose
//! target equals its source is off; no pin moved or amount 0 is an exact identity. show_matte views the pins' reach. Alpha kept.
use crate::effects::{Frame, Params};

pub fn catalog() -> Vec<super::Entry> {
    let mut c: Vec<(&'static str, f64, &'static str)> = vec![("amount", 1.0, "0..1 strength of every pin move (0 identity)"), ("keep_luma", 1.0, "1 = moved colours keep their brightness"), ("show_matte", 0.0, "1 = view where the pins reach")];
    const K: [[&str; 6]; 4] = [["p1_hue", "p1_sat", "p1_to_hue", "p1_to_sat", "p1_hue_range", "p1_sat_range"], ["p2_hue", "p2_sat", "p2_to_hue", "p2_to_sat", "p2_hue_range", "p2_sat_range"],
        ["p3_hue", "p3_sat", "p3_to_hue", "p3_to_sat", "p3_hue_range", "p3_sat_range"], ["p4_hue", "p4_sat", "p4_to_hue", "p4_to_sat", "p4_hue_range", "p4_sat_range"]];
    for (i, k) in K.iter().enumerate() { let h = (i * 90) as f64;
        c.extend_from_slice(&[(k[0], h, "pin source hue, deg (HSV: red 0, yellow 60, green 120, blue 240)"), (k[1], 0.6, "pin source saturation 0..1"), (k[2], h, "target hue, deg"),
            (k[3], 0.6, "target saturation 0..1"), (k[4], 25.0, "hue reach, deg 1..180"), (k[5], 0.35, "saturation reach 0.02..1")]); }
    vec![("color_warper", "Color Warper (Resolve hue-sat): 4 pins move a colour to another hue/saturation with soft reach; greys and luma kept", super::lv(c))]
}

fn hsv(c: [f32; 3]) -> (f32, f32, f32) { let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2])); let d = mx - mn; if !(d > 0.0) || !(mx > 0.0) { return (0.0, 0.0, mx); }
    let h = if mx == c[0] { 60.0 * ((c[1] - c[2]) / d) } else if mx == c[1] { 60.0 * ((c[2] - c[0]) / d + 2.0) } else { 60.0 * ((c[0] - c[1]) / d + 4.0) }; (h.rem_euclid(360.0), d / mx, mx) }
fn y709(c: [f32; 3]) -> f32 { 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2] }
fn arc(a: f32) -> f32 { (a + 180.0).rem_euclid(360.0) - 180.0 }
struct Pin { h: f32, s: f32, dh: f32, ds: f32, hr: f32, sr: f32 }

pub fn apply(pr: &Params, f: &mut Frame, t: f64) -> Result<(), String> {
    let get = |k: &str, d: f64| pr.get(k).map(|x| x.at(t)).unwrap_or(d);
    let chk = |k: &str, d: f64, lo: f64, hi: f64| -> Result<f64, String> { let v = get(k, d); if !v.is_finite() || v < lo || v > hi { Err(format!("color_warper: {k} must be in {lo}..{hi}")) } else { Ok(v) } };
    let amount = chk("amount", 1.0, 0.0, 1.0)? as f32; let keep = chk("keep_luma", 1.0, 0.0, 1.0)? >= 0.5; let show = chk("show_matte", 0.0, 0.0, 1.0)? >= 0.5;
    let mut pins = Vec::new();
    for k in 1..=4 { let d = ((k - 1) * 90) as f64; let n = |s: &str| format!("p{k}_{s}");
        let h = chk(&n("hue"), d, -720.0, 720.0)?; let s = chk(&n("sat"), 0.6, 0.0, 1.0)?; let th = chk(&n("to_hue"), d, -720.0, 720.0)?; let ts = chk(&n("to_sat"), 0.6, 0.0, 1.0)?;
        let hr = chk(&n("hue_range"), 25.0, 1.0, 180.0)?; let sr = chk(&n("sat_range"), 0.35, 0.02, 1.0)?;
        let (dh, ds) = (arc((th - h) as f32), (ts - s) as f32); if dh == 0.0 && ds == 0.0 { continue; }
        pins.push(Pin { h: h.rem_euclid(360.0) as f32, s: s as f32, dh, ds, hr: hr as f32, sr: sr as f32 }); }
    if pins.is_empty() || (amount == 0.0 && !show) { if show { for px in f.d.chunks_exact_mut(4) { px[0] = 0.0; px[1] = 0.0; px[2] = 0.0; } } return Ok(()); }
    for (i, px) in f.d.chunks_exact_mut(4).enumerate() { if i % 65536 == 0 { crate::guard::check() }
        let c = [px[0], px[1], px[2]]; if c.iter().any(|v| !v.is_finite()) { continue; }
        let (h, s, v) = hsv(c); let u = (s / 0.1).clamp(0.0, 1.0); let g = u * u * (3.0 - 2.0 * u);
        let (mut sw, mut dh, mut ds) = (0.0f32, 0.0f32, 0.0f32);
        if g > 0.0 { for p in &pins { let hd = arc(h - p.h) / p.hr; let sd = (s - p.s) / p.sr; let w = (-0.5 * (hd * hd + sd * sd)).exp(); sw += w; dh += w * p.dh; ds += w * p.ds; } }
        if show { let m = sw.min(1.0) * g; px[0] = m; px[1] = m; px[2] = m; continue; }
        if !(sw > 0.0) || !(g > 0.0) { continue; }
        let k = amount * g / sw.max(1.0); let mut o = super::looks::from_hsv(h + k * dh, (s + k * ds).clamp(0.0, 1.0), v);
        if keep { let (y0, y1) = (y709(c), y709(o)); if y1 > 0.0 { for x in o.iter_mut() { *x *= y0 / y1; } }
            let lim = c[0].max(c[1]).max(c[2]).max(1.0); let mx = o[0].max(o[1]).max(o[2]); if mx > lim { let y = y709(o); let q = (lim - y) / (mx - y); for x in o.iter_mut() { *x = y + q * (*x - y); } } }
        px[0] = o[0]; px[1] = o[1]; px[2] = o[2]; }
    Ok(())
}
