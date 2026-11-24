// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit 25979b15a419d4457fb67ea5e10c1a15d2e56b62), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Streaming sample-rate conversion for playback, when the output device runs at another rate
//! than the sequence (a 44.1 kHz device playing a 48 kHz sequence).
//!
//! Every output sample's source position comes from its absolute output frame, as an exact
//! rational (`frame × from / to`), and is linearly interpolated, so the output is identical however
//! the stream is cut into blocks: no phase jump at block boundaries. Source frames are pulled in
//! contiguous, non-overlapping spans, so whatever produces them (the mixer, with its effect state)
//! sees one continuous stream. Output lags the source by one source sample (≈21 µs at 48 kHz),
//! which is what lets each block interpolate without reading ahead.

/// Converts a planar stream from `from` Hz to `to` Hz, block by block.
#[derive(Debug, Clone)]
pub struct StreamResampler {
    from: u32,
    to: u32,
    /// Source frames `[held_start, held_start + held[c].len())` kept from the last pull.
    held_start: i64,
    held: Vec<Vec<f32>>,
    /// The output frame the next block is expected to start at (contiguous playback).
    next_out: Option<i64>,
}

/// Maximum source frames fetched in a single resampler pull.
pub const MAX_RESAMPLE_SOURCE_SPAN: usize = 384_000 * 10;

impl StreamResampler {
    /// Rates of zero are treated as 1 Hz rather than dividing by zero.
    pub fn new(from: u32, to: u32) -> Self {
        Self { from: from.max(1), to: to.max(1), held_start: 0, held: Vec::new(), next_out: None }
    }

    pub fn rates(&self) -> (u32, u32) {
        (self.from, self.to)
    }

    /// The source frame whose sample lands at (or just before) output frame `out`, and the
    /// remainder of the exact position in units of `1 / to` source frames.
    fn source_index(&self, out: i64) -> (i64, i64) {
        let p = out as i128 * self.from as i128;
        let to = self.to as i128;
        (p.div_euclid(to) as i64, p.rem_euclid(to) as i64)
    }

    /// Output frames `[out, out + n)`, planar. `pull(start, frames)` returns source frames
    /// `[start, start + frames)`, planar (missing samples read as silence). A block that does not
    /// continue the previous one (a seek) starts the stream afresh.
    pub fn process(&mut self, out: i64, n: usize, mut pull: impl FnMut(i64, usize) -> Vec<Vec<f32>>) -> Vec<Vec<f32>> {
        if n == 0 {
            return vec![Vec::new(); self.held.len().max(1)];
        }
        let (first, first_rem) = self.source_index(out);
        let (last, _) = self.source_index(out.saturating_add(n as i64 - 1));
        // interpolation reads source frames first - 1 ..= last
        if self.next_out != Some(out) || self.held.is_empty() {
            self.held_start = first - 1;
            self.held.clear();
        }
        let held_end = self.held_start + self.held.first().map_or(0, Vec::len) as i64;
        if last + 1 > held_end {
            let want = ((last + 1 - held_end) as usize).min(MAX_RESAMPLE_SOURCE_SPAN);
            let got = pull(held_end, want);
            if self.held.is_empty() {
                self.held = vec![Vec::new(); got.len().max(1)];
            }
            for (c, h) in self.held.iter_mut().enumerate() {
                let src = got.get(c).map(Vec::as_slice).unwrap_or_default();
                h.extend((0..want).map(|i| src.get(i).copied().unwrap_or(0.0)));
            }
        }
        let mut dst = vec![vec![0.0f32; n]; self.held.len()];
        let to = self.to as i64;
        let (step, rem_step) = ((self.from / self.to) as i64, (self.from % self.to) as i64);
        let (mut s, mut rem) = (first, first_rem);
        for i in 0..n {
            let frac = (rem as f64 / to as f64) as f32;
            // held starts at first - 1, so the earlier frame is always held
            let k = usize::try_from(s - self.held_start).unwrap_or(0);
            for (d, h) in dst.iter_mut().zip(&self.held) {
                let a = k.checked_sub(1).and_then(|j| h.get(j)).copied().unwrap_or(0.0);
                let b = h.get(k).copied().unwrap_or(0.0);
                if let Some(x) = d.get_mut(i) {
                    *x = a + (b - a) * frac;
                }
            }
            s = s.wrapping_add(step);
            rem += rem_step;
            if rem >= to {
                rem -= to;
                s = s.wrapping_add(1);
            }
        }
        // keep what the next block can still read: from its first frame - 1 on
        let (next_first, _) = self.source_index(out.saturating_add(n as i64));
        let drop = (next_first - 1 - self.held_start).clamp(0, self.held.first().map_or(0, Vec::len) as i64) as usize;
        for h in &mut self.held {
            h.drain(..drop);
        }
        self.held_start += drop as i64;
        self.next_out = Some(out.saturating_add(n as i64));
        dst
    }
}


