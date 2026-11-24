// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit 25979b15a419d4457fb67ea5e10c1a15d2e56b62), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Essential Sound processors: DeEsser (split-band), DeReverb (spectral late-reverberation
//! suppression), Enhance Speech (a DSP voice-enhancement chain) and Stereo Width (mid/side).
//!
//! All written from textbook descriptions:
//! * de-esser: complementary split (band = 2nd-order high-pass, rest = x − band) with a relative
//!   band-vs-broadband detector driving the band gain;
//! * de-reverb: the statistical late-reverberation model (exponentially decaying diffuse tail,
//!   Polack): the late-reverb PSD at frame t is predicted from the reverberant PSD T_d earlier,
//!   λ_r(t) = e^{−2Δ·T_d}·λ_x(t − T_d), Δ = 3·ln10 / RT60, and removed by spectral subtraction
//!   with over-subtraction and a gain floor (Lebart et al. 2001; Habets 2007);
//! * enhance speech: high-pass, de-mud cut, presence and air boosts (RBJ biquads), then one gain
//!   computer combining a downward expander (noise between words) and a soft-knee compressor.

use super::stft::{Pairing, Plan, Spectral, StftChannel, fft_size, stream};
use crate::biquad::{Biquad, Coeffs, FilterType};
use crate::{AudioEffect, ParamSpec, ParamValues, Smoothed, Unit, block_len, db_to_gain, flush, param_plumbing};

/// One-pole smoothing coefficient for a time constant in ms.
fn coef(ms: f64, sr: f64) -> f64 {
    if ms <= 0.0 { 0.0 } else { (-1.0 / (ms * 0.001 * sr)).exp() }
}

#[inline]
fn pow_db(p: f64) -> f64 {
    10.0 * (p + 1e-20).log10()
}

// ---------------------------------------------------------------------------------------------

/// De-esser: a 2nd-order high-pass sidechain at `frequency` detects sibilance (the band level
/// exceeds the broadband level by more than `threshold`); the band above `frequency` is then cut
/// by a dynamic RBJ high shelf whose gain follows the gain reduction (attack ~1 ms, release
/// ~60 ms, stereo-linked). At 0 dB reduction the shelf is an exact identity, so non-sibilant
/// passages pass unchanged. (A subtractive split `x − HP(x)` is not used: the high-pass phase
/// shift around the crossover makes `x − (1 − g)·HP(x)` boost there instead of cutting.)
pub struct DeEsser {
    pv: ParamValues,
    sr: f64,
    freq: Smoothed,
    coeffs: Coeffs,
    hp: Vec<Biquad>,
    shelf: Vec<Biquad>,
    shelf_coeffs: Coeffs,
    shelf_gr: f64,
    env_band: f64,
    env_full: f64,
    /// Smoothed gain reduction (dB, ≤ 0).
    gr: f64,
    det: f64,
    att: f64,
    rel: f64,
    threshold: f64,
    reduction: f64,
    gr_meter: f32,
}

impl DeEsser {
    pub const PARAMS: &'static [ParamSpec] = &[
        ParamSpec::log("frequency", "Frequency", 2000.0, 12000.0, 6000.0, Unit::Hertz),
        ParamSpec::new("threshold", "Threshold", -40.0, 0.0, -12.0, Unit::Decibels),
        ParamSpec::new("reduction", "Max Reduction", 0.0, 24.0, 8.0, Unit::Decibels),
    ];

    pub fn new(sample_rate: f32, channels: usize) -> Self {
        let sr = sample_rate as f64;
        let mut s = DeEsser {
            pv: ParamValues::new(Self::PARAMS),
            sr,
            freq: Smoothed::with_ms(6000.0, sample_rate, 20.0),
            coeffs: Coeffs::IDENTITY,
            hp: vec![Biquad::new(); channels.max(1)],
            shelf: vec![Biquad::new(); channels.max(1)],
            shelf_coeffs: Coeffs::IDENTITY,
            shelf_gr: 0.0,
            env_band: 0.0,
            env_full: 0.0,
            gr: 0.0,
            det: coef(2.0, sr),
            att: coef(1.0, sr),
            rel: coef(60.0, sr),
            threshold: -12.0,
            reduction: 8.0,
            gr_meter: 0.0,
        };
        s.apply_params(true);
        s
    }

    fn design(&mut self) {
        self.coeffs = Coeffs::design(FilterType::HighPass, self.sr, self.freq.value() as f64, std::f64::consts::FRAC_1_SQRT_2, 0.0);
        self.design_shelf();
    }

    fn design_shelf(&mut self) {
        self.shelf_coeffs = if self.gr == 0.0 {
            Coeffs::IDENTITY
        } else {
            Coeffs::design(FilterType::HighShelf, self.sr, self.freq.value() as f64, std::f64::consts::FRAC_1_SQRT_2, self.gr)
        };
        self.shelf_gr = self.gr;
    }

    fn apply_params(&mut self, snap: bool) {
        self.freq.set(self.pv.v("frequency"));
        self.threshold = self.pv.v("threshold") as f64;
        self.reduction = self.pv.v("reduction") as f64;
        if snap {
            self.freq.snap();
            self.design();
        }
    }

    /// Current band gain reduction in dB (≤ 0), for metering.
    pub fn gain_reduction_db(&self) -> f32 {
        self.gr_meter
    }
}

