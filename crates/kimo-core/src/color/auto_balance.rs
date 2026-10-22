//! Auto Balance (Resolve's "A" in the Color page): neutralise a colour cast with no reference clip and no hand-picked grey.
//! Which pixels may be neutral: a frame can be dominated by strongly coloured things (a red sheet, a lilac cup) whose per-channel
//! extremes are not neutral, so the measurement first finds the neutral cluster in log-chroma (ln r/g, ln b/g): mean-shift
//! (bandwidth 0.15) from the origin and from the median chroma, each mode scored by its share of pixels within 0.2, weighted
//! towards low chroma (Gaussian prior, sigma 0.5). A global cast moves every grey together, so the cluster is the cast itself.
//! Only pixels within 0.2 of that mode (wider for dark pixels, whose ratios are noisy) are measured: per channel the robust
//! black and white points (a percentile clipped at each end) are mapped to their common level (their Rec.709 luma, so overall
//! brightness is kept; or 0 and 1 with `stretch`). Mids are then neutralised by a per-channel gamma fitted to near-neutral
//! pixels (Rec.709 luma 0.15..0.85, |(Cb,Cr)| < 0.05 after the black/white map), clamped to 0.8..1.25, and skipped when fewer
//! than 1% of the measured pixels qualify. Confidence: when under 30% of the frame belongs to the neutral cluster the frame
//! is mostly strong colour and the correction is not applied; from 30% to 60% it is scaled in linearly; above 60% it is full.
//! Display-referred: it measures encoded pixel values.
use crate::effects::Frame;

#[derive(Clone, Debug)]
pub struct Balance { pub black: [f64; 3], pub white: [f64; 3], pub target: [f64; 2], pub slope: [f64; 3], pub intercept: [f64; 3], pub gamma: [f64; 3], pub pixels: usize, pub mid_pixels: usize,
    /// log-chroma (ln r/g, ln b/g) of the neutral cluster: the cast as measured
    pub neutral_chroma: [f64; 2], /// share of measured pixels in the neutral cluster
    pub neutral_share: f64, /// 0..1 scale applied to the correction (strength multiplies it)
    pub confidence: f64 }

const EPS: f64 = 0.01; const BANDWIDTH: f64 = 0.15; const RADIUS: f64 = 0.2; const PRIOR: f64 = 0.5; const SHARE_LO: f64 = 0.3; const SHARE_HI: f64 = 0.6;

fn percentile(v: &mut [f32], q: f64) -> f64 { let n = v.len(); let k = ((q * (n - 1) as f64).round() as usize).min(n - 1); let (_, x, _) = v.select_nth_unstable_by(k, |a, b| a.total_cmp(b)); *x as f64 }
fn chroma(p: [f32; 3]) -> [f64; 2] { let c = p.map(|v| (v as f64).max(0.0) + EPS); [(c[0] / c[1]).ln(), (c[2] / c[1]).ln()] }
fn dist(a: [f64; 2], b: [f64; 2]) -> f64 { ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() }
/// Gaussian mean-shift from `c` to the nearest mode of `s` (weights taken relative to the nearest sample, so a far start cannot underflow).
fn mode(s: &[[f64; 2]], mut c: [f64; 2]) -> [f64; 2] {
    for _ in 0..80 { crate::guard::check(); let d2: Vec<f64> = s.iter().map(|p| dist(*p, c).powi(2)).collect(); let m = d2.iter().cloned().fold(f64::INFINITY, f64::min);
        let (mut w, mut a, mut b) = (0.0, 0.0, 0.0); for (p, d) in s.iter().zip(&d2) { let k = (-(d - m) / (2.0 * BANDWIDTH * BANDWIDTH)).exp(); w += k; a += k * p[0]; b += k * p[1]; }
        if !(w > 0.0) { break; } let n = [a / w, b / w]; let moved = dist(n, c); c = n; if moved < 1e-5 { break; } }
    c
}

