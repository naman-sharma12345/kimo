// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/time,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Grid and Nudge values: musical or absolute step sizes, grid lines and snapping.

use super::{FrameRate, SampleRate, Samples, TICKS_PER_QUARTER, TempoMap, Timecode, to_samples};

/// Note values for Bars|Beats grids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NoteValue {
    Bar,
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

impl NoteValue {
    /// Every musical increment in menu order, from largest to smallest.
    pub const ALL: [NoteValue; 8] = [
        NoteValue::Bar,
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
        NoteValue::SixtyFourth,
    ];
    /// Returns the nominal tick length; bars are adjusted for the active meter when used.
    pub fn ticks(self) -> i64 {
        match self {
            NoteValue::Bar => TICKS_PER_QUARTER * 4,
            NoteValue::Whole => TICKS_PER_QUARTER * 4,
            NoteValue::Half => TICKS_PER_QUARTER * 2,
            NoteValue::Quarter => TICKS_PER_QUARTER,
            NoteValue::Eighth => TICKS_PER_QUARTER / 2,
            NoteValue::Sixteenth => TICKS_PER_QUARTER / 4,
            NoteValue::ThirtySecond => TICKS_PER_QUARTER / 8,
            NoteValue::SixtyFourth => TICKS_PER_QUARTER / 16,
        }
    }
    /// Returns the menu label for this musical increment.
    pub fn label(self) -> &'static str {
        match self {
            NoteValue::Bar => "1 bar",
            NoteValue::Whole => "Whole note",
            NoteValue::Half => "1/2 note",
            NoteValue::Quarter => "1/4 note",
            NoteValue::Eighth => "1/8 note",
            NoteValue::Sixteenth => "1/16 note",
            NoteValue::ThirtySecond => "1/32 note",
            NoteValue::SixtyFourth => "1/64 note",
        }
    }
}

/// The time scale used by a Grid or Nudge value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum GridScale {
    BarsBeats,
    MinSecs,
    Timecode,
    FeetFrames,
    Samples,
    ClipsMarkers,
}

impl GridScale {
    /// Numeric time scales shared by Grid and Nudge.
    pub const TIME: [GridScale; 5] = [GridScale::BarsBeats, GridScale::MinSecs, GridScale::Timecode, GridScale::FeetFrames, GridScale::Samples];

    /// Returns the user-facing scale label.
    pub fn label(self) -> &'static str {
        match self {
            GridScale::BarsBeats => "Bars|Beats",
            GridScale::MinSecs => "Min:Secs",
            GridScale::Timecode => "Timecode",
            GridScale::FeetFrames => "Feet+Frames",
            GridScale::Samples => "Samples",
            GridScale::ClipsMarkers => "Clips/Markers",
        }
    }

    /// Returns the stable command and session identifier.
    pub fn id(self) -> &'static str {
        match self {
            GridScale::BarsBeats => "bars_beats",
            GridScale::MinSecs => "min_secs",
            GridScale::Timecode => "timecode",
            GridScale::FeetFrames => "feet_frames",
            GridScale::Samples => "samples",
            GridScale::ClipsMarkers => "clips_markers",
        }
    }

    /// Parses a stable identifier or user-facing label.
    pub fn from_id(id: &str) -> Option<GridScale> {
        Self::TIME.into_iter().chain([GridScale::ClipsMarkers]).find(|scale| scale.id() == id || scale.label().eq_ignore_ascii_case(id))
    }
}

/// Timecode Grid/Nudge increments, ordered independently of the session frame rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TimecodeValue {
    Second,
    /// Five physical frames, independent of skipped drop-frame labels.
    FiveFrames,
    Frame,
    HalfFrame,
    QuarterFrame,
    SubFrame,
}

impl TimecodeValue {
    /// Every Timecode increment in menu order, from largest to smallest.
    pub const ALL: [TimecodeValue; 6] = [
        TimecodeValue::Second,
        TimecodeValue::FiveFrames,
        TimecodeValue::Frame,
        TimecodeValue::HalfFrame,
        TimecodeValue::QuarterFrame,
        TimecodeValue::SubFrame,
    ];

