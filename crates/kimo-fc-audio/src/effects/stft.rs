// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit 25979b15a419d4457fb67ea5e10c1a15d2e56b62), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Streaming STFT overlap-add framework shared by the spectral effects.

use crate::fft::Fft;
use std::f64::consts::PI;

/// Per-channel streaming buffers. Latency is exactly `n` samples.
#[derive(Clone, Debug)]
pub(crate) struct StftChannel {
    input: Vec<f32>,
    output: Vec<f32>,
    pos: usize,
}

impl StftChannel {
    pub(crate) fn new(n: usize) -> Self {
        StftChannel { input: vec![0.0; n], output: vec![0.0; n], pos: 0 }
    }
    pub(crate) fn reset(&mut self) {
        self.input.iter_mut().for_each(|v| *v = 0.0);
        self.output.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
    }
    /// Push one sample and pop one output sample. Every `hop` samples `frame` is called with the
    /// last `n` input samples and must write an `n`-sample (windowed) frame that is overlap-added.
    #[inline]
    pub(crate) fn tick(&mut self, x: f32, hop: usize, frame: &mut impl FnMut(&[f32], &mut [f32])) -> f32 {
        let n = self.input.len();
        self.input[n - hop + self.pos] = x;
        let y = self.output[self.pos];
        self.pos += 1;
        if self.pos == hop {
            self.pos = 0;
            self.output.copy_within(hop.., 0);
            self.output[n - hop..].iter_mut().for_each(|v| *v = 0.0);
            frame(&self.input, &mut self.output);
            self.input.copy_within(hop.., 0);
        }
        y
    }

    /// `tick` for two channels that frame on the same samples: `frame` gets both inputs and both
    /// outputs.
    #[inline]
    fn tick_pair(
        a: &mut StftChannel,
        b: &mut StftChannel,
        x: (f32, f32),
        hop: usize,
        frame: &mut impl FnMut(&[f32], &[f32], &mut [f32], &mut [f32]),
    ) -> (f32, f32) {
        debug_assert_eq!(a.pos, b.pos);
        let n = a.input.len();
        a.input[n - hop + a.pos] = x.0;
        b.input[n - hop + a.pos] = x.1;
        let y = (a.output[a.pos], b.output[a.pos]);
        a.pos += 1;
        b.pos = a.pos;
        if a.pos == hop {
            a.pos = 0;
            b.pos = 0;
            for c in [&mut *a, &mut *b] {
                c.output.copy_within(hop.., 0);
                c.output[n - hop..].iter_mut().for_each(|v| *v = 0.0);
            }
            frame(&a.input, &b.input, &mut a.output, &mut b.output);
            a.input.copy_within(hop.., 0);
            b.input.copy_within(hop.., 0);
        }
        y
    }
}

/// Hop, gain and FFT pairing shared by every frame of one STFT effect run.
#[derive(Clone, Copy)]
pub(crate) struct Plan {
    pub hop: usize,
    pub scale: f32,
    pub pairing: Pairing,
}

