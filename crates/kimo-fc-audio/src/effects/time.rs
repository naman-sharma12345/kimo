// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit 25979b15a419d4457fb67ea5e10c1a15d2e56b62), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Time-based effects: feedback delay and an algorithmic (FDN) reverb.

use crate::{AudioEffect, ParamSpec, ParamValues, Smoothed, Unit, block_len, flush32, param_plumbing};

/// Fractional read from a circular buffer: `delay` samples behind write position `w`.
#[inline]
fn read_frac(buf: &[f32], w: usize, delay: f32) -> f32 {
    read_prepared(buf, w, prepare_delay(buf.len(), delay))
}

#[inline]
fn prepare_delay(len: usize, delay: f32) -> (usize, f32) {
    let d = delay.clamp(1.0, (len - 2) as f32);
    let di = d.floor();
    let frac = d - di;
    // The delay is bounded by this ring's length, so wrapping needs no integer division.
    // Keep the fallback for very large rings whose f32 length rounds upward.
    let distance = di as usize;
    let distance = if distance <= len { distance } else { distance % len };
    (distance, frac)
}

#[inline]
fn read_prepared(buf: &[f32], w: usize, (distance, frac): (usize, f32)) -> f32 {
    let len = buf.len();
    let i0 = if w >= distance { w - distance } else { len - (distance - w) };
    let i1 = if i0 == 0 { len - 1 } else { i0 - 1 };
    buf[i0] + (buf[i1] - buf[i0]) * frac
}

/// Note divisions for tempo-synced delay, in quarter notes.
const DIVISIONS: &[&str] = &["1/1", "1/2", "1/4", "1/8", "1/16", "1/4 dotted", "1/8 dotted", "1/4 triplet", "1/8 triplet"];
const DIVISION_QUARTERS: [f32; 9] = [4.0, 2.0, 1.0, 0.5, 0.25, 1.5, 0.75, 2.0 / 3.0, 1.0 / 3.0];

/// Delay time in ms for a tempo and division index.
pub fn tempo_delay_ms(bpm: f32, division: usize) -> f32 {
    60_000.0 / bpm.max(1.0) * DIVISION_QUARTERS[division.min(DIVISION_QUARTERS.len() - 1)]
}

/// Feedback delay (per-channel), time in ms or tempo-synced.
pub struct Delay {
    pv: ParamValues,
    sr: f32,
    bufs: Vec<Vec<f32>>,
    w: usize,
    /// Delay in samples (smoothed: glides like a tape delay instead of clicking).
    time: Smoothed,
    feedback: Smoothed,
    mix: Smoothed,
}

impl Delay {
    pub const MAX_MS: f32 = 4000.0;
    pub const PARAMS: &'static [ParamSpec] = &[
        ParamSpec::choice("mode", "Mode", &["Time", "Tempo"], 0),
        ParamSpec::log("time", "Delay Time", 1.0, 2000.0, 250.0, Unit::Milliseconds),
        ParamSpec::new("bpm", "Tempo", 20.0, 300.0, 120.0, Unit::Bpm),
        ParamSpec::choice("division", "Division", DIVISIONS, 3),
        ParamSpec::new("feedback", "Feedback", 0.0, 95.0, 30.0, Unit::Percent),
        ParamSpec::new("mix", "Mix", 0.0, 100.0, 30.0, Unit::Percent),
    ];

    pub fn new(sample_rate: f32, channels: usize) -> Self {
        let len = (Self::MAX_MS * 0.001 * sample_rate) as usize + 4;
        let mut s = Delay {
            pv: ParamValues::new(Self::PARAMS),
            sr: sample_rate,
            bufs: vec![vec![0.0; len]; channels.max(1)],
            w: 0,
            time: Smoothed::with_ms(1.0, sample_rate, 100.0),
            feedback: Smoothed::with_ms(0.0, sample_rate, 20.0),
            mix: Smoothed::with_ms(0.0, sample_rate, 20.0),
        };
        s.apply_params(true);
        s
    }

    /// Effective delay time (ms) at the current settings.
    pub fn delay_ms(&self) -> f32 {
        let ms = if self.pv.idx("mode") == 1 { tempo_delay_ms(self.pv.v("bpm"), self.pv.idx("division")) } else { self.pv.v("time") };
        ms.min(Self::MAX_MS)
    }

    fn apply_params(&mut self, snap: bool) {
        self.time.set(self.delay_ms() * 0.001 * self.sr);
        self.feedback.set(self.pv.v("feedback") / 100.0);
        self.mix.set(self.pv.v("mix") / 100.0);
        if snap {
            self.time.snap();
            self.feedback.snap();
            self.mix.snap();
        }
    }
}