    /// Returns the user-facing increment label.
    pub fn label(self) -> &'static str {
        match self {
            TimecodeValue::Second => "1 second",
            TimecodeValue::FiveFrames => "5 frames",
            TimecodeValue::Frame => "1 frame",
            TimecodeValue::HalfFrame => "1/2 frame",
            TimecodeValue::QuarterFrame => "1/4 frame",
            TimecodeValue::SubFrame => "1 subframe",
        }
    }

    /// Returns the stable command and session identifier.
    pub fn id(self) -> &'static str {
        match self {
            TimecodeValue::Second => "second",
            TimecodeValue::FiveFrames => "5_frames",
            TimecodeValue::Frame => "frame",
            TimecodeValue::HalfFrame => "half_frame",
            TimecodeValue::QuarterFrame => "quarter_frame",
            TimecodeValue::SubFrame => "subframe",
        }
    }

    /// Parses a stable identifier or user-facing label.
    pub fn from_id(id: &str) -> Option<TimecodeValue> {
        Self::ALL.into_iter().find(|value| value.id() == id || value.label().eq_ignore_ascii_case(id))
    }

    /// Returns the increment length in frames at the session rate.
    fn frames(self, rate: FrameRate) -> f64 {
        match self {
            TimecodeValue::Second => rate.nominal() as f64,
            TimecodeValue::FiveFrames => 5.0,
            TimecodeValue::Frame => 1.0,
            TimecodeValue::HalfFrame => 0.5,
            TimecodeValue::QuarterFrame => 0.25,
            TimecodeValue::SubFrame => 0.01,
        }
    }
}

/// Feet+Frames Grid/Nudge increments in physical frames, independent of drop-frame label skips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FeetFramesValue {
    Foot,
    Frame,
    QuarterFrame,
    SubFrame,
}

impl FeetFramesValue {
    /// Every Feet+Frames increment in menu order, from largest to smallest.
    pub const ALL: [FeetFramesValue; 4] = [FeetFramesValue::Foot, FeetFramesValue::Frame, FeetFramesValue::QuarterFrame, FeetFramesValue::SubFrame];