impl AudioEffect for DeEsser {
    param_plumbing!("deesser");
    fn reset(&mut self) {
        self.freq.snap();
        self.design();
        self.hp.iter_mut().for_each(Biquad::reset);
        self.shelf.iter_mut().for_each(Biquad::reset);
        self.env_band = 0.0;
        self.env_full = 0.0;
        self.gr = 0.0;
        self.gr_meter = 0.0;
        self.design_shelf();
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        let n = block_len(channels);
        let nch = channels.len().min(self.hp.len());
        for i in 0..n {
            if self.freq.is_smoothing() {
                self.freq.tick();
                self.design();
            }
            let (mut pb, mut pf) = (0.0f64, 0.0f64);
            for ch in 0..nch {
                let x = channels[ch][i] as f64;
                let b = self.hp[ch].tick(&self.coeffs, x);
                pb += b * b;
                pf += x * x;
            }
            self.env_band = flush(self.det * self.env_band + (1.0 - self.det) * pb);
            self.env_full = flush(self.det * self.env_full + (1.0 - self.det) * pf);
            let bdb = pow_db(self.env_band);
            let excess = bdb - pow_db(self.env_full) - self.threshold;
            let target = if bdb > -70.0 && excess > 0.0 { -(excess * 2.0).min(self.reduction) } else { 0.0 };
            let c = if target < self.gr { self.att } else { self.rel };
            self.gr = flush(c * self.gr + (1.0 - c) * target);
            if target == 0.0 && self.gr > -1e-4 {
                self.gr = 0.0;
            }
            if (self.gr - self.shelf_gr).abs() > 0.05 || (self.gr == 0.0) != (self.shelf_gr == 0.0) {
                self.design_shelf();
            }
            for ch in 0..nch {
                let x = channels[ch][i] as f64;
                channels[ch][i] = self.shelf[ch].tick(&self.shelf_coeffs, x) as f32;
            }
        }
        self.gr_meter = self.gr as f32;
    }
}

// ---------------------------------------------------------------------------------------------

/// Late-reverberation delay T_d (s): reflections later than this count as reverb.
const DEREVERB_TD: f64 = 0.05;
/// Time constant (s) of the PSD smoothing.
const DEREVERB_PSD_TAU: f64 = 0.02;

#[derive(Clone, Debug)]
struct ReverbState {
    psd: Vec<f32>,
    /// Ring of past smoothed PSDs, `delay_frames` deep.
    hist: Vec<f32>,
    pos: usize,
    gains: Vec<f32>,
    frames: usize,
}

impl ReverbState {
    fn new(bins: usize, delay: usize) -> Self {
        ReverbState { psd: vec![0.0; bins], hist: vec![0.0; bins * delay], pos: 0, gains: vec![1.0; bins], frames: 0 }
    }
    fn reset(&mut self) {
        self.psd.iter_mut().for_each(|v| *v = 0.0);
        self.hist.iter_mut().for_each(|v| *v = 0.0);
        self.gains.iter_mut().for_each(|v| *v = 1.0);
        self.pos = 0;
        self.frames = 0;
    }
}

