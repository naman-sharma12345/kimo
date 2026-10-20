//! Spatial motion paths: a clip travels along a bezier path (After Effects position path / Resolve motion path) with
//! optional auto-orient (rotate to the tangent) and arc-length (constant speed) parameterisation.
//! Progress is the clip property "path" (0..1), keyframable, with easing, expressions and bezier handles like any other property.
use serde::{Deserialize, Serialize};
use crate::masks::PathPt;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MotionPath {
    /// Bezier path, normalized canvas coords 0..1 (tangents are offsets).
    pub points: Vec<PathPt>,
    #[serde(default)] pub closed: bool,
    /// Rotate the clip to follow the path tangent.
    #[serde(default)] pub auto_orient: bool,
    /// Extra rotation in degrees added to the tangent angle (so a sprite drawn facing up can face forward).
    #[serde(default)] pub orient_offset: f64,
    /// true = progress is proportional to distance travelled (constant speed); false = proportional to bezier parameter.
    #[serde(default = "d_true")] pub constant_speed: bool,
    /// 0..1: how strongly orientation is smoothed over a look-ahead window (avoids snapping at sharp corners).
    #[serde(default)] pub orient_smooth: f64,
}
fn d_true() -> bool { true }

/// (dx, dy) in px relative to the canvas centre, and the tangent angle in degrees (0 = moving right, +y down).
pub fn sample(mp: &MotionPath, u: f64, w: usize, h: usize) -> (f64, f64, f64) {
    let poly = crate::masks::flatten_pub(&mp.points, mp.closed, w, h); let n = poly.len(); if n < 2 { return (0.0, 0.0, 0.0); }
    let u = if mp.closed { u.rem_euclid(1.0) } else { u.clamp(0.0, 1.0) };
    let m = if mp.closed { n } else { n - 1 }; let mut cum = Vec::with_capacity(m + 1); cum.push(0.0f64);
    for i in 0..m { let (a, b) = (poly[i], poly[(i + 1) % n]); cum.push(cum[i] + ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()); }
    let total = cum[m].max(1e-9);
    let at = |s: f64| -> (f64, f64, usize) { let s = if mp.closed { s.rem_euclid(total) } else { s.clamp(0.0, total) }; let i = match cum.binary_search_by(|v| v.partial_cmp(&s).unwrap()) { Ok(i) => i.min(m - 1), Err(i) => i.saturating_sub(1).min(m - 1) };
        let (a, b) = (poly[i], poly[(i + 1) % n]); let seg = (cum[i + 1] - cum[i]).max(1e-9); let k = (s - cum[i]) / seg; (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k, i) };
    let s = if mp.constant_speed { u * total } else { let f = u * m as f64; let i = (f.floor() as usize).min(m - 1); cum[i] + (f - i as f64) * (cum[i + 1] - cum[i]) };
    let (x, y, _) = at(s);
    let ang = |s: f64| -> f64 { let e = (total * 0.004).max(0.5); let (x0, y0, _) = at(s - e); let (x1, y1, _) = at(s + e); (y1 - y0).atan2(x1 - x0) };
    let a = if mp.orient_smooth > 0.0 { let win = total * 0.08 * mp.orient_smooth; let (mut cs, mut sn) = (0.0, 0.0); for k in -4..=4 { let a = ang(s + win * k as f64 / 4.0); cs += a.cos(); sn += a.sin(); } sn.atan2(cs) } else { ang(s) };
    (x - w as f64 / 2.0, y - h as f64 / 2.0, a.to_degrees())
}

