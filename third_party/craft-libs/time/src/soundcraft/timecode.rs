// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/time,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! SMPTE timecode and Feet+Frames.

use super::{SampleRate, Samples, TimeError};

/// Session timecode rates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum FrameRate {
    Fps23_976,
    #[default]
    Fps24,
    Fps25,
    Fps29_97,
    Fps29_97Drop,
    Fps30,
    Fps47_952,
    Fps48,
    Fps50,
    Fps59_94,
    Fps59_94Drop,
    Fps60,
    Fps100,
    Fps119_88,
    Fps120,
}

impl FrameRate {
    pub const ALL: [FrameRate; 15] = [
        FrameRate::Fps23_976,
        FrameRate::Fps24,
        FrameRate::Fps25,
        FrameRate::Fps29_97,
        FrameRate::Fps29_97Drop,
        FrameRate::Fps30,
        FrameRate::Fps47_952,
        FrameRate::Fps48,
        FrameRate::Fps50,
        FrameRate::Fps59_94,
        FrameRate::Fps59_94Drop,
        FrameRate::Fps60,
        FrameRate::Fps100,
        FrameRate::Fps119_88,
        FrameRate::Fps120,
    ];

    /// Real frames per second as a rational (num, den).
    pub fn rational(self) -> (i64, i64) {
        match self {
            FrameRate::Fps23_976 => (24_000, 1001),
            FrameRate::Fps24 => (24, 1),
            FrameRate::Fps25 => (25, 1),
            FrameRate::Fps29_97 | FrameRate::Fps29_97Drop => (30_000, 1001),
            FrameRate::Fps30 => (30, 1),
            FrameRate::Fps47_952 => (48_000, 1001),
            FrameRate::Fps48 => (48, 1),
            FrameRate::Fps50 => (50, 1),
            FrameRate::Fps59_94 | FrameRate::Fps59_94Drop => (60_000, 1001),
            FrameRate::Fps60 => (60, 1),
            FrameRate::Fps100 => (100, 1),
            FrameRate::Fps119_88 => (120_000, 1001),
            FrameRate::Fps120 => (120, 1),
        }
    }

    /// Frames per labelled second (the counting base).
    pub fn nominal(self) -> i64 {
        let (n, d) = self.rational();
        (n + d / 2) / d
    }

    pub fn is_drop(self) -> bool {
        matches!(self, FrameRate::Fps29_97Drop | FrameRate::Fps59_94Drop)
    }

    pub fn label(self) -> &'static str {
        match self {
            FrameRate::Fps23_976 => "23.976",
            FrameRate::Fps24 => "24",
            FrameRate::Fps25 => "25",
            FrameRate::Fps29_97 => "29.97",
            FrameRate::Fps29_97Drop => "29.97 Drop",
            FrameRate::Fps30 => "30",
            FrameRate::Fps47_952 => "47.952",
            FrameRate::Fps48 => "48",
            FrameRate::Fps50 => "50",
            FrameRate::Fps59_94 => "59.94",
            FrameRate::Fps59_94Drop => "59.94 Drop",
            FrameRate::Fps60 => "60",
            FrameRate::Fps100 => "100",
            FrameRate::Fps119_88 => "119.88",
            FrameRate::Fps120 => "120",
        }
    }

    /// Whole frame index at a sample position (floored).
    pub fn frames_at(self, s: Samples, sr: SampleRate) -> i64 {
        let (n, d) = self.rational();
        let num = i128::from(s) * i128::from(n);
        let den = i128::from(sr.hz()) * i128::from(d);
        if den == 0 {
            return 0;
        }
        i64::try_from(num.div_euclid(den)).unwrap_or(0)
    }

    /// Sample position of the start of frame `f`.
    pub fn samples_at_frame(self, f: i64, sr: SampleRate) -> Samples {
        let (n, d) = self.rational();
        if n == 0 {
            return 0;
        }
        // Use the first integer sample whose floored frame lookup is `f`; nearest rounding
        // can put fractional-rate frame starts just before their own lookup interval.
        let numerator = i128::from(f) * i128::from(sr.hz()) * i128::from(d);
        let denominator = i128::from(n);
        let floor = numerator.div_euclid(denominator);
        let v = floor + i128::from(numerator.rem_euclid(denominator) != 0);
        i64::try_from(v).unwrap_or(if v < 0 { i64::MIN } else { i64::MAX })
    }

    /// Samples per frame (fractional).
    pub fn samples_per_frame(self, sr: SampleRate) -> f64 {
        let (n, d) = self.rational();
        sr.as_f64() * d as f64 / n as f64
    }
}