/// Spectral de-reverberation: suppresses the late reverberant tail predicted by the exponential
/// decay model for the assumed `rt60`. `amount` sets the over-subtraction and the gain floor
/// (0 % = identity apart from the STFT delay, 100 % ≈ −18 dB floor).
pub struct DeReverb {
    pv: ParamValues,
    sr: f32,
    n: usize,
    hop: usize,
    delay: usize,
    spec: Spectral,
    stft: Vec<StftChannel>,
    state: Vec<ReverbState>,
    smoothed_gains: Vec<f32>,
    /// e^{−2Δ·T_d} (power decay over the delay).
    decay: f32,
    beta: f32,
    floor: f32,
    alpha: f32,
}

impl DeReverb {
    pub const PARAMS: &'static [ParamSpec] =
        &[ParamSpec::new("amount", "Amount", 0.0, 100.0, 50.0, Unit::Percent), ParamSpec::log("rt60", "Decay (RT60)", 0.1, 5.0, 0.8, Unit::Seconds)];

    pub fn new(sample_rate: f32, channels: usize) -> Self {
        let n = fft_size(sample_rate);
        let hop = n / 4;
        let bins = n / 2 + 1;
        let delay = ((DEREVERB_TD * sample_rate as f64 / hop as f64).round() as usize).max(1);
        let ch = channels.max(1);
        let mut s = DeReverb {
            pv: ParamValues::new(Self::PARAMS),
            sr: sample_rate,
            n,
            hop,
            delay,
            spec: Spectral::new(n, true),
            stft: vec![StftChannel::new(n); ch],
            state: vec![ReverbState::new(bins, delay); ch],
            smoothed_gains: vec![1.0; bins],
            decay: 1.0,
            beta: 1.0,
            floor: 1.0,
            alpha: (-(hop as f64) / (sample_rate as f64 * DEREVERB_PSD_TAU)).exp() as f32,
        };
        s.apply_params(true);
        s
    }

    fn apply_params(&mut self, _snap: bool) {
        let amount = self.pv.v("amount") / 100.0;
        let rt60 = self.pv.v("rt60") as f64;
        let delta = 3.0 * std::f64::consts::LN_10 / rt60;
        let td = self.delay as f64 * self.hop as f64 / self.sr as f64;
        self.decay = (-2.0 * delta * td).exp() as f32;
        self.beta = 1.0 + amount;
        self.floor = db_to_gain(-18.0 * amount);
    }
}

impl AudioEffect for DeReverb {
    param_plumbing!("dereverb");
    fn latency(&self) -> usize {
        self.n
    }
    fn reset(&mut self) {
        self.stft.iter_mut().for_each(StftChannel::reset);
        self.state.iter_mut().for_each(ReverbState::reset);
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        let len = block_len(channels);
        let (n, hop, delay) = (self.n, self.hop, self.delay);
        let (decay, beta, floor, alpha) = (self.decay, self.beta, self.floor, self.alpha);
        let scale = 1.0 / (n as f32 * 2.0);
        let bins = n / 2 + 1;
        let (states, sg) = (&mut self.state, &mut self.smoothed_gains);
        stream(&mut self.stft, &mut self.spec, channels, len, Plan { hop, scale, pairing: Pairing::Full }, &mut |ch, _, re, im| {
            let st = &mut states[ch];
            let warm = st.frames >= delay;
            let base = st.pos * bins;
            for k in 0..bins {
                let p = re[k] * re[k] + im[k] * im[k];
                let psd = alpha * st.psd[k] + (1.0 - alpha) * p;
                // Range test: tiny values and NaN/inf (one bad input sample) clear the state
                // instead of poisoning the recursion and its history ring.
                st.psd[k] = if (1e-30..=f32::MAX).contains(&psd) { psd } else { 0.0 };
                // Late reverb predicted from the PSD `delay` frames ago (the ring slot we are
                // about to overwrite holds exactly that frame).
                let late = if warm { decay * st.hist[base + k] } else { 0.0 };
                st.hist[base + k] = st.psd[k];
                let target = if st.psd[k] > 0.0 { (1.0 - beta * late / st.psd[k]).max(floor) } else { 1.0 };
                let g = &mut st.gains[k];
                // fast recovery on onsets, slower suppression (less musical noise)
                let c = if target > *g { 0.8 } else { 0.5 };
                *g += (target - *g) * c;
            }
            st.pos = (st.pos + 1) % delay;
            st.frames = st.frames.saturating_add(1);
            for k in 0..bins {
                let a = st.gains[k.saturating_sub(1)];
                let c = st.gains[(k + 1).min(bins - 1)];
                sg[k] = 0.25 * a + 0.5 * st.gains[k] + 0.25 * c;
            }
            for k in 0..bins {
                re[k] *= sg[k];
                im[k] *= sg[k];
            }
        });
    }
}

