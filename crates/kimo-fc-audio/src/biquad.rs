// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit 25979b15a419d4457fb67ea5e10c1a15d2e56b62), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Second-order IIR sections: RBJ "Audio EQ Cookbook" designs, transposed direct form II.

use crate::flush;
use std::f64::consts::PI;

/// Filter shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterType {
    Peaking,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
    Notch,
    BandPass,
    AllPass,
    /// Constant-bandwidth cut: `1 + (g − 1)·BP(s)` — the −3 dB width of the band-pass stays
    /// `f/Q` however deep the cut (`gain_db` ≤ 0), unlike the RBJ peaking filter whose cut
    /// widens with depth. Used by the Notch Filter.
    Cut,
}

impl FilterType {
    /// Order used by the parametric EQ's `type` choice parameter.
    pub const ALL: [FilterType; 7] =
        [FilterType::Peaking, FilterType::LowShelf, FilterType::HighShelf, FilterType::LowPass, FilterType::HighPass, FilterType::Notch, FilterType::BandPass];
    pub fn from_index(i: usize) -> FilterType {
        Self::ALL[i.min(Self::ALL.len() - 1)]
    }
}

/// Normalised biquad coefficients (`a0 == 1`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Coeffs {
    /// Pass-through.
    pub const IDENTITY: Coeffs = Coeffs { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    fn norm(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Coeffs {
        Coeffs { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }

    /// RBJ cookbook design. `freq` in Hz (clamped below Nyquist), `q` > 0, `gain_db` used by
    /// peaking and shelving types (shelves use `q` as the shelf "Q", S=1 ≙ Q=1/√2).
    pub fn design(kind: FilterType, sample_rate: f64, freq: f64, q: f64, gain_db: f64) -> Coeffs {
        let f = freq.clamp(1.0, sample_rate * 0.49);
        let q = q.max(1e-3);
        let w0 = 2.0 * PI * f / sample_rate;
        let (sn, cs) = w0.sin_cos();
        let alpha = sn / (2.0 * q);
        let a = 10f64.powf(gain_db / 40.0);
        match kind {
            FilterType::LowPass => Self::norm((1.0 - cs) / 2.0, 1.0 - cs, (1.0 - cs) / 2.0, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            FilterType::HighPass => Self::norm((1.0 + cs) / 2.0, -(1.0 + cs), (1.0 + cs) / 2.0, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            FilterType::BandPass => Self::norm(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            FilterType::Notch => Self::norm(1.0, -2.0 * cs, 1.0, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            FilterType::AllPass => Self::norm(1.0 - alpha, -2.0 * cs, 1.0 + alpha, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            FilterType::Cut => {
                let g = 10f64.powf(gain_db / 20.0);
                Self::norm(1.0 + g * alpha, -2.0 * cs, 1.0 - g * alpha, 1.0 + alpha, -2.0 * cs, 1.0 - alpha)
            }
            FilterType::Peaking => Self::norm(1.0 + alpha * a, -2.0 * cs, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cs, 1.0 - alpha / a),
            FilterType::LowShelf => {
                let k = 2.0 * a.sqrt() * alpha;
                Self::norm(
                    a * ((a + 1.0) - (a - 1.0) * cs + k),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cs),
                    a * ((a + 1.0) - (a - 1.0) * cs - k),
                    (a + 1.0) + (a - 1.0) * cs + k,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cs),
                    (a + 1.0) + (a - 1.0) * cs - k,
                )
            }
            FilterType::HighShelf => {
                let k = 2.0 * a.sqrt() * alpha;
                Self::norm(
                    a * ((a + 1.0) + (a - 1.0) * cs + k),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cs),
                    a * ((a + 1.0) + (a - 1.0) * cs - k),
                    (a + 1.0) - (a - 1.0) * cs + k,
                    2.0 * ((a - 1.0) - (a + 1.0) * cs),
                    (a + 1.0) - (a - 1.0) * cs - k,
                )
            }
        }
    }

    /// |H(e^{jω})| at `freq` Hz.
    pub fn magnitude(&self, freq: f64, sample_rate: f64) -> f64 {
        let w = 2.0 * PI * freq / sample_rate;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        // H = (b0 + b1 z^-1 + b2 z^-2) / (1 + a1 z^-1 + a2 z^-2), z^-1 = e^{-jω}.
        let nr = self.b0 + self.b1 * c1 + self.b2 * c2;
        let ni = -(self.b1 * s1 + self.b2 * s2);
        let dr = 1.0 + self.a1 * c1 + self.a2 * c2;
        let di = -(self.a1 * s1 + self.a2 * s2);
        ((nr * nr + ni * ni) / (dr * dr + di * di)).sqrt()
    }

    /// Magnitude in dB.
    pub fn magnitude_db(&self, freq: f64, sample_rate: f64) -> f64 {
        20.0 * self.magnitude(freq, sample_rate).log10()
    }
}

/// One biquad section's state (TDF-II), f64 internally.
#[derive(Clone, Copy, Debug, Default)]
pub struct Biquad {
    s1: f64,
    s2: f64,
}

impl Biquad {
    pub const fn new() -> Self {
        Biquad { s1: 0.0, s2: 0.0 }
    }
    #[inline(always)]
    pub fn tick(&mut self, c: &Coeffs, x: f64) -> f64 {
        let y = c.b0 * x + self.s1;
        self.s1 = flush(c.b1 * x - c.a1 * y + self.s2);
        self.s2 = flush(c.b2 * x - c.a2 * y);
        y
    }
    pub fn reset(&mut self) {
        *self = Biquad::new();
    }
    /// Exact zero state, including the sign of zero, for identity-filter fast paths.
    pub(crate) fn is_clear(&self) -> bool {
        self.s1.to_bits() == 0 && self.s2.to_bits() == 0
    }
    /// Filter a buffer in place.
    pub fn process(&mut self, c: &Coeffs, buf: &mut [f32]) {
        for v in buf {
            *v = self.tick(c, *v as f64) as f32;
        }
    }
}


