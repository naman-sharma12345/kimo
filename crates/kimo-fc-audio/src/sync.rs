// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit 25979b15a419d4457fb67ea5e10c1a15d2e56b62), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Synchronising recordings of the same event by their audio (Synchronize, Merge Clips and
//! Create Multi-Camera Source Sequence "Audio").
//!
//! [`find_offset`] returns the lag `L` (in samples) for which `b[n] ≈ g · a[n + L]`: when `b`
//! started recording two seconds after `a`, `L = 2 · sample_rate`. It works in two passes:
//!
//! 1. **Coarse.** Both signals are DC-removed, normalised, low-pass filtered (windowed sinc) and
//!    decimated to ≤ 8 kHz (further for very long recordings, so the transform stays ≤ 4 M points).
//!    The generalised cross-correlation with partial phase transform weighting (GCC-PHAT-β:
//!    `R = A·B* / |A·B*|^β`) is computed with one complex FFT of both signals packed as `a + i·b`.
//!    PHAT weighting whitens the spectra, so the peak is sharp and independent of the microphones'
//!    gains and frequency responses; β < 1 keeps noise-only bins from dominating.
//! 2. **Refine.** Around the coarse peak, the same correlation is computed at the full sample rate
//!    on the loudest common window (≤ 2.7 s at 48 kHz). Its highest peak, of either sign, gives the
//!    lag and the polarity, and is refined by parabolic interpolation to a fraction of a sample.
//!
//! Two figures say how much to trust the result: `confidence`, the coarse peak's height over the
//! RMS of the correlation, and `distinct`, its height over the highest peak more than 50 ms away
//! (periodic material such as music or a steady hum can match at several lags).
//!
//! References: Knapp & Carter, "The generalized correlation method for estimation of time delay",
//! IEEE Trans. ASSP 24(4), 1976; the β-weighted PHAT variant is standard textbook practice.

use crate::fft::Fft;

/// Tuning of [`find_offset`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyncOptions {
    /// Largest |lag| considered, in samples (None = anything with enough overlap).
    pub max_lag: Option<i64>,
    /// Upper bound of the coarse search's sample rate (Hz).
    pub coarse_rate: u32,
    /// Most samples (both signals together) the coarse transform may hold; longer recordings are
    /// decimated further.
    pub max_coarse_samples: usize,
    /// PHAT weighting exponent (1 = full whitening, 0 = plain cross-correlation).
    pub phat_beta: f32,
    /// Length of the full-rate refinement window (samples).
    pub refine_window: usize,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self { max_lag: None, coarse_rate: 8000, max_coarse_samples: 1 << 22, phat_beta: 0.75, refine_window: 1 << 17 }
    }
}

/// The offset found between two recordings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyncResult {
    /// `b[n]` lines up with `a[n + lag]` (samples at the input rate).
    pub lag: i64,
    /// The same with sub-sample precision (parabolic interpolation of the refined peak).
    pub lag_fine: f64,
    /// Coarse peak height over the RMS of the correlation.
    pub confidence: f32,
    /// Coarse peak height over the next-highest peak more than 50 ms away.
    pub distinct: f32,
    /// True when the signals correlate negatively (one recording has inverted polarity).
    pub inverted: bool,
}

impl SyncResult {
    /// Whether the match is clear enough to trust.
    pub fn reliable(&self) -> bool {
        self.confidence >= 8.0 && self.distinct >= 1.5
    }
}

/// Average interleaved or planar channels into one mono signal.
pub fn mixdown(channels: &[&[f32]]) -> Vec<f32> {
    let n = channels.iter().map(|c| c.len()).min().unwrap_or(0);
    let k = channels.len().max(1) as f32;
    (0..n).map(|i| channels.iter().map(|c| c[i]).sum::<f32>() / k).collect()
}

/// Low-pass filter (Blackman-windowed sinc, cutoff at 0.42 × the new sample rate) and keep every
/// `d`-th sample.
pub fn decimate(x: &[f32], d: usize) -> Vec<f32> {
    if x.is_empty() || d <= 1 {
        return x.to_vec();
    }
    // Factors above the input length cannot produce another output sample. Cap the useful factor
    // as well as the filter width so a public caller cannot turn a tiny input into a huge plan.
    const MAX_FACTOR: usize = 4096;
    let d = d.min(x.len()).min(MAX_FACTOR);
    let half = 8 * d;
    let fc = 0.42 / d as f64; // cycles per input sample
    let taps: Vec<f32> = {
        let n = 2 * half + 1;
        let mut h: Vec<f64> = (0..n)
            .map(|k| {
                let t = k as f64 - half as f64;
                let sinc = if t == 0.0 { 2.0 * fc } else { (2.0 * std::f64::consts::PI * fc * t).sin() / (std::f64::consts::PI * t) };
                let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * k as f64 / (n - 1) as f64).cos()
                    + 0.08 * (4.0 * std::f64::consts::PI * k as f64 / (n - 1) as f64).cos();
                sinc * w
            })
            .collect();
        let s: f64 = h.iter().sum();
        h.iter_mut().for_each(|v| *v /= s);
        h.into_iter().map(|v| v as f32).collect()
    };
    let out_len = x.len().div_ceil(d);
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let c = (i * d) as isize;
        let lo = (c - half as isize).max(0) as usize;
        let hi = ((c + half as isize + 1) as usize).min(x.len());
        let k0 = (lo as isize - (c - half as isize)) as usize;
        let mut acc = 0.0f32;
        for (j, &v) in x[lo..hi].iter().enumerate() {
            acc += v * taps[k0 + j];
        }
        out.push(acc);
    }
    out
}

