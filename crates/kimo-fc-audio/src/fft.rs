// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit 25979b15a419d4457fb67ea5e10c1a15d2e56b62), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Minimal in-place radix-2 complex FFT (power-of-two sizes), allocation-free after planning.
//!
//! Kept in-crate (rather than depending on an FFT library) so the crate stays dependency-free;
//! the STFT effects only need a few fixed power-of-two sizes.

use std::f64::consts::PI;

/// A planned FFT of fixed power-of-two size.
#[derive(Clone, Debug)]
pub struct Fft {
    n: usize,
    /// Per-stage twiddles, contiguous: the stage with half-length `h` reads e^{-2πik/(2h)} for
    /// k in 0..h at offset `h - 1` (values taken from the n-point table, so they match it exactly).
    tw_re: Vec<f32>,
    tw_im: Vec<f32>,
    /// `tw_im` negated, for the inverse transform.
    tw_im_inv: Vec<f32>,
    rev: Vec<u32>,
}

impl Fft {
    /// Plan an FFT of size `n` (must be a power of two ≥ 2).
    pub fn new(n: usize) -> Fft {
        assert!(n >= 2 && n.is_power_of_two(), "FFT size must be a power of two");
        let bits = n.trailing_zeros();
        let rev = (0..n as u32).map(|i| i.reverse_bits() >> (32 - bits)).collect();
        let (mut tw_re, mut tw_im) = (Vec::with_capacity(n - 1), Vec::with_capacity(n - 1));
        let mut half = 1;
        while half < n {
            let stride = n / (2 * half);
            for k in 0..half {
                let a = -2.0 * PI * (k * stride) as f64 / n as f64;
                tw_re.push(a.cos() as f32);
                tw_im.push(a.sin() as f32);
            }
            half <<= 1;
        }
        let tw_im_inv = tw_im.iter().map(|w| -w).collect();
        Fft { n, tw_re, tw_im, tw_im_inv, rev }
    }

    /// Forward transform (e^{-iωt}), unscaled.
    pub fn forward(&self, re: &mut [f32], im: &mut [f32]) {
        self.run(re, im, false);
    }

    /// Inverse transform (e^{+iωt}), unscaled (divide by `n` yourself).
    pub fn inverse(&self, re: &mut [f32], im: &mut [f32]) {
        self.run(re, im, true);
    }

    fn run(&self, re: &mut [f32], im: &mut [f32], inverse: bool) {
        let n = self.n;
        assert!(re.len() >= n && im.len() >= n);
        let (re, im) = (&mut re[..n], &mut im[..n]);
        for i in 0..n {
            let j = self.rev[i] as usize;
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let tw_im = if inverse { &self.tw_im_inv } else { &self.tw_im };
        // First stage: the twiddle is 1 + i·(∓0). The multiplications by 1 are exact and dropped;
        // the ones by ∓0 stay so signed zeros come out exactly as in the general butterfly.
        let wi = tw_im[0];
        for (r, i) in re.as_chunks_mut::<2>().0.iter_mut().zip(im.as_chunks_mut::<2>().0) {
            let xr = r[1] - i[1] * wi;
            let xi = r[1] * wi + i[1];
            r[1] = r[0] - xr;
            i[1] = i[0] - xi;
            r[0] += xr;
            i[0] += xi;
        }
        let mut half = 2;
        while half < n {
            let wr = &self.tw_re[half - 1..2 * half - 1];
            let wi = &tw_im[half - 1..2 * half - 1];
            for (r, i) in re.chunks_exact_mut(2 * half).zip(im.chunks_exact_mut(2 * half)) {
                let (ra, rb) = r.split_at_mut(half);
                let (ia, ib) = i.split_at_mut(half);
                for k in 0..half {
                    let xr = rb[k] * wr[k] - ib[k] * wi[k];
                    let xi = rb[k] * wi[k] + ib[k] * wr[k];
                    rb[k] = ra[k] - xr;
                    ib[k] = ia[k] - xi;
                    ra[k] += xr;
                    ia[k] += xi;
                }
            }
            half <<= 1;
        }
    }
}