// ---------------------------------------------------------------------------------------------

const ENH_STAGES: usize = 4;
/// Expander: below this level (dBFS) the signal is pushed down (noise between words).
const ENH_EXP_THRESHOLD: f64 = -50.0;
const ENH_EXP_RATIO: f64 = 3.0;
const ENH_EXP_RANGE: f64 = 18.0;
/// Compressor above this level (dBFS).
const ENH_COMP_THRESHOLD: f64 = -24.0;
const ENH_COMP_RATIO: f64 = 3.0;
const ENH_COMP_KNEE: f64 = 6.0;
/// Make-up gain: puts conversational speech (≈ −20 dBFS RMS) back near its input loudness.
const ENH_MAKEUP: f64 = 3.0;

/// Enhance Speech: a DSP voice-enhancement chain (no machine learning) — 80 Hz high-pass,
/// de-mud cut, presence and air boosts, then a combined downward expander + soft-knee
/// compressor with make-up gain. `tone` picks the cut/boost frequencies for lower or higher
/// voices; `mix` blends with the dry signal.
pub struct SpeechEnhance {
    pv: ParamValues,
    sr: f64,
    coeffs: [Coeffs; ENH_STAGES],
    filters: Vec<[Biquad; ENH_STAGES]>,
    mix: Smoothed,
    /// Detector mean square.
    ms: f64,
    /// Smoothed expander gain (dB, ≤ 0): opens fast, closes slowly.
    env_exp: f64,
    /// Smoothed compressor gain (dB, ≤ 0): fast attack, slow release.
    env_comp: f64,
    det: f64,
    fast: f64,
    exp_close: f64,
    comp_rel: f64,
}

impl SpeechEnhance {
    pub const PARAMS: &'static [ParamSpec] =
        &[ParamSpec::new("mix", "Mix", 0.0, 100.0, 100.0, Unit::Percent), ParamSpec::choice("tone", "Tone", &["Low Tone", "High Tone"], 0)];

    pub fn new(sample_rate: f32, channels: usize) -> Self {
        let sr = sample_rate as f64;
        let mut s = SpeechEnhance {
            pv: ParamValues::new(Self::PARAMS),
            sr,
            coeffs: [Coeffs::IDENTITY; ENH_STAGES],
            filters: vec![[Biquad::new(); ENH_STAGES]; channels.max(1)],
            mix: Smoothed::with_ms(1.0, sample_rate, 20.0),
            ms: 0.0,
            env_exp: 0.0,
            env_comp: 0.0,
            det: coef(5.0, sr),
            fast: coef(3.0, sr),
            exp_close: coef(30.0, sr),
            comp_rel: coef(120.0, sr),
        };
        s.apply_params(true);
        s
    }

    fn apply_params(&mut self, snap: bool) {
        let high = self.pv.idx("tone") == 1;
        let (mud, presence) = if high { (350.0, 4000.0) } else { (250.0, 2500.0) };
        let sr = self.sr;
        self.coeffs = [
            Coeffs::design(FilterType::HighPass, sr, 80.0, std::f64::consts::FRAC_1_SQRT_2, 0.0),
            Coeffs::design(FilterType::Peaking, sr, mud, 1.0, -3.0),
            Coeffs::design(FilterType::Peaking, sr, presence, 1.0, 4.0),
            Coeffs::design(FilterType::HighShelf, sr, 10000.0, std::f64::consts::FRAC_1_SQRT_2, 2.0),
        ];
        self.mix.set(self.pv.v("mix") / 100.0);
        if snap {
            self.mix.snap();
        }
    }