/// Stream `len` samples of every channel through its `StftChannel`, sample-major. At a frame
/// boundary `bins(ch, first, re, im)` edits that channel's half spectrum (bins 0..=n/2);
/// `first` is set for the first channel that frames on this sample. Neighbouring channels that
/// frame on the same samples share one complex FFT (`a + i·b`), except for frames with a
/// non-finite sample, which run per channel so the bad channel cannot contaminate its neighbour.
/// `pairing` says whether the forward FFT is shared too or only the inverse one.
pub(crate) fn stream(
    stft: &mut [StftChannel],
    spec: &mut Spectral,
    channels: &mut [&mut [f32]],
    len: usize,
    plan: Plan,
    bins: &mut impl FnMut(usize, bool, &mut [f32], &mut [f32]),
) {
    let Plan { hop, scale, pairing } = plan;
    let nch = channels.len().min(stft.len());
    let half = spec.re2.len();
    for i in 0..len {
        let mut first = true;
        let mut ch = 0;
        while ch < nch {
            if ch + 1 < nch && stft[ch].pos == stft[ch + 1].pos {
                let (lo, hi) = stft.split_at_mut(ch + 1);
                let x = (channels[ch][i], channels[ch + 1][i]);
                let (ya, yb) = StftChannel::tick_pair(&mut lo[ch], &mut hi[0], x, hop, &mut |a, b, out_a, out_b| {
                    // Finite samples can still overflow the unscaled FFT: for `Full` the packed
                    // analysis itself is checked before either callback sees it.
                    let mut shared = all_finite(a) && all_finite(b);
                    if shared && pairing == Pairing::Full {
                        spec.analyse_pair(a, b);
                        shared = spec.pair_finite(half);
                    }
                    if shared {
                        match pairing {
                            Pairing::Full => {
                                bins(ch, first, &mut spec.re[..half], &mut spec.im[..half]);
                                bins(ch + 1, false, &mut spec.re2, &mut spec.im2);
                            }
                            Pairing::SynthesisOnly => {
                                // Each channel's spectrum is bit-identical to its mono analysis;
                                // the edited half spectra then share one inverse FFT.
                                spec.analyse(a);
                                bins(ch, first, &mut spec.re[..half], &mut spec.im[..half]);
                                spec.re2.copy_from_slice(&spec.re[..half]);
                                spec.im2.copy_from_slice(&spec.im[..half]);
                                spec.analyse(b);
                                bins(ch + 1, false, &mut spec.re[..half], &mut spec.im[..half]);
                                spec.re[..half].swap_with_slice(&mut spec.re2);
                                spec.im[..half].swap_with_slice(&mut spec.im2);
                            }
                        }
                        if spec.pair_finite(half) {
                            spec.synthesise_add_pair(out_a, out_b, scale);
                        } else {
                            // Poisoned per-channel state can leave one edited spectrum
                            // non-finite: synthesise separately so it cannot leak either.
                            spec.synthesise_add_split(out_a, out_b, scale);
                        }
                    } else {
                        // A non-finite sample (or a non-finite packed spectrum) would leak into
                        // the other channel: keep the channels isolated for this frame. Each
                        // callback still runs exactly once per channel.
                        spec.analyse(a);
                        bins(ch, first, &mut spec.re[..half], &mut spec.im[..half]);
                        spec.synthesise_add(out_a, scale);
                        spec.analyse(b);
                        bins(ch + 1, false, &mut spec.re[..half], &mut spec.im[..half]);
                        spec.synthesise_add(out_b, scale);
                    }
                    first = false;
                });
                channels[ch][i] = ya;
                channels[ch + 1][i] = yb;
                ch += 2;
            } else {
                channels[ch][i] = stft[ch].tick(channels[ch][i], hop, &mut |input, out| {
                    spec.analyse(input);
                    bins(ch, first, &mut spec.re[..half], &mut spec.im[..half]);
                    spec.synthesise_add(out, scale);
                    first = false;
                });
                ch += 1;
            }
        }
    }
}

/// How much of the STFT two paired channels share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pairing {
    /// Forward and inverse FFT (`a + i·b`). The analysed spectra differ from the mono ones at
    /// rounding level, so only for effects whose bin edit is continuous in the spectrum.
    Full,
    /// Only the inverse FFT. For effects whose bin edit makes discrete decisions on the spectrum
    /// (peak picking, thresholds): a rounding-level change could flip one, so they get the exact
    /// mono analysis.
    SynthesisOnly,
}

/// Branch-free scan (vectorises, unlike a short-circuiting `all`).
#[inline]
fn all_finite(x: &[f32]) -> bool {
    x.iter().fold(true, |ok, v| ok & v.is_finite())
}

/// Periodic Hann window.
pub(crate) fn hann(n: usize) -> Vec<f32> {
    (0..n).map(|i| (0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos()) as f32).collect()
}

/// FFT size for spectral effects at a sample rate (~43 ms at 48 kHz).
pub(crate) fn fft_size(sample_rate: f32) -> usize {
    if sample_rate <= 50_000.0 {
        2048
    } else if sample_rate <= 100_000.0 {
        4096
    } else {
        8192
    }
}

/// Shared spectral scratch.
#[derive(Clone, Debug)]
pub(crate) struct Spectral {
    pub fft: Fft,
    pub window: Vec<f32>,
    pub re: Vec<f32>,
    pub im: Vec<f32>,
    /// Second channel's half spectrum (bins 0..=n/2) after `analyse_pair`.
    pub re2: Vec<f32>,
    pub im2: Vec<f32>,
}

