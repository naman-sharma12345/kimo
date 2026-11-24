// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit 25979b15a419d4457fb67ea5e10c1a15d2e56b62), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Time stretch that keeps the pitch: WSOLA (waveform-similarity overlap-add).
//!
//! The output is made of grains two hops long, overlapped by one hop with a raised-cosine window
//! (the falling half of one grain plus the rising half of the next always sums to one). Grain `k`
//! is read from the input near its nominal position (where the speed puts output hop `k`), shifted
//! by at most the tolerance to the position whose first hop best matches the previous grain's
//! natural continuation: the input that would have followed that grain at speed 1. The two halves
//! that are crossfaded are then in phase, so a steady tone keeps its level and its pitch.
//!
//! [`Wsola`] holds the sizes and the window; the caller keeps the chain of grain positions (each
//! one follows from the one before, see [`Wsola::align`]) and reads the input however it is
//! stored. Nothing here allocates per call or keeps state between calls.

/// WSOLA sizes and window for one sample rate.
#[derive(Clone, Debug)]
pub struct Wsola {
    hop: usize,
    tolerance: usize,
    /// The window's rising half, `hop` samples; the falling half is `1 - rise`.
    rise: Vec<f32>,
}

impl Wsola {
    /// Sizes for `sample_rate`: 15 ms hops (30 ms grains) and an 8 ms search either side, which
    /// covers a period of every pitch above 62 Hz.
    pub fn new(sample_rate: u32) -> Self {
        let sr = f64::from(sample_rate.clamp(1_000, 768_000));
        let hop = ((sr * 0.015).round() as usize).max(16);
        let tolerance = ((sr * 0.008).round() as usize).max(4);
        let rise = (0..hop)
            .map(|t| {
                let s = (std::f64::consts::FRAC_PI_2 * (t as f64 + 0.5) / hop as f64).sin();
                (s * s) as f32
            })
            .collect();
        Wsola { hop, tolerance, rise }
    }

    /// Output samples between grains (half a grain).
    pub fn hop(&self) -> usize {
        self.hop
    }

    /// How far (samples) a grain may move from its nominal position.
    pub fn tolerance(&self) -> usize {
        self.tolerance
    }

    /// Where to read the next grain: the position within the tolerance of `nominal` whose first
    /// hop of `guide` best matches the hop at `continuation` (the previous grain's position plus
    /// one hop). Positions index `guide` (a mono mix of the input); candidates whose hop would
    /// leave it are skipped, and `nominal` is returned when there is nothing to match (the
    /// continuation is outside `guide`, or silent). Ties go to the candidate closest to `nominal`.
    pub fn align(&self, guide: &[f32], continuation: i64, nominal: i64) -> i64 {
        let h = self.hop;
        let len = guide.len() as i64;
        let window = |p: i64| -> Option<&[f32]> {
            if p < 0 || p.checked_add(h as i64)? > len {
                return None;
            }
            guide.get(p as usize..p as usize + h)
        };
        let Some(template) = window(continuation) else { return nominal };
        if template.iter().all(|x| x.abs() < 1e-9) {
            return nominal;
        }
        // normalized cross-correlation (the template's own energy is the same for every candidate)
        let score = |p: i64, step: usize| -> Option<f64> {
            let cand = window(p)?;
            let (mut xy, mut yy) = (0.0f64, 0.0f64);
            for (x, y) in template.iter().zip(cand).step_by(step) {
                xy += f64::from(*x) * f64::from(*y);
                yy += f64::from(*y) * f64::from(*y);
            }
            Some(if yy > 1e-18 { xy / yy.sqrt() } else { 0.0 })
        };
        let tol = self.tolerance as i64;
        // coarse pass on every other offset and sample, nearest offsets first so ties keep them;
        // then the neighbours of the best at full resolution
        let consider = |d: i64, step: usize, best: &mut Option<(i64, f64)>| {
            if let Some(s) = score(nominal.saturating_add(d), step)
                && best.is_none_or(|(_, b)| s > b)
            {
                *best = Some((d, s));
            }
        };
        let mut best = None;
        let mut d = 0;
        while d <= tol {
            consider(d, 2, &mut best);
            if d > 0 {
                consider(-d, 2, &mut best);
            }
            d += 2;
        }
        let Some((coarse, _)) = best else { return nominal };
        let mut fine = None;
        for d in [coarse, coarse - 1, coarse + 1] {
            if d.abs() <= tol {
                consider(d, 1, &mut fine);
            }
        }
        nominal.saturating_add(fine.map_or(coarse, |(d, _)| d))
    }

    /// One output hop: the previous grain's falling half (`prev`, the samples one hop into it)
    /// crossfaded into the next grain's rising half (`cur`). Writes `out.len()` samples, at most a
    /// hop; missing input reads as silence.
    pub fn overlap(&self, prev: &[f32], cur: &[f32], out: &mut [f32]) {
        for (t, o) in out.iter_mut().enumerate().take(self.hop) {
            let r = self.rise(t);
            let a = prev.get(t).copied().unwrap_or(0.0);
            let b = cur.get(t).copied().unwrap_or(0.0);
            *o = a * (1.0 - r) + b * r;
        }
    }

    /// The window's weight for the next grain at sample `t` of a hop (the previous grain gets one
    /// minus this).
    pub fn rise(&self, t: usize) -> f32 {
        self.rise.get(t).copied().unwrap_or(1.0)
    }
}