impl AudioEffect for Delay {
    param_plumbing!("delay");
    fn reset(&mut self) {
        self.bufs.iter_mut().for_each(|b| b.iter_mut().for_each(|v| *v = 0.0));
        self.w = 0;
        self.time.snap();
        self.feedback.snap();
        self.mix.snap();
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        let n = block_len(channels);
        let nch = channels.len().min(self.bufs.len());
        let len = self.bufs[0].len();
        for i in 0..n {
            let d = self.time.tick();
            let fb = self.feedback.tick();
            let mix = self.mix.tick();
            for ch in 0..nch {
                let x = channels[ch][i];
                let buf = &mut self.bufs[ch];
                let yd = read_frac(buf, self.w, d);
                buf[self.w] = flush32(x + fb * yd);
                channels[ch][i] = x + mix * (yd - x);
            }
            self.w = if self.w + 1 == len { 0 } else { self.w + 1 };
        }
    }
}

// ---------------------------------------------------------------------------------------------

const FDN: usize = 8;
/// Base loop delays (ms) at size = 50 %; mutually prime-ish so modes don't pile up.
const BASE_MS: [f32; FDN] = [29.7, 37.1, 41.1, 43.7, 53.9, 59.3, 67.1, 73.3];
const MAX_SIZE_SCALE: f32 = 2.0;
/// Output tap signs (left, right) for decorrelated stereo.
const SIGN_L: [f32; FDN] = [1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0];
const SIGN_R: [f32; FDN] = [1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0];

fn size_scale(size_pct: f32) -> f32 {
    0.35 + (MAX_SIZE_SCALE - 0.35) * size_pct / 100.0
}

/// 8×8 feedback-delay-network reverb with a Householder feedback matrix, per-line RT60 gains,
/// one-pole HF damping in each loop, pre-delay, size and dry/wet mix.
pub struct Reverb {
    pv: ParamValues,
    sr: f32,
    lines: Vec<Vec<f32>>,
    w: usize,
    len: [Smoothed; FDN],
    lp: [f32; FDN],
    gains: [f32; FDN],
    gain_key: ([f32; FDN], f32),
    damp: Smoothed,
    rt60: Smoothed,
    mix: Smoothed,
    pre: Vec<f32>,
    pre_w: usize,
    pre_len: Smoothed,
    counter: u32,
}

