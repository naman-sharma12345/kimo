//! Shaped film grain (Film Look Creator-style) behind the `grain` effect when any of its shaping params leaves its default.
//! Value noise on a lattice of `size` px (smoothstep-bilinear between cells; size 1 = one value per pixel), softened by a
//! Gaussian of sigma softness*size/2, each field normalised to zero mean and the legacy grain's standard deviation amount/sqrt(12).
//! `color` mixes a monochrome field with independent per-channel fields ((1-c)*mono + c*own, renormalised); the luma response is
//! the Bernstein blend shadows*(1-Y)^2 + 2*mids*Y(1-Y) + highlights*Y^2 (all 1 = flat); red/green/blue scale each channel.
//! Seeded by the frame seed (new grain every frame, same grain on re-render). Additive, unclipped; alpha kept.
use crate::effects::Frame;

fn hash(seed: u64, k: u64, x: i64, y: i64) -> f64 {
    let mut z = seed ^ k.wrapping_mul(0x9E3779B97F4A7C15) ^ (x as u64).wrapping_mul(0xC2B2AE3D27D4EB4F) ^ (y as u64).wrapping_mul(0x165667B19E3779F9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9); z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB); z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64 - 0.5
}

fn field(seed: u64, k: u64, w: usize, h: usize, size: f64, soft: f64, target: f64) -> Vec<f64> {
    let mut v: Vec<f64> = if size <= 1.0 { (0..w * h).map(|i| hash(seed, k, (i % w) as i64, (i / w) as i64)).collect() } else {
        let mut out = Vec::with_capacity(w * h);
        for y in 0..h { if y % 256 == 0 { crate::guard::check() } let gy = y as f64 / size; let (y0, fy) = (gy.floor() as i64, gy - gy.floor()); let sy = fy * fy * (3.0 - 2.0 * fy);
            for x in 0..w { let gx = x as f64 / size; let (x0, fx) = (gx.floor() as i64, gx - gx.floor()); let sx = fx * fx * (3.0 - 2.0 * fx);
                let (a, b, c, d) = (hash(seed, k, x0, y0), hash(seed, k, x0 + 1, y0), hash(seed, k, x0, y0 + 1), hash(seed, k, x0 + 1, y0 + 1));
                out.push((a + (b - a) * sx) * (1.0 - sy) + (c + (d - c) * sx) * sy); } }
        out };
    if soft > 0.0 { v = super::boost_detail::blur(&v, w, h, (soft * size * 0.5).max(0.3)); }
    let n = v.len() as f64; let m = v.iter().sum::<f64>() / n; let sd = (v.iter().map(|a| (a - m) * (a - m)).sum::<f64>() / n).sqrt();
    let k = if sd > 1e-12 { target / sd } else { 0.0 }; for a in v.iter_mut() { *a = (*a - m) * k; } v
}

/// resp = [shadows, mids, highlights], chan = [red, green, blue]; all ranges are checked by the caller.
pub fn apply(amount: f64, size: f64, soft: f64, color: f64, resp: [f64; 3], chan: [f64; 3], f: &mut Frame, seed: u64) {
    let (w, h) = (f.w, f.h); let n = w * h; if n == 0 || amount == 0.0 || !amount.is_finite() || f.d.len() < n * 4 { return; }
    let target = amount.abs() / 12f64.sqrt();
    let mono = if color < 1.0 { field(seed, 0, w, h, size, soft, target) } else { Vec::new() };
    let own: Vec<Vec<f64>> = if color > 0.0 { (1..=3).map(|k| field(seed, k, w, h, size, soft, target)).collect() } else { Vec::new() };
    let norm = ((1.0 - color).powi(2) + color * color).sqrt();
    for (i, p) in f.d.chunks_exact_mut(4).take(n).enumerate() { if i % 65536 == 0 { crate::guard::check() }
        let y = (0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64).clamp(0.0, 1.0); let y = if y.is_finite() { y } else { 0.0 };
        let wr = resp[0] * (1.0 - y) * (1.0 - y) + 2.0 * resp[1] * y * (1.0 - y) + resp[2] * y * y;
        for j in 0..3 { if chan[j] == 0.0 { continue; } let g = if color <= 0.0 { mono[i] } else if color >= 1.0 { own[j][i] } else { ((1.0 - color) * mono[i] + color * own[j][i]) / norm };
            let d = (g * wr * chan[j]) as f32; if d.is_finite() { p[j] += d; } } }
}
