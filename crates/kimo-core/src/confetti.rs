//! Glitch confetti: random flat-color squares plus thin horizontal "tear" lines, reshuffled at a flicker rate.
//! Seen over the segmentation-style silhouette shots of the SCUMBLE promo (11.8-14 s). Transparent background.
use serde::{Deserialize, Serialize};
use crate::effects::Frame;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GlitchConfetti {
    #[serde(default = "d_sq")] pub squares: u32,
    /// Square side in px at 720p (min, max).
    #[serde(default = "d_size")] pub size: [f64; 2],
    /// Palette for squares, [r,g,b] each.
    #[serde(default = "d_pal")] pub palette: Vec<[f64; 3]>,
    #[serde(default = "d_ln")] pub lines: u32,
    /// Line thickness px (min, max) and length as fraction of width (min, max).
    #[serde(default = "d_lt")] pub line_thickness: [f64; 2],
    #[serde(default = "d_ll")] pub line_length: [f64; 2],
    #[serde(default = "d_lc")] pub line_colors: Vec<[f64; 3]>,
    /// Reshuffle rate in shuffles per second (0 = static).
    #[serde(default = "d_fl")] pub flicker_fps: f64,
    #[serde(default)] pub seed: u64,
}
fn d_sq() -> u32 { 40 } fn d_size() -> [f64; 2] { [10.0, 24.0] } fn d_ln() -> u32 { 6 } fn d_lt() -> [f64; 2] { [2.0, 5.0] } fn d_ll() -> [f64; 2] { [0.15, 0.6] } fn d_fl() -> f64 { 5.0 }
fn d_pal() -> Vec<[f64; 3]> { vec![[1.0, 0.25, 0.85], [0.1, 0.85, 0.9], [0.2, 0.8, 0.45], [1.0, 0.45, 0.1], [0.1, 0.2, 0.85], [0.08, 0.08, 0.1], [0.3, 0.3, 0.32]] }
fn d_lc() -> Vec<[f64; 3]> { vec![[1.0, 1.0, 1.0], [0.0, 0.0, 0.0], [1.0, 0.35, 0.08]] }
fn hash(mut x: u64) -> f64 { x = x.wrapping_add(0x9E3779B97F4A7C15); x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9); x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB); x ^= x >> 31; (x >> 11) as f64 / (1u64 << 53) as f64 }

fn rect(f: &mut Frame, x0: i64, y0: i64, x1: i64, y1: i64, c: [f64; 3]) {
    let (w, h) = (f.w as i64, f.h as i64);
    for y in y0.max(0)..y1.min(h) { for x in x0.max(0)..x1.min(w) { let i = (y as usize * f.w + x as usize) * 4; for j in 0..3 { f.d[i + j] = c[j] as f32; } f.d[i + 3] = 1.0; } }
}

pub fn render(s: &GlitchConfetti, f: &mut Frame, lt: f64) {
    let sc = f.h as f64 / 720.0;
    let step = if s.flicker_fps > 0.0 { (lt * s.flicker_fps).floor() as u64 } else { 0 };
    let base = s.seed.wrapping_mul(7919).wrapping_add(step.wrapping_mul(104729));
    if !s.palette.is_empty() { for i in 0..s.squares.min(2000) as u64 {
        let k = base.wrapping_add(i * 977);
        let sz = ((s.size[0] + (s.size[1] - s.size[0]) * hash(k)) * sc).round() as i64;
        let x = (hash(k + 1) * f.w as f64) as i64; let y = (hash(k + 2) * f.h as f64) as i64;
        let c = s.palette[(hash(k + 3) * s.palette.len() as f64) as usize % s.palette.len()];
        rect(f, x, y, x + sz, y + sz, c); } }
    if !s.line_colors.is_empty() { for i in 0..s.lines.min(500) as u64 {
        let k = base.wrapping_add(0xABCDEF).wrapping_add(i * 1013);
        let th = ((s.line_thickness[0] + (s.line_thickness[1] - s.line_thickness[0]) * hash(k)) * sc).round().max(1.0) as i64;
        let len = ((s.line_length[0] + (s.line_length[1] - s.line_length[0]) * hash(k + 1)) * f.w as f64) as i64;
        let x = (hash(k + 2) * (f.w as f64 - len as f64).max(1.0)) as i64; let y = (hash(k + 3) * f.h as f64) as i64;
        let c = s.line_colors[(hash(k + 4) * s.line_colors.len() as f64) as usize % s.line_colors.len()];
        rect(f, x, y, x + len, y + th, c); } }
}
