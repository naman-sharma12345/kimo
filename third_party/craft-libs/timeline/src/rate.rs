// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/timeline,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Frame counts and frame rates as documents write them.

use crate::time::{FrameRate, Tick};

/// Frame count nearest to `t` at `rate`.
pub fn frames_round(rate: FrameRate, t: Tick) -> i64 {
    let f = rate.frame_at(t);
    let next = f.saturating_add(1);
    if rate.tick_of(next) - t < t - rate.tick_of(f) { next } else { f }
}

/// Whether `t` is exactly on a frame boundary of `rate`.
pub fn on_frame(rate: FrameRate, t: Tick) -> bool {
    rate.tick_of(rate.frame_at(t)) == t
}

/// FCP7-style `(timebase, ntsc)` for a rate.
pub fn timebase(rate: FrameRate) -> (i64, bool) {
    (rate.timecode_base(), rate.is_ntsc())
}

/// Highest rate (per second) an importer accepts. OTIO audio uses sample rates (192000 and more) as
/// time bases, so this is far above any video rate and still far below where frame arithmetic
/// overflows: a rate near `i64::MAX` made `frame_at` clamp to `i64::MAX`, and the `+ 1` after it overflowed.
pub const MAX_RATE: i64 = 1_000_000;

/// Whether `rate` is positive and at most [`MAX_RATE`] per second.
pub fn plausible_rate(rate: FrameRate) -> bool {
    rate.num > 0 && rate.den > 0 && i128::from(rate.num) <= i128::from(rate.den) * i128::from(MAX_RATE)
}

/// Rate from FCP7-style `(timebase, ntsc)`; the timebase is limited to `1..=MAX_RATE`.
pub fn rate_from_timebase(timebase: i64, ntsc: bool) -> FrameRate {
    let tb = timebase.clamp(1, MAX_RATE);
    if ntsc { FrameRate::new(tb * 1000, 1001) } else { FrameRate::new(tb, 1) }
}

/// Snap a floating rate (OTIO) to an exact rate.
pub fn rate_from_f64(r: f64) -> FrameRate {
    if r <= 0.0 || !r.is_finite() || r > MAX_RATE as f64 {
        return FrameRate::FPS_24;
    }
    for c in FrameRate::COMMON {
        if (c.as_f64() - r).abs() < 1e-3 {
            return c;
        }
    }
    let n = (r * 1001.0).round();
    if ((n / 1001.0) - r).abs() < 1e-9 && (n as i64) % 1000 == 0 {
        return FrameRate::new(n as i64, 1001);
    }
    if (r - r.round()).abs() < 1e-9 {
        return FrameRate::new(r.round() as i64, 1);
    }
    FrameRate::new((r * 1_000_000.0).round() as i64, 1_000_000)
}

// ---------------------------------------------------------------------------------------------
// Media descriptions
// ---------------------------------------------------------------------------------------------