    /// Static downward-expander gain (dB) for a detector level (dBFS).
    fn expander_db(level: f64) -> f64 {
        if level < ENH_EXP_THRESHOLD { ((level - ENH_EXP_THRESHOLD) * (ENH_EXP_RATIO - 1.0)).max(-ENH_EXP_RANGE) } else { 0.0 }
    }

    /// Static soft-knee compressor gain (dB) for a detector level (dBFS).
    fn compressor_db(level: f64) -> f64 {
        let d = level - ENH_COMP_THRESHOLD;
        if 2.0 * d.abs() <= ENH_COMP_KNEE {
            (1.0 / ENH_COMP_RATIO - 1.0) * (d + ENH_COMP_KNEE / 2.0).powi(2) / (2.0 * ENH_COMP_KNEE)
        } else if d > 0.0 {
            d / ENH_COMP_RATIO - d
        } else {
            0.0
        }
    }
}

impl AudioEffect for SpeechEnhance {
    param_plumbing!("speech_enhance");
    fn reset(&mut self) {
        self.filters.iter_mut().for_each(|f| f.iter_mut().for_each(Biquad::reset));
        self.mix.snap();
        self.ms = 0.0;
        self.env_exp = 0.0;
        self.env_comp = 0.0;
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        let n = block_len(channels);
        let nch = channels.len().min(self.filters.len()).min(8);
        let mut wet = [0.0f64; 8];
        for i in 0..n {
            let mut sq = 0.0f64;
            for ch in 0..nch {
                let mut x = channels[ch][i] as f64;
                for (st, c) in self.filters[ch].iter_mut().zip(self.coeffs.iter()) {
                    x = st.tick(c, x);
                }
                wet[ch] = x;
                sq = sq.max(x * x);
            }
            self.ms = flush(self.det * self.ms + (1.0 - self.det) * sq);
            let level = pow_db(self.ms);
            let te = Self::expander_db(level);
            let c = if te > self.env_exp { self.fast } else { self.exp_close };
            self.env_exp = c * self.env_exp + (1.0 - c) * te;
            let tc = Self::compressor_db(level);
            let c = if tc < self.env_comp { self.fast } else { self.comp_rel };
            self.env_comp = c * self.env_comp + (1.0 - c) * tc;
            if self.env_exp.abs() < 1e-9 {
                self.env_exp = 0.0;
            }
            if self.env_comp.abs() < 1e-9 {
                self.env_comp = 0.0;
            }
            let g = 10f64.powf((self.env_exp + self.env_comp + ENH_MAKEUP) / 20.0);
            let m = self.mix.tick() as f64;
            for (ch, w) in wet.iter().enumerate().take(nch) {
                let dry = channels[ch][i] as f64;
                channels[ch][i] = (dry + m * (w * g - dry)) as f32;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------

/// Stereo width by mid/side scaling: 0 % = mono, 100 % = unchanged, 200 % = side doubled.
/// Only the first two channels are processed; mono input passes through.
pub struct StereoWidth {
    pv: ParamValues,
    width: Smoothed,
}

impl StereoWidth {
    pub const PARAMS: &'static [ParamSpec] = &[ParamSpec::new("width", "Width", 0.0, 200.0, 100.0, Unit::Percent)];

    pub fn new(sample_rate: f32, _channels: usize) -> Self {
        let mut s = StereoWidth { pv: ParamValues::new(Self::PARAMS), width: Smoothed::with_ms(1.0, sample_rate, 20.0) };
        s.apply_params(true);
        s
    }
    fn apply_params(&mut self, snap: bool) {
        self.width.set(self.pv.v("width") / 100.0);
        if snap {
            self.width.snap();
        }
    }
}

impl AudioEffect for StereoWidth {
    param_plumbing!("stereo_width");
    fn reset(&mut self) {
        self.width.snap();
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        let n = block_len(channels);
        if channels.len() < 2 {
            // keep the smoother in step with time
            for _ in 0..n {
                self.width.tick();
            }
            return;
        }
        let (l, rest) = channels.split_at_mut(1);
        let (l, r) = (&mut l[0], &mut rest[0]);
        for i in 0..n {
            let w = self.width.tick();
            let m = 0.5 * (l[i] + r[i]);
            let s = 0.5 * (l[i] - r[i]) * w;
            l[i] = m + s;
            r[i] = m - s;
        }
    }
}