/// Measure the balance over every opaque (alpha > 0.5), finite pixel of `frames`. `pct` is the percent clipped at each end (0..10).
pub fn measure(frames: &[Frame], pct: f64, stretch: bool, mids: bool) -> Result<Balance, String> {
    if !pct.is_finite() || !(0.0..=10.0).contains(&pct) { return Err("auto_color: percentile must be in 0..10".into()); }
    let mut px: Vec<[f32; 3]> = Vec::new();
    for f in frames { for p in f.d.chunks_exact(4) { if !(p[3] > 0.5) || p[..3].iter().any(|v| !v.is_finite()) { continue; } px.push([p[0], p[1], p[2]]); } }
    if px.len() < 16 { return Err(format!("auto_color: only {} opaque pixels to measure (need 16)", px.len())); }
    let y709 = |c: [f64; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]; let yf = |p: &[f32; 3]| y709(p.map(|v| v as f64));
    // the neutral cluster, found on lit, unclipped pixels (at most ~200k of them)
    let lit: Vec<&[f32; 3]> = px.iter().filter(|p| yf(p) > 0.05 && p.iter().all(|v| (*v as f64) < 0.98)).collect(); let step = (lit.len() / 200_000).max(1);
    let sample: Vec<[f64; 2]> = lit.iter().step_by(step).map(|p| chroma(**p)).collect();
    let center = if sample.len() < 16 { [0.0, 0.0] } else {
        let med = |j: usize| { let mut v: Vec<f64> = sample.iter().map(|p| p[j]).collect(); let k = v.len() / 2; *v.select_nth_unstable_by(k, |a, b| a.total_cmp(b)).1 };
        let mut best = (f64::NEG_INFINITY, [0.0, 0.0]);
        for start in [[0.0, 0.0], [med(0), med(1)]] { let c = mode(&sample, start); let share = sample.iter().filter(|p| dist(**p, c) < RADIUS).count() as f64 / sample.len() as f64;
            let score = share * (-(c[0] * c[0] + c[1] * c[1]) / (2.0 * PRIOR * PRIOR)).exp(); if score > best.0 { best = (score, c); } }
        best.1 };
    let sel: Vec<[f32; 3]> = px.iter().filter(|p| { let lo = p.iter().fold(f32::INFINITY, |m, v| m.min(*v)).max(0.0) as f64; dist(chroma(**p), center) < RADIUS + 0.02 / (lo + EPS) }).cloned().collect();
    let neutral_share = sel.len() as f64 / px.len() as f64; let confidence = ((neutral_share - SHARE_LO) / (SHARE_HI - SHARE_LO)).clamp(0.0, 1.0);
    if sel.len() < 16 || confidence == 0.0 {
        return Ok(Balance { black: [0.0; 3], white: [1.0; 3], target: [0.0, 1.0], slope: [1.0; 3], intercept: [0.0; 3], gamma: [1.0; 3], pixels: px.len(), mid_pixels: 0, neutral_chroma: center, neutral_share, confidence: 0.0 }); }
    let mut ch: [Vec<f32>; 3] = [0, 1, 2].map(|j| sel.iter().map(|p| p[j]).collect());
    let q = pct / 100.0; let (mut black, mut white) = ([0.0; 3], [0.0; 3]);
    for j in 0..3 { black[j] = percentile(&mut ch[j], q); white[j] = percentile(&mut ch[j], 1.0 - q); }
    for j in 0..3 { if white[j] - black[j] < 0.02 { return Err(format!("auto_color: the frame is too flat to balance ({} channel range {:.4} < 0.02)", ["red", "green", "blue"][j], white[j] - black[j])); } }
    let (tb, tw) = if stretch { (0.0, 1.0) } else { (y709(black), y709(white)) };
    let slope = [0, 1, 2].map(|j| (tw - tb) / (white[j] - black[j])); let intercept = [0, 1, 2].map(|j| tb - slope[j] * black[j]);
    let (mut gamma, mut mid_pixels) = ([1.0; 3], 0usize);
    if mids {
        let mut sum = [0f64; 3];
        for p in &sel { let c = [0, 1, 2].map(|j| slope[j] * p[j] as f64 + intercept[j]); let y = y709(c);
            if !(0.15..=0.85).contains(&y) { continue; } let (cb, cr) = ((c[2] - y) / 1.8556, (c[0] - y) / 1.5748); if (cb * cb + cr * cr).sqrt() >= 0.05 { continue; }
            for j in 0..3 { sum[j] += c[j]; } mid_pixels += 1; }
        if mid_pixels > 0 && mid_pixels * 100 >= sel.len() { let m = sum.map(|v| v / mid_pixels as f64); let t = y709(m);
            if m.iter().all(|v| *v > 0.02 && *v < 0.98) { gamma = m.map(|v| (v.ln() / t.ln()).clamp(0.8, 1.25)); } }
    }
    Ok(Balance { black, white, target: [tb, tw], slope, intercept, gamma, pixels: px.len(), mid_pixels, neutral_chroma: center, neutral_share, confidence })
}

/// Primaries model 1 per-channel [lift, gain, gamma] for `b` scaled by strength x confidence (0 = exact identity): out = ((gain-lift)x + lift)^(1/gamma).
pub fn wheels(b: &Balance, strength: f64) -> [[f64; 3]; 3] {
    let strength = strength * b.confidence;
    let lift = [0, 1, 2].map(|j| strength * b.intercept[j]); let slope = [0, 1, 2].map(|j| 1.0 + strength * (b.slope[j] - 1.0));
    [lift, [0, 1, 2].map(|j| slope[j] + lift[j]), [0, 1, 2].map(|j| 1.0 + strength * (b.gamma[j] - 1.0))]
}