impl Reverb {
    pub const PARAMS: &'static [ParamSpec] = &[
        ParamSpec::new("predelay", "Pre-delay", 0.0, 200.0, 20.0, Unit::Milliseconds),
        ParamSpec::log("decay", "Decay (RT60)", 0.1, 20.0, 2.0, Unit::Seconds),
        ParamSpec::new("damping", "HF Damping", 0.0, 100.0, 40.0, Unit::Percent),
        ParamSpec::new("size", "Room Size", 0.0, 100.0, 50.0, Unit::Percent),
        ParamSpec::new("mix", "Mix", 0.0, 100.0, 25.0, Unit::Percent),
    ];

    pub fn new(sample_rate: f32, _channels: usize) -> Self {
        let max_line = (BASE_MS[FDN - 1] * MAX_SIZE_SCALE * 0.001 * sample_rate) as usize + 4;
        let pre_len = (0.2 * sample_rate) as usize + 4;
        let sm = |v| Smoothed::with_ms(v, sample_rate, 50.0);
        let mut s = Reverb {
            pv: ParamValues::new(Self::PARAMS),
            sr: sample_rate,
            lines: vec![vec![0.0; max_line]; FDN],
            w: 0,
            len: [sm(1.0); FDN],
            lp: [0.0; FDN],
            gains: [0.0; FDN],
            gain_key: ([0.0; FDN], 0.0),
            damp: sm(0.0),
            rt60: sm(1.0),
            mix: sm(0.0),
            pre: vec![0.0; pre_len],
            pre_w: 0,
            pre_len: Smoothed::with_ms(1.0, sample_rate, 100.0),
            counter: 0,
        };
        s.apply_params(true);
        s
    }

    fn apply_params(&mut self, snap: bool) {
        let scale = size_scale(self.pv.v("size"));
        for (k, l) in self.len.iter_mut().enumerate() {
            l.set(BASE_MS[k] * scale * 0.001 * self.sr);
        }
        self.pre_len.set((self.pv.v("predelay") * 0.001 * self.sr).max(1.0));
        self.damp.set(self.pv.v("damping") / 100.0 * 0.85);
        self.rt60.set(self.pv.v("decay"));
        self.mix.set(self.pv.v("mix") / 100.0);
        if snap {
            self.len.iter_mut().for_each(Smoothed::snap);
            self.pre_len.snap();
            self.damp.snap();
            self.rt60.snap();
            self.mix.snap();
            self.update_gains();
        }
    }

    fn update_gains(&mut self) {
        let rt = self.rt60.value().max(0.01);
        let lengths = self.len.map(|l| l.value());
        if self.gain_key == (lengths, rt) {
            return;
        }
        self.gain_key = (lengths, rt);
        for k in 0..FDN {
            // Attenuation per pass so the loop decays 60 dB in rt seconds.
            self.gains[k] = 10f32.powf(-3.0 * self.len[k].value() / (rt * self.sr));
        }
    }
}

impl AudioEffect for Reverb {
    param_plumbing!("reverb");
    fn reset(&mut self) {
        self.lines.iter_mut().for_each(|l| l.iter_mut().for_each(|v| *v = 0.0));
        self.pre.iter_mut().for_each(|v| *v = 0.0);
        self.lp = [0.0; FDN];
        self.w = 0;
        self.pre_w = 0;
        self.counter = 0;
        self.apply_params(true);
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        let n = block_len(channels);
        let nch = channels.len();
        if nch == 0 {
            return;
        }
        let line_len = self.lines[0].len();
        let pre_len = self.pre.len();
        let hh = 2.0 / FDN as f32;
        // During a settled block these positions cannot change. Preserve the original path
        // while smoothing, and preserve the 32-sample gain update grid in both cases.
        let fixed = self.len.iter().all(|l| !l.is_smoothing());
        let delays = self.len.map(|l| prepare_delay(line_len, l.value()));
        let predelay = (!self.pre_len.is_smoothing()).then(|| prepare_delay(pre_len, self.pre_len.value()));
        for i in 0..n {
            if self.counter == 0 {
                self.update_gains();
            }
            self.counter = (self.counter + 1) % 32;
            if !fixed {
                for l in &mut self.len {
                    l.tick();
                }
            }
            self.rt60.tick();
            let damp = self.damp.tick();
            let mix = self.mix.tick();
            let pd = self.pre_len.tick();

            let mut mono = 0.0;
            for ch in channels.iter() {
                mono += ch[i];
            }
            mono /= nch as f32;
            self.pre[self.pre_w] = mono;
            let input = match predelay {
                Some(delay) => read_prepared(&self.pre, self.pre_w, delay),
                None => read_frac(&self.pre, self.pre_w, pd),
            };
            self.pre_w = if self.pre_w + 1 == pre_len { 0 } else { self.pre_w + 1 };

            let mut v = [0.0f32; FDN];
            let mut sum = 0.0;
            for k in 0..FDN {
                let d = if fixed { read_prepared(&self.lines[k], self.w, delays[k]) } else { read_frac(&self.lines[k], self.w, self.len[k].value()) };
                self.lp[k] = flush32(d + (self.lp[k] - d) * damp);
                v[k] = self.lp[k] * self.gains[k];
                sum += v[k];
            }
            let (mut wl, mut wr) = (0.0, 0.0);
            for k in 0..FDN {
                let fbk = v[k] - hh * sum;
                self.lines[k][self.w] = flush32(fbk + input * 0.5);
                wl += SIGN_L[k] * v[k];
                wr += SIGN_R[k] * v[k];
            }
            self.w = if self.w + 1 == line_len { 0 } else { self.w + 1 };
            let (wl, wr) = (wl * 0.35, wr * 0.35);
            for (c, ch) in channels.iter_mut().enumerate() {
                let x = ch[i];
                let wet = if nch == 1 {
                    0.5 * (wl + wr)
                } else if c % 2 == 0 {
                    wl
                } else {
                    wr
                };
                ch[i] = x + mix * (wet - x);
            }
        }
    }
}



