//! Manga / comic speed lines: tapered radial rays from a center, reshuffled at a flicker rate, optional rotation.
//! Used for impact bursts (SCUMBLE 12.5, 15.6, 45.3, 50.2 s).
use serde::{Deserialize, Serialize};
use crate::effects::Frame;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SpeedLines {
    #[serde(default = "d_count")] pub count: u32,
    /// Rays start this far from the center, as a fraction of the half-diagonal.
    #[serde(default = "d_inner")] pub inner: f64,
    /// Ray half-width in degrees at the outer edge (min, max).
    #[serde(default = "d_w")] pub width: [f64; 2],
    #[serde(default = "d_col")] pub color: [f64; 3],
    /// Optional background fill [r,g,b,a]; a=0 leaves it transparent.
    #[serde(default)] pub bg: [f64; 4],
    /// Center, 0..1.
    #[serde(default = "d_c")] pub cx: f64,
    #[serde(default = "d_c")] pub cy: f64,
    /// Reshuffle rate (shuffles per second). 0 = static.
    #[serde(default = "d_fl")] pub flicker_fps: f64,
    /// Degrees per second.
    #[serde(default)] pub spin: f64,
    #[serde(default)] pub seed: u64,
}
fn d_count() -> u32 { 48 } fn d_inner() -> f64 { 0.28 } fn d_w() -> [f64; 2] { [0.5, 2.2] } fn d_col() -> [f64; 3] { [1.0, 1.0, 1.0] } fn d_c() -> f64 { 0.5 } fn d_fl() -> f64 { 12.0 }
fn hash(mut x: u64) -> f64 { x = x.wrapping_add(0x9E3779B97F4A7C15); x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9); x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB); x ^= x >> 31; (x >> 11) as f64 / (1u64 << 53) as f64 }

pub fn render(s: &SpeedLines, f: &mut Frame, lt: f64) {
    let (w, h) = (f.w, f.h);
    if s.bg[3] > 0.0 { for i in 0..w * h { for j in 0..3 { f.d[i * 4 + j] = s.bg[j] as f32; } f.d[i * 4 + 3] = s.bg[3] as f32; } }
    let step = if s.flicker_fps > 0.0 { (lt * s.flicker_fps).floor() as u64 } else { 0 };
    let n = s.count.clamp(1, 400) as usize;
    let (cx, cy) = (s.cx * w as f64, s.cy * h as f64);
    let rmax = ((w * w + h * h) as f64).sqrt() * 0.5 * (1.0 + (0.5 - s.cx).abs().max((0.5 - s.cy).abs()) * 2.0);
    let rot = (s.spin * lt).to_radians();
    // rays: (angle, half-width rad at outer, inner radius px)
    let rays: Vec<(f64, f64, f64)> = (0..n).map(|i| { let k = s.seed.wrapping_mul(7919).wrapping_add(step.wrapping_mul(104729)).wrapping_add((i as u64).wrapping_mul(31));
        let a = (i as f64 + hash(k) * 0.9) / n as f64 * std::f64::consts::TAU + rot; let hw = (s.width[0] + (s.width[1] - s.width[0]) * hash(k.wrapping_add(1)).powf(1.5)).to_radians();
        let inn = rmax * s.inner * (0.8 + 0.5 * hash(k.wrapping_add(2))); (a, hw, inn) }).collect();
    let col = [s.color[0] as f32, s.color[1] as f32, s.color[2] as f32];
    let tau = std::f64::consts::TAU;
    for y in 0..h { for x in 0..w {
        let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy); let r = (dx * dx + dy * dy).sqrt(); if r < 1.0 { continue; }
        let th = dy.atan2(dx); let mut cov = 0.0f64;
        for &(a, hw, inn) in &rays { if r < inn { continue; }
            let mut d = (th - a) % tau; if d > std::f64::consts::PI { d -= tau; } if d < -std::f64::consts::PI { d += tau; } let d = d.abs();
            let t = ((r - inn) / (rmax - inn).max(1.0)).clamp(0.0, 1.0); let half = hw * (0.15 + 0.85 * t.sqrt());
            if d < half + 1.0 / r { let c = ((half + 1.0 / r - d) * r).clamp(0.0, 1.0); if c > cov { cov = c; } } }
        if cov > 0.0 { let i = (y * w + x) * 4; let a = cov as f32; let da = f.d[i + 3]; let oa = a + da * (1.0 - a);
            for j in 0..3 { f.d[i + j] = (col[j] * a + f.d[i + j] * da * (1.0 - a)) / oa.max(1e-6); } f.d[i + 3] = oa; }
    }}
}

#[cfg(test)]
mod prng_overflow_tests {
    use super::*;
    #[test]
    fn extreme_seed_and_flicker_step_are_deterministic() {
        // seed * 7919 wraps to u64::MAX, exercising hash offset overflow.
        for (seed, time, flicker) in [(5627899189554524145u64, 0.0, 0.0), (0, 1e16, 12.0)] {
            let s: SpeedLines = serde_json::from_value(serde_json::json!({"seed":seed,"flicker_fps":flicker})).unwrap();
            let mut a = Frame::new(16, 16);
            let mut b = Frame::new(16, 16);
            render(&s, &mut a, time);
            render(&s, &mut b, time);
            assert_eq!(a.d, b.d);
            assert!(a.d.iter().all(|v| v.is_finite()));
        }
    }
}