    /// Returns the user-facing increment label.
    pub fn label(self) -> &'static str {
        match self {
            FeetFramesValue::Foot => "1 foot",
            FeetFramesValue::Frame => "1 frame",
            FeetFramesValue::QuarterFrame => "1/4 frame",
            FeetFramesValue::SubFrame => "1 subframe",
        }
    }

    /// Returns the stable command and session identifier.
    pub fn id(self) -> &'static str {
        match self {
            FeetFramesValue::Foot => "foot",
            FeetFramesValue::Frame => "frame",
            FeetFramesValue::QuarterFrame => "quarter_frame",
            FeetFramesValue::SubFrame => "subframe",
        }
    }

    /// Parses a stable identifier or user-facing label.
    pub fn from_id(id: &str) -> Option<FeetFramesValue> {
        Self::ALL.into_iter().find(|value| value.id() == id || value.label().eq_ignore_ascii_case(id))
    }

    /// Returns the increment length in frames.
    fn frames(self) -> f64 {
        let (num, den) = self.frame_ratio();
        num as f64 / den as f64
    }

    /// Exact numerator and denominator of the increment in physical frames.
    fn frame_ratio(self) -> (i128, i128) {
        match self {
            Self::Foot => (16, 1),
            Self::Frame => (1, 1),
            Self::QuarterFrame => (1, 4),
            Self::SubFrame => (1, 100),
        }
    }

    /// Apply the counter origin in frames before rounding once to the first sample at the frame
    /// boundary. This matches `FrameRate::samples_at_frame` and typed Feet+Frames positions.
    fn sample_line(self, index: i128, sr: SampleRate, rate: FrameRate, origin: i64) -> Samples {
        let (num, den) = self.frame_ratio();
        let (rate_num, rate_den) = rate.rational();
        let frames = index * num - i128::from(origin) * den;
        let samples = frames * i128::from(sr.hz().max(1)) * i128::from(rate_den);
        let divisor = den * i128::from(rate_num);
        let rounded = samples.div_euclid(divisor) + i128::from(samples.rem_euclid(divisor) != 0);
        i64::try_from(rounded).unwrap_or(if rounded < 0 { i64::MIN } else { i64::MAX })
    }

    /// Find a line index using exact frame arithmetic, allowing for whole-sample rounding.
    fn line_index(self, position: Samples, sr: SampleRate, rate: FrameRate, origin: i64) -> i128 {
        let (num, den) = self.frame_ratio();
        let (rate_num, rate_den) = rate.rational();
        let scale = i128::from(sr.hz().max(1)) * i128::from(rate_den);
        let frames = i128::from(position) * i128::from(rate_num) + i128::from(origin) * scale;
        let mut index = (frames * den).div_euclid(scale * num);
        if self.sample_line(index + 1, sr, rate, origin) <= position {
            index += 1;
        }
        index
    }

    /// The Feet+Frames line at or before a sample, relative to the counter's frame origin.
    pub fn line_at_or_before(self, position: Samples, sr: SampleRate, rate: FrameRate, origin: i64) -> Samples {
        self.sample_line(self.line_index(position, sr, rate, origin), sr, rate, origin)
    }

    /// The next distinct sample line; subframe increments may share a sample at low sample rates.
    fn next_line(self, position: Samples, sr: SampleRate, rate: FrameRate, origin: i64) -> Samples {
        self.sample_line(self.line_index(position, sr, rate, origin) + 1, sr, rate, origin).max(position.saturating_add(1))
    }

    /// Snap against frame-origin lines, choosing the earlier line when distances are equal.
    pub fn snap(self, position: Samples, sr: SampleRate, rate: FrameRate, origin: i64) -> Samples {
        let lo = self.line_at_or_before(position, sr, rate, origin);
        let hi = self.next_line(lo, sr, rate, origin);
        if i128::from(position) - i128::from(lo) <= i128::from(hi) - i128::from(position) { lo } else { hi }
    }

    /// Distinct Feet+Frames sample lines within `[start, end)`, capped at `max`.
    pub fn lines(self, start: Samples, end: Samples, sr: SampleRate, rate: FrameRate, origin: i64, max: usize) -> Vec<Samples> {
        let mut out = Vec::new();
        let mut line = self.line_at_or_before(start, sr, rate, origin);
        while line < end && out.len() < max {
            if line >= start {
                out.push(line);
            }
            let next = self.next_line(line, sr, rate, origin);
            if next <= line {
                break;
            }
            line = next;
        }
        out
    }
}

/// A grid or nudge step.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GridValue {
    /// Musical: note value, optionally dotted or triplet.
    Note { value: NoteValue, dotted: bool, triplet: bool },
    /// Absolute seconds (Min:Secs grids: 0.001 … 1 s).
    Seconds(f64),
    /// Frames at the session timecode rate.
    /// Kept for sessions and commands written before named timecode increments were added.
    Frames(i64),
    /// A named Timecode increment, including fractional frames and one second.
    Timecode(TimecodeValue),
    /// A Feet+Frames increment (one foot is 16 frames).
    FeetFrames(FeetFramesValue),
    /// Raw samples.
    Samples(i64),
}

impl Default for GridValue {
    /// Starts on a straight sixteenth-note Bars|Beats grid.
    fn default() -> Self {
        GridValue::Note { value: NoteValue::Sixteenth, dotted: false, triplet: false }
    }
}

impl GridValue {
    /// Returns the user-facing label for this increment.
    pub fn label(&self) -> String {
        match self {
            GridValue::Note { value, dotted, triplet } => {
                let mut s = value.label().to_string();
                if *dotted {
                    s.push_str(" dotted");
                }
                if *triplet {
                    s.push_str(" triplet");
                }
                s
            }
            GridValue::Seconds(s) => {
                if *s >= 1.0 {
                    format!("{s} second{}", if *s == 1.0 { "" } else { "s" })
                } else {
                    format!("{} ms", (s * 1000.0).round() as i64)
                }
            }
            GridValue::Frames(f) => format!("{f} frame{}", if *f == 1 { "" } else { "s" }),
            GridValue::Timecode(value) => value.label().to_string(),
            GridValue::FeetFrames(value) => value.label().to_string(),
            GridValue::Samples(n) => format!("{n} sample{}", if *n == 1 { "" } else { "s" }),
        }
    }

