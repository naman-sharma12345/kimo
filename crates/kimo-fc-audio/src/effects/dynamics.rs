// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit 25979b15a419d4457fb67ea5e10c1a15d2e56b62), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Dynamics: compressor, expander/gate, look-ahead true-peak limiter.
//!
//! Detection is stereo-linked (the loudest channel drives one common gain) so the image does
//! not shift. Gain computers work in dB; attack/release are one-pole smoothers on the gain.

use super::util::{coef, lin_db};
use crate::oversample::{Interpolator, PolyphaseBank, true_peak_factor};
use crate::{AudioEffect, ParamSpec, ParamValues, Smoothed, Unit, block_len, db_to_gain, param_plumbing};
use std::collections::VecDeque;

/// Static compressor curve with a quadratic soft knee (output level in dB for input level `x`).
pub fn compressor_curve(x: f32, threshold: f32, ratio: f32, knee: f32) -> f32 {
    let d = x - threshold;
    if knee > 0.0 && 2.0 * d.abs() <= knee {
        x + (1.0 / ratio - 1.0) * (d + knee / 2.0).powi(2) / (2.0 * knee)
    } else if d > 0.0 {
        threshold + d / ratio
    } else {
        x
    }
}

/// Static downward-expander curve (output level in dB), attenuation limited to `range` dB.
pub fn expander_curve(x: f32, threshold: f32, ratio: f32, range: f32) -> f32 {
    if x >= threshold { x } else { x + ((x - threshold) * (ratio - 1.0)).max(-range) }
}

// ---------------------------------------------------------------------------------------------

/// Feed-forward compressor (peak or RMS detector, soft knee, attack/release, make-up gain).
pub struct Compressor {
    pv: ParamValues,
    sr: f32,
    threshold: f32,
    ratio: f32,
    knee: f32,
    rms: bool,
    att: f32,
    rel: f32,
    rms_coef: f32,
    makeup: Smoothed,
    /// Detector mean square (RMS mode).
    ms: f32,
    /// Smoothed gain reduction (dB, ≤ 0).
    env: f32,
    gr_meter: f32,
}

impl Compressor {
    pub const PARAMS: &'static [ParamSpec] = &[
        ParamSpec::new("threshold", "Threshold", -60.0, 0.0, -20.0, Unit::Decibels),
        ParamSpec::new("ratio", "Ratio", 1.0, 30.0, 4.0, Unit::Ratio),
        ParamSpec::new("knee", "Knee", 0.0, 24.0, 6.0, Unit::Decibels),
        ParamSpec::log("attack", "Attack", 0.01, 300.0, 10.0, Unit::Milliseconds),
        ParamSpec::log("release", "Release", 1.0, 3000.0, 100.0, Unit::Milliseconds),
        ParamSpec::new("makeup", "Make-up Gain", -40.0, 40.0, 0.0, Unit::Decibels),
        ParamSpec::choice("detector", "Detector", &["Peak", "RMS"], 0),
    ];

    pub fn new(sample_rate: f32, _channels: usize) -> Self {
        let mut s = Compressor {
            pv: ParamValues::new(Self::PARAMS),
            sr: sample_rate,
            threshold: 0.0,
            ratio: 1.0,
            knee: 0.0,
            rms: false,
            att: 0.0,
            rel: 0.0,
            rms_coef: coef(10.0, sample_rate),
            makeup: Smoothed::with_ms(1.0, sample_rate, 20.0),
            ms: 0.0,
            env: 0.0,
            gr_meter: 0.0,
        };
        s.apply_params(true);
        s
    }

    fn apply_params(&mut self, snap: bool) {
        self.threshold = self.pv.v("threshold");
        self.ratio = self.pv.v("ratio");
        self.knee = self.pv.v("knee");
        self.att = coef(self.pv.v("attack"), self.sr);
        self.rel = coef(self.pv.v("release"), self.sr);
        self.rms = self.pv.idx("detector") == 1;
        self.makeup.set(db_to_gain(self.pv.v("makeup")));
        if snap {
            self.makeup.snap();
        }
    }

    /// Static curve at the current settings.
    pub fn curve(&self, x_db: f32) -> f32 {
        compressor_curve(x_db, self.threshold, self.ratio, self.knee)
    }

    /// Current gain reduction in dB (≤ 0), for metering.
    pub fn gain_reduction_db(&self) -> f32 {
        self.gr_meter
    }
}