#[cfg(test)]
fn read_frac_reference(buf: &[f32], w: usize, delay: f32) -> f32 {
    let len = buf.len();
    let d = delay.clamp(1.0, (len - 2) as f32);
    let di = d.floor();
    let frac = d - di;
    let i0 = (w + len - di as usize) % len;
    let i1 = (i0 + len - 1) % len;
    buf[i0] + (buf[i1] - buf[i0]) * frac
}
#[cfg(test)]
impl Reverb {
    fn update_gains_reference(&mut self) {
        let rt = self.rt60.value().max(0.01);
        for k in 0..FDN {
            // Attenuation per pass so the loop decays 60 dB in rt seconds.
            self.gains[k] = 10f32.powf(-3.0 * self.len[k].value() / (rt * self.sr));
        }
    }
    fn process_reference(&mut self, channels: &mut [&mut [f32]]) {
        let n = block_len(channels);
        let nch = channels.len();
        if nch == 0 {
            return;
        }
        let line_len = self.lines[0].len();
        let pre_len = self.pre.len();
        let hh = 2.0 / FDN as f32;
        for i in 0..n {
            if self.counter == 0 {
                self.update_gains_reference();
            }
            self.counter = (self.counter + 1) % 32;
            for l in &mut self.len {
                l.tick();
            }
            self.rt60.tick();
            let damp = self.damp.tick();
            let mix = self.mix.tick();
            let pd = self.pre_len.tick();

            let mut mono = 0.0;
            for ch in channels.iter() {
                mono += ch[i];
            }
            mono /= nch as f32;
            self.pre[self.pre_w] = mono;
            let input = read_frac_reference(&self.pre, self.pre_w, pd);
            self.pre_w = (self.pre_w + 1) % pre_len;

            let mut v = [0.0f32; FDN];
            let mut sum = 0.0;
            for k in 0..FDN {
                let d = read_frac_reference(&self.lines[k], self.w, self.len[k].value());
                self.lp[k] = flush32(d + (self.lp[k] - d) * damp);
                v[k] = self.lp[k] * self.gains[k];
                sum += v[k];
            }
            let (mut wl, mut wr) = (0.0, 0.0);
            for k in 0..FDN {
                let fbk = v[k] - hh * sum;
                self.lines[k][self.w] = flush32(fbk + input * 0.5);
                wl += SIGN_L[k] * v[k];
                wr += SIGN_R[k] * v[k];
            }
            self.w = (self.w + 1) % line_len;
            let (wl, wr) = (wl * 0.35, wr * 0.35);
            for (c, ch) in channels.iter_mut().enumerate() {
                let x = ch[i];
                let wet = if nch == 1 {
                    0.5 * (wl + wr)
                } else if c % 2 == 0 {
                    wl
                } else {
                    wr
                };
                ch[i] = x + mix * (wet - x);
            }
        }
    }
}