    /// Returns the time scale to which this increment belongs.
    pub fn scale(self) -> GridScale {
        match self {
            GridValue::Note { .. } => GridScale::BarsBeats,
            GridValue::Seconds(_) => GridScale::MinSecs,
            GridValue::Frames(_) | GridValue::Timecode(_) => GridScale::Timecode,
            GridValue::FeetFrames(_) => GridScale::FeetFrames,
            GridValue::Samples(_) => GridScale::Samples,
        }
    }

    /// Returns the default increment for a newly selected time scale.
    pub fn for_scale(scale: GridScale) -> GridValue {
        match scale {
            GridScale::BarsBeats => GridValue::default(),
            GridScale::MinSecs => GridValue::Seconds(1.0),
            GridScale::Timecode => GridValue::Timecode(TimecodeValue::Frame),
            GridScale::FeetFrames => GridValue::FeetFrames(FeetFramesValue::Frame),
            GridScale::Samples => GridValue::Samples(100),
            GridScale::ClipsMarkers => GridValue::default(),
        }
    }

    /// The next larger or smaller menu value of the same kind, or `self` at either end. Note values keep
    /// their dot or triplet modifier.
    pub fn stepped(self, larger: bool) -> GridValue {
        /// Finds the next value in a numeric increment ladder without wrapping.
        fn next<T: PartialOrd + Copy>(ladder: &[T], v: T, larger: bool) -> Option<T> {
            if larger { ladder.iter().copied().find(|x| *x > v) } else { ladder.iter().rev().copied().find(|x| *x < v) }
        }
        match self {
            GridValue::Note { value, dotted, triplet } => {
                let i = NoteValue::ALL.iter().position(|v| *v == value).unwrap_or(0);
                let j = if larger { i.checked_sub(1) } else { i.checked_add(1) };
                j.and_then(|j| NoteValue::ALL.get(j)).map_or(self, |&value| GridValue::Note { value, dotted, triplet })
            }
            GridValue::Seconds(s) => next(&[0.001, 0.01, 0.1, 0.5, 1.0], s, larger).map_or(self, GridValue::Seconds),
            GridValue::Frames(f) => next(&[1, 10, 100], f, larger).map_or(self, GridValue::Frames),
            GridValue::Timecode(value) => step_enum(&TimecodeValue::ALL, value, larger).map_or(self, GridValue::Timecode),
            GridValue::FeetFrames(value) => step_enum(&FeetFramesValue::ALL, value, larger).map_or(self, GridValue::FeetFrames),
            GridValue::Samples(n) => next(&[1, 2, 10, 100, 1_000, 10_000], n, larger).map_or(self, GridValue::Samples),
        }
    }

    fn note_ticks(value: NoteValue, dotted: bool, triplet: bool, map: &TempoMap, at_tick: i64) -> i64 {
        let base = if value == NoteValue::Bar { map.meter_at_tick(at_tick).ticks_per_bar() } else { value.ticks() };
        let mut t = base;
        if dotted {
            t = t * 3 / 2;
        }
        if triplet {
            t = t * 2 / 3;
        }
        t.max(1)
    }

    /// The step in samples at position `at` (musical steps depend on tempo).
    pub fn step_samples(&self, at: Samples, sr: SampleRate, map: &TempoMap, rate: FrameRate) -> Samples {
        match *self {
            GridValue::Note { value, dotted, triplet } => {
                let t0 = map.samples_to_ticks(at, sr);
                let dt = Self::note_ticks(value, dotted, triplet, map, t0);
                (map.tick_to_samples(t0.saturating_add(dt), sr).saturating_sub(map.tick_to_samples(t0, sr))).max(1)
            }
            GridValue::Seconds(s) => sr.samples(s.clamp(0.000_01, 3600.0)).max(1),
            GridValue::Frames(f) => to_samples(rate.samples_per_frame(sr) * f.clamp(1, 1_000_000) as f64).max(1),
            GridValue::Timecode(value) => to_samples(rate.samples_per_frame(sr) * value.frames(rate)).max(1),
            GridValue::FeetFrames(value) => to_samples(rate.samples_per_frame(sr) * value.frames()).max(1),
            GridValue::Samples(n) => n.max(1),
        }
    }