impl AudioEffect for Compressor {
    param_plumbing!("compressor");
    fn transfer_db(&self, band: usize, input_db: f32) -> Option<f32> {
        (band == 0).then(|| self.curve(input_db) + self.pv.v("makeup"))
    }
    fn reset(&mut self) {
        self.makeup.snap();
        self.ms = 0.0;
        self.env = 0.0;
        self.gr_meter = 0.0;
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        let n = block_len(channels);
        for i in 0..n {
            let mut peak = 0.0f32;
            let mut sq = 0.0f32;
            for ch in channels.iter() {
                let x = ch[i];
                peak = peak.max(x.abs());
                sq = sq.max(x * x);
            }
            let level_db = if self.rms {
                self.ms = self.rms_coef * self.ms + (1.0 - self.rms_coef) * sq;
                if !(1e-30..=f32::MAX).contains(&self.ms) {
                    self.ms = 0.0;
                }
                10.0 * self.ms.max(1e-20).log10()
            } else {
                lin_db(peak)
            };
            let target = self.curve(level_db) - level_db;
            let c = if target < self.env { self.att } else { self.rel };
            self.env = c * self.env + (1.0 - c) * target;
            if !(1e-9..=f32::MAX).contains(&self.env.abs()) {
                self.env = 0.0;
            }
            let g = db_to_gain(self.env) * self.makeup.tick();
            for ch in channels.iter_mut() {
                ch[i] *= g;
            }
        }
        self.gr_meter = self.env;
    }
}

// ---------------------------------------------------------------------------------------------

/// Downward expander / noise gate with hold (ratio ≥ 10 behaves as a gate).
pub struct Gate {
    pv: ParamValues,
    sr: f32,
    threshold: f32,
    ratio: f32,
    range: f32,
    att: f32,
    rel: f32,
    hold: u32,
    hold_left: u32,
    env: f32,
}

impl Gate {
    pub const PARAMS: &'static [ParamSpec] = &[
        ParamSpec::new("threshold", "Threshold", -90.0, 0.0, -40.0, Unit::Decibels),
        ParamSpec::log("ratio", "Ratio", 1.0, 100.0, 10.0, Unit::Ratio),
        ParamSpec::new("range", "Range", 0.0, 90.0, 60.0, Unit::Decibels),
        ParamSpec::log("attack", "Attack", 0.01, 100.0, 1.0, Unit::Milliseconds),
        ParamSpec::new("hold", "Hold", 0.0, 2000.0, 50.0, Unit::Milliseconds),
        ParamSpec::log("release", "Release", 1.0, 3000.0, 150.0, Unit::Milliseconds),
    ];

    pub fn new(sample_rate: f32, _channels: usize) -> Self {
        let mut s = Gate {
            pv: ParamValues::new(Self::PARAMS),
            sr: sample_rate,
            threshold: 0.0,
            ratio: 1.0,
            range: 0.0,
            att: 0.0,
            rel: 0.0,
            hold: 0,
            hold_left: 0,
            env: 0.0,
        };
        s.apply_params(true);
        s
    }

    fn apply_params(&mut self, _snap: bool) {
        self.threshold = self.pv.v("threshold");
        self.ratio = self.pv.v("ratio");
        self.range = self.pv.v("range");
        self.att = coef(self.pv.v("attack"), self.sr);
        self.rel = coef(self.pv.v("release"), self.sr);
        self.hold = (self.pv.v("hold") * 0.001 * self.sr) as u32;
    }

    pub fn curve(&self, x_db: f32) -> f32 {
        expander_curve(x_db, self.threshold, self.ratio, self.range)
    }
}