/// Turn through-points into a smooth bezier path (Catmull-Rom, tension 0..1): the easy way for an AI to say "move through these points".
pub fn smooth_points(pts: &[(f64, f64)], closed: bool, tension: f64) -> Vec<PathPt> {
    let n = pts.len(); let k = tension.clamp(0.0, 1.0) / 6.0 * 2.0 * 0.5 * 3.0 / 3.0 * 3.0 / 3.0; // 1/6 at tension 1 (classic Catmull-Rom)
    (0..n).map(|i| { let p = pts[i]; let (a, b) = if closed { (pts[(i + n - 1) % n], pts[(i + 1) % n]) } else { (pts[i.saturating_sub(1)], pts[(i + 1).min(n - 1)]) };
        let (tx, ty) = (b.0 - a.0, b.1 - a.1); let s = k * 3.0 / 3.0; let (ox, oy) = (tx * s, ty * s);
        let (ix, iy) = if !closed && i == 0 { (0.0, 0.0) } else { (-ox, -oy) }; let (ox, oy) = if !closed && i == n - 1 { (0.0, 0.0) } else { (ox, oy) };
        PathPt { x: p.0, y: p.1, ix, iy, ox, oy, f: 0.0, ..Default::default() } }).collect()
}
/// Preset paths in normalized canvas coords.
pub fn preset(shape: &str, cx: f64, cy: f64, rx: f64, ry: f64) -> Option<(Vec<(f64, f64)>, bool)> {
    let tau = std::f64::consts::TAU;
    Some(match shape {
        "circle" | "ellipse" => ((0..12).map(|i| { let a = i as f64 / 12.0 * tau; (cx + rx * a.cos(), cy + ry * a.sin()) }).collect(), true),
        "figure8" => ((0..16).map(|i| { let a = i as f64 / 16.0 * tau; (cx + rx * a.sin(), cy + ry * (2.0 * a).sin() * 0.5) }).collect(), true),
        "wave" => ((0..=10).map(|i| { let u = i as f64 / 10.0; (cx - rx + 2.0 * rx * u, cy + ry * (u * tau * 1.5).sin()) }).collect(), false),
        "arc" => ((0..=8).map(|i| { let u = i as f64 / 8.0; let a = std::f64::consts::PI * (1.0 - u); (cx + rx * a.cos(), cy - ry * a.sin()) }).collect(), false),
        "spiral" => ((0..=24).map(|i| { let u = i as f64 / 24.0; let a = u * tau * 2.0; (cx + rx * u * a.cos(), cy + ry * u * a.sin()) }).collect(), false),
        _ => return None })
}

/// Onset-envelope beat tracker: decode mono 11.025 kHz audio, spectral-flux-like energy difference in 512-sample hops, tempo by autocorrelation (60-200 BPM), beat grid phase-locked to the strongest onsets.
pub fn detect_beats(path: &str, t0: f64, dur: f64, min_bpm: f64, max_bpm: f64) -> Result<(f64, Vec<f64>, f64), String> {
    let out = std::process::Command::new("ffmpeg").args(["-v","error","-ss",&format!("{t0}"),"-t",&format!("{dur}"),"-i",path,"-vn","-ac","1","-ar","11025","-f","s16le","-"]).output().map_err(|e| e.to_string())?;
    let s: Vec<f32> = out.stdout.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect(); if s.len() < 11025 { return Err("audio too short or missing".into()); }
    let hop = 256usize; let hz = 11025.0 / hop as f64; let n = s.len() / hop; let mut env = vec![0f32; n];
    // high-passed energy (first difference) then half-wave rectified flux
    let mut prev = 0f32; for i in 0..n { let mut e = 0f32; for k in 0..hop { let x = s[i * hop + k]; let d = x - if k > 0 { s[i * hop + k - 1] } else { 0.0 }; e += d * d; } let e = (e / hop as f32).sqrt(); env[i] = (e - prev).max(0.0); prev = e * 0.5 + prev * 0.5; }
    let mean = env.iter().sum::<f32>() / n as f32; for v in env.iter_mut() { *v = (*v - mean).max(0.0); }
    let (lo, hi) = ((hz * 60.0 / max_bpm) as usize, (hz * 60.0 / min_bpm) as usize); let (mut best, mut bl) = (-1.0f64, lo.max(2));
    for l in lo.max(2)..=hi.min(n / 2) { let mut ac = 0f64; for i in 0..n - l { ac += (env[i] * env[i + l]) as f64; } let ac = ac / (n - l) as f64; if ac > best { best = ac; bl = l; } }
    let bpm0 = 60.0 * hz / bl as f64; let bpm_c = if bpm0 < 70.0 { bpm0 * 2.0 } else { bpm0 };
    let (mut bph, mut bs, mut bpm, mut per) = (0.0f64, -1.0f32, bpm_c, 60.0 / bpm_c * hz);
    let mut cand = bpm_c - 5.0; while cand <= bpm_c + 5.0 { let pr = 60.0 / cand * hz; for p in 0..48 { let ph = pr * p as f64 / 48.0; let mut sum = 0f32; let mut b = ph; let mut cnt = 0; while (b as usize) < n { let i = b as usize; sum += env[i].max(env[i.saturating_sub(1)]).max(env[(i + 1).min(n - 1)]); b += pr; cnt += 1; } let sc = sum / cnt.max(1) as f32; if sc > bs { bs = sc; bph = ph; bpm = cand; per = pr; } } cand += 0.25; }
    let mut beats = vec![]; let mut b = bph; while (b as usize) < n { beats.push(t0 + b / hz); b += per; }
    let conf = if mean > 0.0 { (best / (mean as f64).powi(2).max(1e-12)).min(99.0) } else { 0.0 }; Ok((bpm, beats, conf))
}