    /// Nearest grid line to `pos`.
    pub fn snap(&self, pos: Samples, sr: SampleRate, map: &TempoMap, rate: FrameRate) -> Samples {
        if let Self::FeetFrames(value) = *self {
            return value.snap(pos, sr, rate, 0);
        }
        let lo = self.line_at_or_before(pos, sr, map, rate);
        let hi = self.next_line(lo, sr, map, rate);
        if (pos - lo) <= (hi - pos) { lo } else { hi }
    }

    /// The grid line at or before `pos`.
    pub fn line_at_or_before(&self, pos: Samples, sr: SampleRate, map: &TempoMap, rate: FrameRate) -> Samples {
        match *self {
            Self::FeetFrames(value) => value.line_at_or_before(pos, sr, rate, 0),
            GridValue::Note { value, dotted, triplet } => {
                // Float ticks can land a hair either side of a line's own tick.
                let mut t = map.samples_to_ticks_f(pos, sr).floor() as i64;
                if map.tick_to_samples(t, sr) > pos {
                    t = t.saturating_sub(1);
                } else if map.tick_to_samples(t.saturating_add(1), sr) <= pos {
                    t = t.saturating_add(1);
                }
                // Sub-bar values restart at each bar so odd meters stay aligned. A whole note is
                // always four quarter notes and therefore keeps a continuous tick origin.
                let bb = map.bar_beat_at_tick(t);
                let bar_tick = map.bar_start_tick(bb.bar);
                let origin = if value == NoteValue::Whole || (value == NoteValue::Bar && dotted) { 0 } else { bar_tick };
                let step = Self::note_ticks(value, dotted, triplet, map, origin);
                let k = (t - origin).div_euclid(step);
                map.tick_to_samples(origin + k * step, sr)
            }
            _ => self.abs_line(self.abs_index_at_or_before(pos, sr, rate), sr, rate),
        }
    }

    /// Position of absolute grid line `k` (Seconds, Frames, Samples), at an exact multiple of the step
    /// rounded once, so rounding never accumulates from line to line.
    fn abs_line(&self, k: i64, sr: SampleRate, rate: FrameRate) -> Samples {
        if let Self::FeetFrames(value) = *self {
            return value.sample_line(i128::from(k), sr, rate, 0);
        }
        if matches!(self, GridValue::Timecode(TimecodeValue::Second)) && rate.is_drop() {
            return rate.samples_at_frame(Self::drop_second_frame(k, rate), sr);
        }
        if let Some(frames) = self.frame_step(rate) {
            let whole = frames.round();
            if frames == whole {
                return rate.samples_at_frame(k.saturating_mul(whole as i64), sr);
            }
            let step = (rate.samples_per_frame(sr) * frames).max(1.0);
            return to_samples(k as f64 * step);
        }
        match *self {
            GridValue::Seconds(s) => to_samples(k as f64 * Self::seconds_step(s, sr)),
            GridValue::Samples(n) => k.saturating_mul(n.max(1)),
            GridValue::Note { .. } => 0,
            GridValue::Frames(_) | GridValue::Timecode(_) | GridValue::FeetFrames(_) => 0,
        }
    }

    /// Index of the absolute grid line at or before `pos`.
    fn abs_index_at_or_before(&self, pos: Samples, sr: SampleRate, rate: FrameRate) -> i64 {
        if matches!(self, GridValue::Timecode(TimecodeValue::Second)) && rate.is_drop() {
            let tc = Timecode::from_frames(rate.frames_at(pos, sr), rate);
            let magnitude = tc.hours.saturating_mul(3600).saturating_add(tc.minutes.saturating_mul(60)).saturating_add(tc.seconds);
            let mut k = if tc.negative { magnitude.saturating_neg() } else { magnitude };
            while self.abs_line(k, sr, rate) > pos {
                let previous = k.saturating_sub(1);
                if previous == k {
                    break;
                }
                k = previous;
            }
            while self.abs_line(k.saturating_add(1), sr, rate) <= pos {
                let next = k.saturating_add(1);
                if next == k {
                    break;
                }
                k = next;
            }
            return k;
        }
        let mut k = if let Some(frames) = self.frame_step(rate) {
            let step = (rate.samples_per_frame(sr) * frames).max(1.0);
            (pos as f64 / step).floor() as i64
        } else {
            match *self {
                GridValue::Seconds(s) => (pos as f64 / Self::seconds_step(s, sr)).floor() as i64,
                GridValue::Samples(n) => return pos.div_euclid(n.max(1)),
                GridValue::Note { .. } => return 0,
                GridValue::Frames(_) | GridValue::Timecode(_) | GridValue::FeetFrames(_) => return 0,
            }
        };
        // Lines are rounded to whole samples, so the floored index can be one off either way.
        if self.abs_line(k, sr, rate) > pos {
            k = k.saturating_sub(1);
        } else if self.abs_line(k.saturating_add(1), sr, rate) <= pos {
            k = k.saturating_add(1);
        }
        k
    }

