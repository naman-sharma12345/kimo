// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit 25979b15a419d4457fb67ea5e10c1a15d2e56b62), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Pitch shifter: phase vocoder with peak-region shifting and identity phase locking.
//!
//! Every STFT frame (8× overlap) is analysed for spectral peaks; each peak's region of influence
//! (down to the magnitude minima between peaks) is moved as a block so that the peak lands on
//! `k·ratio`, preserving the window's main-lobe shape. All bins of a region receive the same phase
//! rotation, accumulated per frame from the peak's instantaneous frequency so the partial's
//! frequency is scaled exactly by `ratio` while the within-lobe phase relations stay coherent
//! (low phasiness). Duration is unchanged. At ratio 1 the output is the input delayed by the
//! frame size.

use super::stft::{Pairing, Plan, Spectral, StftChannel, fft_size, stream};
use crate::{AudioEffect, ParamSpec, ParamValues, Unit, block_len, param_plumbing};
use std::f32::consts::PI;

const OVERLAP: usize = 8;

#[derive(Clone, Debug)]
struct PvState {
    /// Previous frame's analysis spectrum (bins 0..=n/2); its phase is only needed at peaks.
    last_re: Vec<f32>,
    last_im: Vec<f32>,
    /// Accumulated phase rotation per output bin (from the previous frame).
    rot: Vec<f32>,
    new_rot: Vec<f32>,
}

/// Frame scratch shared by all channels.
#[derive(Clone, Debug)]
struct Scratch {
    mag: Vec<f32>,
    out_re: Vec<f32>,
    out_im: Vec<f32>,
    peaks: Vec<usize>,
}

/// Semitone pitch shifter (±12 st, fractional allowed).
pub struct PitchShifter {
    pv: ParamValues,
    n: usize,
    hop: usize,
    spec: Spectral,
    stft: Vec<StftChannel>,
    state: Vec<PvState>,
    scratch: Scratch,
    target_ratio: f32,
    /// Ratio used by the current frame (glides per frame towards the target).
    ratio: f32,
}

/// Wrap a phase to (−π, π].
#[inline]
fn wrap(p: f32) -> f32 {
    p - 2.0 * PI * ((p + PI) / (2.0 * PI)).floor()
}

impl PitchShifter {
    pub const PARAMS: &'static [ParamSpec] = &[ParamSpec::new("semitones", "Pitch", -12.0, 12.0, 0.0, Unit::Semitones)];

    pub fn new(sample_rate: f32, channels: usize) -> Self {
        let n = fft_size(sample_rate);
        let bins = n / 2 + 1;
        let ch = channels.max(1);
        let mut s = PitchShifter {
            pv: ParamValues::new(Self::PARAMS),
            n,
            hop: n / OVERLAP,
            spec: Spectral::new(n, false),
            stft: vec![StftChannel::new(n); ch],
            state: vec![PvState { last_re: vec![0.0; bins], last_im: vec![0.0; bins], rot: vec![0.0; bins], new_rot: vec![0.0; bins] }; ch],
            scratch: Scratch { mag: vec![0.0; bins], out_re: vec![0.0; bins], out_im: vec![0.0; bins], peaks: Vec::with_capacity(bins) },
            target_ratio: 1.0,
            ratio: 1.0,
        };
        s.apply_params(true);
        s
    }

    fn apply_params(&mut self, snap: bool) {
        self.target_ratio = 2f32.powf(self.pv.v("semitones") / 12.0);
        if snap {
            self.ratio = self.target_ratio;
        }
    }
}

