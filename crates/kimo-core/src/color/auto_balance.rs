//! Auto Balance (Resolve's "A" in the Color page): neutralise a colour cast with no reference clip and no hand-picked grey.
//! Per channel the robust black and white points (a percentile clipped at each end) are mapped to their common level
//! (their Rec.709 luma, so overall brightness is kept; or 0 and 1 with `stretch`). Mids are then neutralised by a per-channel
//! gamma fitted to near-neutral pixels (Rec.709 luma 0.15..0.85, |(Cb,Cr)| < 0.05 after the black/white map), clamped to 0.8..1.25,
//! and skipped when fewer than 1% of pixels qualify. Display-referred: it measures encoded pixel values.
use crate::effects::Frame;

#[derive(Clone, Debug)]
pub struct Balance { pub black: [f64; 3], pub white: [f64; 3], pub target: [f64; 2], pub slope: [f64; 3], pub intercept: [f64; 3], pub gamma: [f64; 3], pub pixels: usize, pub mid_pixels: usize }

fn percentile(v: &mut [f32], q: f64) -> f64 { let n = v.len(); let k = ((q * (n - 1) as f64).round() as usize).min(n - 1); let (_, x, _) = v.select_nth_unstable_by(k, |a, b| a.total_cmp(b)); *x as f64 }

/// Measure the balance over every opaque (alpha > 0.5), finite pixel of `frames`. `pct` is the percent clipped at each end (0..10).
pub fn measure(frames: &[Frame], pct: f64, stretch: bool, mids: bool) -> Result<Balance, String> {
    if !pct.is_finite() || !(0.0..=10.0).contains(&pct) { return Err("auto_color: percentile must be in 0..10".into()); }
    let mut ch: [Vec<f32>; 3] = Default::default(); let mut px: Vec<[f32; 3]> = Vec::new();
    for f in frames { for p in f.d.chunks_exact(4) { if !(p[3] > 0.5) || p[..3].iter().any(|v| !v.is_finite()) { continue; } for j in 0..3 { ch[j].push(p[j]); } px.push([p[0], p[1], p[2]]); } }
    if px.len() < 16 { return Err(format!("auto_color: only {} opaque pixels to measure (need 16)", px.len())); }
    let q = pct / 100.0; let (mut black, mut white) = ([0.0; 3], [0.0; 3]);
    for j in 0..3 { black[j] = percentile(&mut ch[j], q); white[j] = percentile(&mut ch[j], 1.0 - q); }
    for j in 0..3 { if white[j] - black[j] < 0.02 { return Err(format!("auto_color: the frame is too flat to balance ({} channel range {:.4} < 0.02)", ["red", "green", "blue"][j], white[j] - black[j])); } }
    let y709 = |c: [f64; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]; let (tb, tw) = if stretch { (0.0, 1.0) } else { (y709(black), y709(white)) };
    let slope = [0, 1, 2].map(|j| (tw - tb) / (white[j] - black[j])); let intercept = [0, 1, 2].map(|j| tb - slope[j] * black[j]);
    let (mut gamma, mut mid_pixels) = ([1.0; 3], 0usize);
    if mids {
        let mut sum = [0f64; 3];
        for p in &px { let c = [0, 1, 2].map(|j| slope[j] * p[j] as f64 + intercept[j]); let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            if !(0.15..=0.85).contains(&y) { continue; } let (cb, cr) = ((c[2] - y) / 1.8556, (c[0] - y) / 1.5748); if (cb * cb + cr * cr).sqrt() >= 0.05 { continue; }
            for j in 0..3 { sum[j] += c[j]; } mid_pixels += 1; }
        if mid_pixels > 0 && mid_pixels * 100 >= px.len() { let m = sum.map(|v| v / mid_pixels as f64); let t = y709(m);
            if m.iter().all(|v| *v > 0.02 && *v < 0.98) { gamma = m.map(|v| (v.ln() / t.ln()).clamp(0.8, 1.25)); } }
    }
    Ok(Balance { black, white, target: [tb, tw], slope, intercept, gamma, pixels: px.len(), mid_pixels })
}

/// Primaries model 1 per-channel [lift, gain, gamma] for `b` scaled by strength (0 = exact identity): out = ((gain-lift)x + lift)^(1/gamma).
pub fn wheels(b: &Balance, strength: f64) -> [[f64; 3]; 3] {
    let lift = [0, 1, 2].map(|j| strength * b.intercept[j]); let slope = [0, 1, 2].map(|j| 1.0 + strength * (b.slope[j] - 1.0));
    [lift, [0, 1, 2].map(|j| slope[j] + lift[j]), [0, 1, 2].map(|j| 1.0 + strength * (b.gamma[j] - 1.0))]
}