    /// The exact (fractional) Seconds step in samples, at least one sample.
    fn seconds_step(s: f64, sr: SampleRate) -> f64 {
        (s.clamp(0.000_01, 3600.0) * sr.as_f64()).max(1.0)
    }

    /// Maps a signed whole-second timecode label to its physical frame at a drop-frame rate.
    fn drop_second_frame(index: i64, rate: FrameRate) -> i64 {
        let seconds = index.unsigned_abs();
        let total_minutes = seconds / 60;
        let frames = if seconds.is_multiple_of(60) && !total_minutes.is_multiple_of(10) { if rate.nominal() >= 60 { 4 } else { 2 } } else { 0 };
        Timecode {
            negative: index < 0,
            hours: i64::try_from(seconds / 3600).unwrap_or(i64::MAX),
            minutes: i64::try_from((seconds / 60) % 60).unwrap_or(0),
            seconds: i64::try_from(seconds % 60).unwrap_or(0),
            frames,
        }
        .to_frames(rate)
    }

    /// A frame-based increment as a possibly fractional number of frames.
    fn frame_step(&self, rate: FrameRate) -> Option<f64> {
        match *self {
            GridValue::Frames(frames) => Some(frames.clamp(1, 1_000_000) as f64),
            GridValue::Timecode(value) => Some(value.frames(rate)),
            GridValue::FeetFrames(value) => Some(value.frames()),
            _ => None,
        }
    }

    /// The first grid line strictly after `line`.
    pub fn next_line(&self, line: Samples, sr: SampleRate, map: &TempoMap, rate: FrameRate) -> Samples {
        if let Self::FeetFrames(value) = *self {
            return value.next_line(line, sr, rate, 0);
        }
        if !matches!(self, GridValue::Note { .. }) {
            let next = self.abs_line(self.abs_index_at_or_before(line, sr, rate).saturating_add(1), sr, rate);
            return if next > line { next } else { line.saturating_add(1) };
        }
        let step = self.step_samples(line, sr, map, rate);
        let candidate = self.line_at_or_before(line.saturating_add(step), sr, map, rate);
        let next = if candidate > line { candidate } else { line.saturating_add(step) };
        if matches!(self, GridValue::Note { value: NoteValue::Bar, .. }) {
            return next;
        }
        let tick = map.samples_to_ticks(line, sr);
        let bar = map.bar_beat_at_tick(tick).bar;
        let next_bar = map.tick_to_samples(map.bar_start_tick(bar.saturating_add(1)), sr);
        if next_bar > line { next.min(next_bar) } else { next }
    }

    /// All grid lines within `[start, end)`, capped at `max` lines.
    pub fn lines(&self, start: Samples, end: Samples, sr: SampleRate, map: &TempoMap, rate: FrameRate, max: usize) -> Vec<Samples> {
        if let Self::FeetFrames(value) = *self {
            return value.lines(start, end, sr, rate, 0, max);
        }
        let mut out = Vec::new();
        let mut p = self.line_at_or_before(start, sr, map, rate);
        while p < end && out.len() < max {
            if p >= start {
                out.push(p);
            }
            let n = self.next_line(p, sr, map, rate);
            if n <= p {
                break;
            }
            p = n;
        }
        out
    }
}

/// Moves one position through an ordered increment menu without wrapping.
fn step_enum<T: Copy + PartialEq>(values: &[T], value: T, larger: bool) -> Option<T> {
    let index = values.iter().position(|candidate| *candidate == value)?;
    let next = if larger { index.checked_sub(1) } else { index.checked_add(1) }?;
    values.get(next).copied()
}