/// Remove the mean and scale to unit RMS (silence stays silence).
fn normalise(x: &mut [f32]) {
    if x.is_empty() {
        return;
    }
    let mean = (x.iter().map(|&v| v as f64).sum::<f64>() / x.len() as f64) as f32;
    x.iter_mut().for_each(|v| *v -= mean);
    let rms = (x.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>() / x.len() as f64).sqrt();
    if rms > 1e-12 {
        let k = (1.0 / rms) as f32;
        x.iter_mut().for_each(|v| *v *= k);
    }
}

/// GCC-PHAT-β of `a` and `b` (zero-padded to `n`, a power of two ≥ `a.len() + b.len()`):
/// `r[L mod n] = Σ a[i + L]·b[i]` with whitened spectra. Only bins whose frequency (cycles per
/// sample) lies in `band` contribute.
fn gcc(a: &[f32], b: &[f32], n: usize, beta: f32, band: (f32, f32)) -> Vec<f32> {
    let fft = Fft::new(n);
    let mut re = vec![0.0f32; n];
    let mut im = vec![0.0f32; n];
    re[..a.len()].copy_from_slice(a);
    im[..b.len()].copy_from_slice(b);
    fft.forward(&mut re, &mut im);
    // X = A + iB for real a, b: A(k) = (X(k) + X*(n-k)) / 2, B(k) = (X(k) - X*(n-k)) / 2i.
    let weigh = |k: usize, xr: f32, xi: f32, yr: f32, yi: f32| -> (f32, f32) {
        // y = X*(n-k) given as (yr, yi) already conjugated
        let (ar, ai) = (0.5 * (xr + yr), 0.5 * (xi + yi));
        let (dr, di) = (xr - yr, xi - yi);
        let (br, bi) = (0.5 * di, -0.5 * dr);
        // R = A·conj(B)
        let rr = ar * br + ai * bi;
        let ri = ai * br - ar * bi;
        let f = k.min(n - k) as f32 / n as f32;
        if f < band.0 || f > band.1 {
            return (0.0, 0.0);
        }
        let mag = (rr * rr + ri * ri).sqrt();
        if mag <= 1e-30 {
            return (0.0, 0.0);
        }
        let w = mag.powf(-beta);
        (rr * w, ri * w)
    };
    for k in 0..=n / 2 {
        let j = (n - k) % n;
        let (xkr, xki, xjr, xji) = (re[k], im[k], re[j], im[j]);
        let rk = weigh(k, xkr, xki, xjr, -xji);
        let rj = weigh(j, xjr, xji, xkr, -xki);
        re[k] = rk.0;
        im[k] = rk.1;
        re[j] = rj.0;
        im[j] = rj.1;
    }
    fft.inverse(&mut re, &mut im);
    re
}

fn corr_at(r: &[f32], lag: i64) -> f32 {
    let n = r.len() as i64;
    r[lag.rem_euclid(n) as usize]
}