/// A timecode label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Timecode {
    pub negative: bool,
    pub hours: i64,
    pub minutes: i64,
    pub seconds: i64,
    pub frames: i64,
}

impl Timecode {
    /// Label for an absolute frame count.
    pub fn from_frames(frames: i64, rate: FrameRate) -> Timecode {
        let negative = frames < 0;
        // Drop-frame labels can exceed i64::MAX even when the frame count fits. Widen before
        // taking the magnitude so i64::MIN is represented exactly too.
        let mut f = i128::from(frames).abs();
        let base = i128::from(rate.nominal().max(1));
        if rate.is_drop() {
            // Drop 2 (or 4) labels each minute except every tenth.
            let drop = if base >= 60 { 4 } else { 2 };
            let per_10min = base * 600 - drop * 9;
            let per_min = base * 60 - drop;
            let tens = f / per_10min;
            let rem = f % per_10min;
            let extra = if rem > drop { drop * 9 * tens + drop * ((rem - drop) / per_min) } else { drop * 9 * tens };
            f += extra;
        }
        let frames_l = f % base;
        let total_secs = f / base;
        Timecode {
            negative,
            hours: i64::try_from(total_secs / 3600).unwrap_or(i64::MAX),
            minutes: i64::try_from((total_secs / 60) % 60).unwrap_or(0),
            seconds: i64::try_from(total_secs % 60).unwrap_or(0),
            frames: i64::try_from(frames_l).unwrap_or(0),
        }
    }

    /// Absolute frame count of this label.
    pub fn to_frames(&self, rate: FrameRate) -> i64 {
        let base = rate.nominal().max(1);
        let total_min = i128::from(self.hours) * 60 + i128::from(self.minutes);
        let mut f = (total_min * 60 + i128::from(self.seconds)) * i128::from(base) + i128::from(self.frames);
        if rate.is_drop() {
            let drop = if base >= 60 { 4 } else { 2 };
            f -= drop * (total_min - total_min / 10);
        }
        let f = if self.negative { -f } else { f };
        i64::try_from(f).unwrap_or(if f < 0 { i64::MIN } else { i64::MAX })
    }

    pub fn from_samples(s: Samples, rate: FrameRate, sr: SampleRate) -> Timecode {
        Timecode::from_frames(rate.frames_at(s, sr), rate)
    }

    pub fn to_samples(&self, rate: FrameRate, sr: SampleRate) -> Samples {
        rate.samples_at_frame(self.to_frames(rate), sr)
    }

    pub fn format(&self, rate: FrameRate) -> String {
        let sep = if rate.is_drop() { ';' } else { ':' };
        let sign = if self.negative { "-" } else { "" };
        format!("{sign}{:02}:{:02}:{:02}{sep}{:02}", self.hours, self.minutes, self.seconds, self.frames)
    }

    /// Parse `HH:MM:SS:FF` (also `;` / `.` separators and fewer fields, right-aligned).
    pub fn parse(text: &str) -> Result<Timecode, TimeError> {
        let t = text.trim();
        let (negative, t) = match t.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, t),
        };
        if t.len() > 80 {
            return Err(TimeError::Parse("timecode is too long".into(), "timecode"));
        }
        if t.bytes().filter(|b| matches!(b, b':' | b';' | b'.')).count() > 3 {
            return Err(TimeError::Parse(text.to_string(), "timecode"));
        }
        let parts: Vec<&str> = t.split([':', ';', '.']).collect();
        if parts.is_empty() || parts.len() > 4 {
            return Err(TimeError::Parse(text.to_string(), "timecode"));
        }
        let mut nums = [0i64; 4];
        let offset = 4 - parts.len();
        for (i, p) in parts.iter().enumerate() {
            let v: i64 = p.trim().parse().map_err(|_| TimeError::Parse(text.to_string(), "timecode"))?;
            if v < 0 {
                return Err(TimeError::Parse(text.to_string(), "timecode"));
            }
            if let Some(slot) = nums.get_mut(offset + i) {
                *slot = v;
            }
        }
        Ok(Timecode { negative, hours: nums[0], minutes: nums[1], seconds: nums[2], frames: nums[3] })
    }
}

/// Feet+Frames for 35 mm film (16 frames per foot), `start` frames in at sample 0.
pub fn feet_frames(s: Samples, rate: FrameRate, sr: SampleRate, start: i64) -> (i64, i64) {
    let f = rate.frames_at(s, sr).saturating_add(start);
    (f.div_euclid(16), f.rem_euclid(16))
}