impl AudioEffect for Gate {
    param_plumbing!("gate");
    fn transfer_db(&self, band: usize, input_db: f32) -> Option<f32> {
        (band == 0).then(|| self.curve(input_db))
    }
    fn reset(&mut self) {
        self.env = 0.0;
        self.hold_left = 0;
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        let n = block_len(channels);
        for i in 0..n {
            let peak = channels.iter().fold(0.0f32, |a, c| a.max(c[i].abs()));
            let level = lin_db(peak);
            let target = self.curve(level) - level;
            if target >= self.env {
                // Opening (or staying open): attack.
                self.env = self.att * self.env + (1.0 - self.att) * target;
                if target >= 0.0 {
                    self.hold_left = self.hold;
                }
            } else if self.hold_left > 0 {
                self.hold_left -= 1;
            } else {
                self.env = self.rel * self.env + (1.0 - self.rel) * target;
                if !self.env.is_finite() {
                    self.env = 0.0;
                }
            }
            let g = db_to_gain(self.env);
            for ch in channels.iter_mut() {
                ch[i] *= g;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------

/// Look-ahead brick-wall limiter.
///
/// The required gain for each (optionally true-peak) detected sample is held for the look-ahead
/// window with a sliding minimum, released with a one-pole, and then smoothed with a moving
/// average exactly as long as the look-ahead, so the gain has reached its target when the peak
/// leaves the delay line. A final clip at the ceiling guarantees the sample peak never exceeds it.
pub struct Limiter {
    pv: ParamValues,
    sr: f32,
    in_gain: Smoothed,
    ceiling: f32,
    rel: f32,
    true_peak: bool,
    /// Look-ahead (samples) and detector alignment delay.
    look: usize,
    det_delay: usize,
    bank: PolyphaseBank,
    interp: Vec<Interpolator>,
    /// Per-channel delay line (ring), length `look + det_delay + 1`.
    delay: Vec<Vec<f32>>,
    wpos: usize,
    /// Monotonic deque for the sliding minimum: (sample index, gain).
    minq: VecDeque<(u64, f32)>,
    /// Box filter ring and running sum.
    box_ring: Vec<f32>,
    box_pos: usize,
    box_sum: f64,
    rel_state: f32,
    t: u64,
    gr_meter: f32,
}

impl Limiter {
    pub const PARAMS: &'static [ParamSpec] = &[
        ParamSpec::new("input_gain", "Input Boost", 0.0, 24.0, 0.0, Unit::Decibels),
        ParamSpec::new("ceiling", "Ceiling", -24.0, 0.0, -1.0, Unit::Decibels),
        ParamSpec::log("release", "Release", 1.0, 1000.0, 60.0, Unit::Milliseconds),
        ParamSpec::toggle("true_peak", "True Peak", true),
    ];
    /// Fixed look-ahead time.
    pub const LOOKAHEAD_MS: f32 = 5.0;

    pub fn new(sample_rate: f32, channels: usize) -> Self {
        let channels = channels.max(1);
        let bank = PolyphaseBank::new(true_peak_factor(sample_rate as f64).max(2));
        let look = ((Self::LOOKAHEAD_MS * 0.001 * sample_rate).round() as usize).max(1);
        let det_delay = bank.delay();
        let dlen = look + det_delay + 1;
        let mut s = Limiter {
            pv: ParamValues::new(Self::PARAMS),
            sr: sample_rate,
            in_gain: Smoothed::with_ms(1.0, sample_rate, 20.0),
            ceiling: 1.0,
            rel: 0.0,
            true_peak: true,
            look,
            det_delay,
            bank,
            interp: vec![Interpolator::default(); channels],
            delay: vec![vec![0.0; dlen]; channels],
            wpos: 0,
            minq: VecDeque::with_capacity(look + 2),
            box_ring: vec![1.0; look + 1],
            box_pos: 0,
            box_sum: (look + 1) as f64,
            rel_state: 1.0,
            t: 0,
            gr_meter: 0.0,
        };
        s.apply_params(true);
        s
    }

    fn apply_params(&mut self, snap: bool) {
        self.in_gain.set(db_to_gain(self.pv.v("input_gain")));
        self.ceiling = db_to_gain(self.pv.v("ceiling"));
        self.rel = coef(self.pv.v("release"), self.sr);
        self.true_peak = self.pv.on("true_peak");
        if snap {
            self.in_gain.snap();
        }
    }

    /// Current gain reduction in dB (≤ 0).
    pub fn gain_reduction_db(&self) -> f32 {
        self.gr_meter
    }
}

impl AudioEffect for Limiter {
    param_plumbing!("limiter");
    fn latency(&self) -> usize {
        self.look + self.det_delay
    }
    fn reset(&mut self) {
        self.in_gain.snap();
        self.interp.iter_mut().for_each(Interpolator::reset);
        self.delay.iter_mut().for_each(|d| d.iter_mut().for_each(|v| *v = 0.0));
        self.wpos = 0;
        self.minq.clear();
        self.box_ring.iter_mut().for_each(|v| *v = 1.0);
        self.box_pos = 0;
        self.box_sum = self.box_ring.len() as f64;
        self.rel_state = 1.0;
        self.t = 0;
        self.gr_meter = 0.0;
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        let n = block_len(channels);
        let nch = channels.len().min(self.delay.len());
        let dlen = self.delay[0].len();
        let window = (self.look + 1) as u64;
        let mut last_g = None;
        for i in 0..n {
            let g_in = self.in_gain.tick();
            let det_pos = (self.wpos + dlen - self.det_delay) % dlen;
            let out_pos = (self.wpos + 1) % dlen; // oldest = look + det_delay samples ago
            let mut det = 0.0f32;
            for ch in 0..nch {
                let x = channels[ch][i] * g_in;
                self.delay[ch][self.wpos] = x;
                let aligned = self.delay[ch][det_pos].abs();
                let tp = if self.true_peak { self.interp[ch].push_peak(&self.bank, x) } else { 0.0 };
                det = det.max(aligned.max(tp));
            }
            let greq = if det > self.ceiling { self.ceiling / det } else { 1.0 };
            // Sliding minimum over the last `window` values.
            while let Some(&(_, v)) = self.minq.back() {
                if v >= greq {
                    self.minq.pop_back();
                } else {
                    break;
                }
            }
            self.minq.push_back((self.t, greq));
            while let Some(&(idx, _)) = self.minq.front() {
                if idx + window <= self.t {
                    self.minq.pop_front();
                } else {
                    break;
                }
            }
            let m = self.minq.front().map_or(1.0, |&(_, v)| v);
            // Release.
            self.rel_state = if m < self.rel_state { m } else { self.rel_state + (m - self.rel_state) * (1.0 - self.rel) };
            // Moving average over look+1 samples.
            self.box_sum += self.rel_state as f64 - self.box_ring[self.box_pos] as f64;
            self.box_ring[self.box_pos] = self.rel_state;
            self.box_pos = (self.box_pos + 1) % self.box_ring.len();
            let g = (self.box_sum / self.box_ring.len() as f64) as f32;
            for ch in 0..nch {
                let y = self.delay[ch][out_pos] * g;
                channels[ch][i] = y.clamp(-self.ceiling, self.ceiling);
            }
            self.wpos = (self.wpos + 1) % dlen;
            self.t += 1;
            // Periodically re-sum to cancel floating-point drift.
            if self.t.is_multiple_of(65536) {
                self.box_sum = self.box_ring.iter().map(|&v| v as f64).sum();
            }
            last_g = Some(g);
        }
        if let Some(g) = last_g {
            self.gr_meter = lin_db(g);
        }
    }
}