/// Find the lag `L` with `b[n] ≈ g·a[n + L]` (see the module docs). `None` when either signal is
/// too short or silent, or no lag leaves enough overlap.
pub fn find_offset(a: &[f32], b: &[f32], sample_rate: u32, opts: &SyncOptions) -> Option<SyncResult> {
    let sr = sample_rate.max(1) as usize;
    let min_len = (sr / 10).max(64);
    if a.len() < min_len || b.len() < min_len {
        return None;
    }
    let energy = |x: &[f32]| x.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>();
    if energy(a) < 1e-12 || energy(b) < 1e-12 {
        return None;
    }
    // ---- coarse
    let d = (sr.div_ceil(opts.coarse_rate.max(500) as usize)).max((a.len() + b.len()).div_ceil(opts.max_coarse_samples.max(1 << 12))).max(1);
    let mut ca = a.to_vec();
    let mut cb = b.to_vec();
    normalise(&mut ca);
    normalise(&mut cb);
    let (ca, cb) = (decimate(&ca, d), decimate(&cb, d));
    let (la, lb) = (ca.len() as i64, cb.len() as i64);
    let n = ((la + lb) as usize).next_power_of_two();
    let rate_c = sr as f32 / d as f32;
    let band = ((40.0 / rate_c).min(0.05), if d > 1 { 0.40 } else { 0.5 });
    let r = gcc(&ca, &cb, n, opts.phat_beta, band);
    // valid lags: b[0..lb) against a[L..L+lb) must overlap by a minimum amount
    let min_overlap = ((rate_c as i64) / 2).min(la.min(lb) / 4).max(8);
    let max_lag_c = opts.max_lag.map(|m| m.abs() / d as i64 + 1);
    let lo = -(lb - min_overlap);
    let hi = la - min_overlap;
    if hi < lo {
        return None;
    }
    let mut best = (0i64, 0.0f32);
    let mut sum2 = 0.0f64;
    let mut count = 0usize;
    for l in lo..=hi {
        if max_lag_c.is_some_and(|m| l.abs() > m) {
            continue;
        }
        let v = corr_at(&r, l);
        sum2 += (v as f64) * (v as f64);
        count += 1;
        if v.abs() > best.1.abs() {
            best = (l, v);
        }
    }
    if count == 0 || best.1 == 0.0 {
        return None;
    }
    let rms = (sum2 / count as f64).sqrt().max(1e-30) as f32;
    let confidence = best.1.abs() / rms;
    let guard = ((rate_c * 0.05) as i64).max(2);
    let mut second = 0.0f32;
    for l in lo..=hi {
        if (l - best.0).abs() <= guard || max_lag_c.is_some_and(|m| l.abs() > m) {
            continue;
        }
        second = second.max(corr_at(&r, l).abs());
    }
    let distinct = if second > 0.0 { best.1.abs() / second } else { f32::INFINITY };
    let inverted = best.1 < 0.0;
    let coarse = best.0 * d as i64;
    // ---- refine at the full rate
    let margin = (2 * d + 4) as i64;
    let (fa, fb) = (a.len() as i64, b.len() as i64);
    let ov0 = (-coarse).max(0);
    let ov1 = fb.min(fa - coarse);
    if ov1 - ov0 < 16 {
        return Some(SyncResult { lag: coarse, lag_fine: coarse as f64, confidence, distinct, inverted });
    }
    let w = ((ov1 - ov0) as usize).min(opts.refine_window.max(256)) as i64;
    // the window of b with the most energy (both sides: a lag-shifted)
    let start = {
        let hop = (w / 4).max(1);
        let pre = |x: &[f32]| {
            let mut p = Vec::with_capacity(x.len() + 1);
            p.push(0.0f64);
            let mut s = 0.0;
            for &v in x {
                s += (v as f64) * (v as f64);
                p.push(s);
            }
            p
        };
        let (pa, pb) = (pre(a), pre(b));
        let mut s = ov0;
        let mut best_s = ov0;
        let mut best_e = -1.0f64;
        while s + w <= ov1 {
            let eb = pb[(s + w) as usize] - pb[s as usize];
            let ea = pa[(s + coarse + w) as usize] - pa[(s + coarse) as usize];
            let e = eb.min(ea);
            if e > best_e {
                best_e = e;
                best_s = s;
            }
            s += hop;
        }
        best_s
    };
    let bw: Vec<f32> = b[start as usize..(start + w) as usize].to_vec();
    let aw: Vec<f32> = (start + coarse - margin..start + coarse + w + margin).map(|i| if i >= 0 && i < fa { a[i as usize] } else { 0.0 }).collect();
    let mut aw = aw;
    let mut bw = bw;
    normalise(&mut aw);
    normalise(&mut bw);
    let n2 = (aw.len() + bw.len()).next_power_of_two();
    let r2 = gcc(&aw, &bw, n2, opts.phat_beta, (20.0 / sr as f32, 0.5));
    // r2[L'] = Σ aw[i + L']·bw[i]; L' = margin + δ. The highest peak of either sign sets the lag
    // and the polarity: when one recording's phase is turned (a band-limited scratch track), the
    // coarse peak and the opposite-signed lobe beside it can come out nearly equal.
    let mut bl = 0i64;
    let mut bv = 0.0f32;
    for l in 0..=2 * margin {
        let v = corr_at(&r2, l);
        if v.abs() > bv.abs() {
            bv = v;
            bl = l;
        }
    }
    let inverted = if bv == 0.0 { inverted } else { bv < 0.0 };
    let sign = if inverted { -1.0 } else { 1.0 };
    let (y0, y1, y2) = (corr_at(&r2, bl - 1) * sign, bv * sign, corr_at(&r2, bl + 1) * sign);
    let den = y0 - 2.0 * y1 + y2;
    let frac = if den.abs() > 1e-20 { (0.5 * (y0 - y2) / den).clamp(-0.5, 0.5) } else { 0.0 };
    let delta = bl - margin;
    let lag = coarse + delta;
    Some(SyncResult { lag, lag_fine: lag as f64 + frac as f64, confidence, distinct, inverted })
}