/// Shift one analysed half spectrum (bins 0..=n/2) in place.
fn shift_frame(spec_re: &mut [f32], spec_im: &mut [f32], st: &mut PvState, sc: &mut Scratch, ratio: f32, expect: f32) {
    let bins = sc.mag.len();
    let mut max_mag = 0.0f32;
    for k in 0..bins {
        let (re, im) = (spec_re[k], spec_im[k]);
        sc.mag[k] = (re * re + im * im).sqrt();
        max_mag = max_mag.max(sc.mag[k]);
        sc.out_re[k] = 0.0;
        sc.out_im[k] = 0.0;
        st.new_rot[k] = 0.0;
    }
    // Peaks: local maxima over ±2 bins, above −100 dB of the frame maximum.
    sc.peaks.clear();
    let floor = max_mag * 1e-5;
    for k in 0..bins {
        let m = sc.mag[k];
        if m <= floor {
            continue;
        }
        let lo = k.saturating_sub(2);
        let hi = (k + 2).min(bins - 1);
        if (lo..k).all(|j| sc.mag[j] < m) && (k + 1..=hi).all(|j| sc.mag[j] <= m) {
            sc.peaks.push(k);
        }
    }
    let np = sc.peaks.len();
    let mut start = 0usize;
    for i in 0..np {
        let p = sc.peaks[i];
        // Region end: magnitude minimum between this peak and the next.
        let end = if i + 1 < np {
            let q = sc.peaks[i + 1];
            let mut mi = p + 1;
            for j in p + 1..q {
                if sc.mag[j] < sc.mag[mi] {
                    mi = j;
                }
            }
            mi
        } else {
            bins
        };
        // Instantaneous frequency of the peak (bins).
        let phase = spec_im[p].atan2(spec_re[p]);
        let last_phase = st.last_im[p].atan2(st.last_re[p]);
        let d = wrap(phase - last_phase - p as f32 * expect);
        let f = p as f32 + d / expect;
        let target = (p as f32 * ratio).round() as isize;
        let delta = target - p as isize;
        let prev_rot = if (0..bins as isize).contains(&target) { st.rot[target as usize] } else { 0.0 };
        let rot = wrap(prev_rot + (ratio - 1.0) * f * expect);
        // A NaN or inf input sample leaves a non-finite previous spectrum; `rot` is accumulated
        // across frames, so it must not keep the NaN.
        let rot = if rot.is_finite() { rot } else { 0.0 };
        // mag·e^{i(φ+rot)} = X·e^{i·rot}: one sin_cos per region instead of one per bin.
        let (s, c) = rot.sin_cos();
        for k in start..end {
            let t = k as isize + delta;
            if t < 0 || t >= bins as isize {
                continue;
            }
            let t = t as usize;
            let (re, im) = (spec_re[k], spec_im[k]);
            sc.out_re[t] += re * c - im * s;
            sc.out_im[t] += re * s + im * c;
            st.new_rot[t] = rot;
        }
        start = end;
    }
    st.last_re.copy_from_slice(spec_re);
    st.last_im.copy_from_slice(spec_im);
    spec_re.copy_from_slice(&sc.out_re);
    spec_im.copy_from_slice(&sc.out_im);
    std::mem::swap(&mut st.rot, &mut st.new_rot);
}

impl AudioEffect for PitchShifter {
    param_plumbing!("pitch_shifter");
    fn latency(&self) -> usize {
        self.n
    }
    fn reset(&mut self) {
        self.stft.iter_mut().for_each(StftChannel::reset);
        for s in &mut self.state {
            s.last_re.iter_mut().for_each(|v| *v = 0.0);
            s.last_im.iter_mut().for_each(|v| *v = 0.0);
            s.rot.iter_mut().for_each(|v| *v = 0.0);
        }
        self.ratio = self.target_ratio;
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        let len = block_len(channels);
        let (n, hop) = (self.n, self.hop);
        // Hann² summed over 8× overlap = 3/8 · 8 = 3.
        let scale = 1.0 / (n as f32 * 3.0 / 8.0 * OVERLAP as f32);
        let expect = 2.0 * PI * hop as f32 / n as f32;
        // Sample-major so every channel hits its frame boundary on the same sample and the ratio
        // glides identically for all channels (once per frame).
        let (state, sc, target_ratio) = (&mut self.state, &mut self.scratch, self.target_ratio);
        let mut ratio = self.ratio;
        let mut frame_ratio = ratio;
        stream(&mut self.stft, &mut self.spec, channels, len, Plan { hop, scale, pairing: Pairing::SynthesisOnly }, &mut |ch, first, re, im| {
            if first {
                frame_ratio = ratio;
                ratio += (target_ratio - ratio) * 0.5;
                if (ratio - target_ratio).abs() < 1e-5 {
                    ratio = target_ratio;
                }
            }
            shift_frame(re, im, &mut state[ch], sc, frame_ratio, expect);
        });
        self.ratio = ratio;
    }
}


