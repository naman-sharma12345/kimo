//! Provenance: vendored from beatbox-rs (github.com/naman-sharma12345/beatbox-rs) commit 907fb0a1c8a30c245facba58ea27df8366080526,
//! src/resample.rs (MIT), itself adapted from SoundCraft (https://github.com/storytold/soundcraft, crates/dsp/src/offline.rs,
//! commit eac0edd, MIT OR Apache-2.0, used here under MIT), Copyright (c) 2026 ArtCraft Team and the SoundCraft contributors.
//! Changes from the beatbox copy: kernel table and accumulation in f64 (thread-independent determinism), typed errors and guards
//! on resample(), pitch_read and the 44.1k-specific tests dropped. See THIRD_PARTY_NOTICES.md.
//!
//! Kaiser-windowed-sinc resampling (beta 9, 512-phase table with linear interpolation between phases; cutoff lowered to
//! 0.94 x ratio when decimating so rate conversion does not alias).

use std::f64::consts::PI;

const SINC_RES: usize = 512;
/// Zero crossings per side for offline-quality conversion.
pub const ZC_HQ: f64 = 32.0;

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let y = x * x / 4.0;
    for k in 1..64 {
        term *= y / (k as f64 * k as f64);
        sum += term;
        if term < sum * 1e-12 {
            break;
        }
    }
    sum
}

/// Precomputed one-sided windowed-sinc kernel for a given cutoff (fraction of input Nyquist).
pub struct SincTable {
    table: Vec<f64>,
    half_width: f64,
}

impl SincTable {
    pub fn new(cutoff: f64, zero_crossings: f64) -> Self {
        let cutoff = cutoff.clamp(0.01, 1.0);
        let half_width = zero_crossings / cutoff;
        let n = (half_width * SINC_RES as f64).ceil() as usize + 2;
        let beta = 9.0;
        let i0b = bessel_i0(beta);
        let table = (0..n)
            .map(|i| {
                let t = i as f64 / SINC_RES as f64;
                let x = t / half_width;
                if x >= 1.0 {
                    return 0.0;
                }
                let arg = PI * cutoff * t;
                let sinc = if arg.abs() < 1e-12 {
                    1.0
                } else {
                    arg.sin() / arg
                };
                let w = bessel_i0(beta * (1.0 - x * x).max(0.0).sqrt()) / i0b;
                cutoff * sinc * w
            })
            .collect();
        SincTable { table, half_width }
    }

    #[inline]
    fn at(&self, t: f64) -> f64 {
        let pos = t.abs() * SINC_RES as f64;
        let i = pos as usize;
        let f = pos - i as f64;
        match (self.table.get(i), self.table.get(i + 1)) {
            (Some(a), Some(b)) => a + (b - a) * f,
            (Some(a), None) => *a,
            _ => 0.0,
        }
    }
}

/// Table suited to a conversion `ratio` (output rate / input rate).
pub fn table_for(ratio: f64, zero_crossings: f64) -> SincTable {
    let cutoff = if ratio < 1.0 { ratio * 0.94 } else { 0.97 };
    SincTable::new(cutoff, zero_crossings)
}

/// `out_len` samples where output sample `n` reads input time `n / ratio`.
pub fn resample_with(x: &[f32], ratio: f64, out_len: usize, table: &SincTable) -> Vec<f32> {
    let len = x.len() as i64;
    let hw = table.half_width;
    (0..out_len)
        .map(|n| {
            let t = n as f64 / ratio;
            let lo = (t - hw).ceil().max(0.0) as i64;
            let hi = ((t + hw).floor() as i64).min(len - 1);
            let mut acc = 0.0f64;
            let mut j = lo;
            while j <= hi {
                acc += f64::from(x[j as usize]) * table.at(t - j as f64);
                j += 1;
            }
            acc as f32
        })
        .collect()
}

/// Largest accepted conversion ratio in either direction.
pub const MAX_RATIO: f64 = 64.0;
/// High-quality sample-rate conversion from `from` Hz to `to` Hz (mono).
/// `from == to` returns the input unchanged. Errors are typed strings ("code: text"): out_of_range for a zero rate or a ratio beyond
/// 1/64..64, invalid_samples for fewer than 2 samples or a non-finite sample, limit_exceeded when the output would pass max_samples.
pub fn resample(x: &[f32], from: u32, to: u32) -> Result<Vec<f32>, String> {
    if from == 0 || to == 0 {
        return Err("out_of_range: sample rates must be above 0".into());
    }
    if from == to {
        return Ok(x.to_vec());
    }
    if x.len() < 2 {
        return Err(format!("invalid_samples: resampling needs at least 2 samples, got {}", x.len()));
    }
    if x.iter().any(|v| !v.is_finite()) {
        return Err("invalid_samples: samples must be finite".into());
    }
    let ratio = f64::from(to) / f64::from(from);
    if !(1.0 / MAX_RATIO..=MAX_RATIO).contains(&ratio) {
        return Err(format!("out_of_range: rate ratio {ratio:.4} outside 1/{MAX_RATIO}..{MAX_RATIO}"));
    }
    let out_len = (x.len() as f64 * ratio).round() as usize;
    let cap = crate::guard::effective_limits().max_samples;
    if out_len as u64 > cap {
        return Err(format!("limit_exceeded: max_samples used {out_len}, cap {cap}; lower usage or adjust set_limits max_samples"));
    }
    crate::guard::reserve(out_len.saturating_mul(4));
    Ok(resample_with(x, ratio, out_len, &table_for(ratio, ZC_HQ)))
}

/// Inter-sample (true) peak, 4x oversampled with the Kaiser sinc kernel.
/// Only neighbourhoods of samples above half the sample peak are
/// interpolated, which is exact for the maximum and cheap on full songs.
pub fn true_peak(x: &[f32]) -> f32 {
    let table = SincTable::new(0.97, 16.0);
    let hw = table.half_width as i64;
    let n = x.len() as i64;
    let mut peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let gate = peak * 0.5;
    for i in 0..n {
        if x[i as usize].abs() < gate {
            continue;
        }
        for ph in 1..4 {
            let t = i as f64 + ph as f64 / 4.0;
            let (lo, hi) = ((t as i64 - hw).max(0), (t as i64 + hw + 1).min(n - 1));
            let mut acc = 0.0f64;
            for j in lo..=hi {
                acc += f64::from(x[j as usize]) * table.at(t - j as f64);
            }
            peak = peak.max(acc.abs() as f32);
        }
    }
    peak
}