impl Spectral {
    /// `sqrt_window`: use √Hann for analysis and synthesis (sums to a constant at 75 % overlap);
    /// otherwise Hann for both.
    pub(crate) fn new(n: usize, sqrt_window: bool) -> Self {
        let mut window = hann(n);
        if sqrt_window {
            window.iter_mut().for_each(|w| *w = w.sqrt());
        }
        Spectral { fft: Fft::new(n), window, re: vec![0.0; n], im: vec![0.0; n], re2: vec![0.0; n / 2 + 1], im2: vec![0.0; n / 2 + 1] }
    }
    /// Window `input` into re/im and run the forward FFT.
    pub(crate) fn analyse(&mut self, input: &[f32]) {
        for i in 0..input.len() {
            self.re[i] = input[i] * self.window[i];
            self.im[i] = 0.0;
        }
        self.fft.forward(&mut self.re, &mut self.im);
    }
    /// Enforce Hermitian symmetry from bins 0..=n/2, inverse FFT, window, scale and add to `out`.
    pub(crate) fn synthesise_add(&mut self, out: &mut [f32], scale: f32) {
        let n = self.re.len();
        self.im[0] = 0.0;
        self.im[n / 2] = 0.0;
        for k in 1..n / 2 {
            self.re[n - k] = self.re[k];
            self.im[n - k] = -self.im[k];
        }
        self.fft.inverse(&mut self.re, &mut self.im);
        for i in 0..n {
            out[i] += self.re[i] * self.window[i] * scale;
        }
    }
    /// Window `a` and `b` into one complex FFT (`a + i·b`), then split it: bins 0..=n/2 of
    /// re/im hold the spectrum of `a`, re2/im2 that of `b`.
    pub(crate) fn analyse_pair(&mut self, a: &[f32], b: &[f32]) {
        let n = self.re.len();
        for i in 0..n {
            self.re[i] = a[i] * self.window[i];
            self.im[i] = b[i] * self.window[i];
        }
        self.fft.forward(&mut self.re, &mut self.im);
        // A[k] = (Z[k] + conj Z[n−k]) / 2, B[k] = (Z[k] − conj Z[n−k]) / 2i. Bins above n/2 are
        // only read, so the lower half can be overwritten in place.
        for k in 0..=n / 2 {
            let m = (n - k) % n;
            let (zr, zi, wr, wi) = (self.re[k], self.im[k], self.re[m], self.im[m]);
            self.re[k] = 0.5 * (zr + wr);
            self.im[k] = 0.5 * (zi - wi);
            self.re2[k] = 0.5 * (zi + wi);
            self.im2[k] = 0.5 * (wr - zr);
        }
    }
    /// Whether both half spectra of `analyse_pair` (bins 0..half) are finite and small enough that
    /// the shared unscaled FFT cannot overflow. Every FFT value sums at most 4 * half unit-twiddle
    /// terms (two mirrored bins, two channels), so for FFT sizes up to 16384 a per-bin limit of
    /// `f32::MAX / 65536` is safe. NaN fails the comparison. Branch-free so it vectorises.
    pub(crate) fn pair_finite(&self, half: usize) -> bool {
        const LIMIT: f32 = f32::MAX / 65536.0;
        [&self.re[..half], &self.im[..half], &self.re2[..half], &self.im2[..half]].iter().all(|s| s.iter().fold(true, |ok, v| ok & (v.abs() <= LIMIT)))
    }
    /// `synthesise_add_pair` without sharing the inverse FFT: the two spectra stay isolated.
    pub(crate) fn synthesise_add_split(&mut self, out_a: &mut [f32], out_b: &mut [f32], scale: f32) {
        self.synthesise_add(out_a, scale);
        let half = self.re2.len();
        self.re[..half].copy_from_slice(&self.re2);
        self.im[..half].copy_from_slice(&self.im2);
        self.synthesise_add(out_b, scale);
    }
    /// `synthesise_add` for both spectra of `analyse_pair`: one inverse FFT of
    /// Z = Y_a + i·Y_b, whose real part goes to `out_a` and imaginary part to `out_b`.
    pub(crate) fn synthesise_add_pair(&mut self, out_a: &mut [f32], out_b: &mut [f32], scale: f32) {
        let n = self.re.len();
        let h = n / 2;
        self.im[0] = 0.0;
        self.im[h] = 0.0;
        self.im2[0] = 0.0;
        self.im2[h] = 0.0;
        for k in 1..h {
            // Z[n−k] = conj A[k] + i·conj B[k]
            self.re[n - k] = self.re[k] + self.im2[k];
            self.im[n - k] = self.re2[k] - self.im[k];
        }
        for k in 0..=h {
            self.re[k] -= self.im2[k];
            self.im[k] += self.re2[k];
        }
        self.fft.inverse(&mut self.re, &mut self.im);
        for i in 0..n {
            out_a[i] += self.re[i] * self.window[i] * scale;
            out_b[i] += self.im[i] * self.window[i] * scale;
        }
    }
}


