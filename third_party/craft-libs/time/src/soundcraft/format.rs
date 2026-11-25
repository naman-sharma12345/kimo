// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/time,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Formatting and parsing positions in the five timebases.

use super::{BarBeat, FrameRate, SampleRate, Samples, TempoMap, TimeError, Timecode, timecode::feet_frames};

/// The counters' and rulers' timebases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum TimeFormat {
    BarsBeats,
    #[default]
    MinSecs,
    Timecode,
    FeetFrames,
    Samples,
}

impl TimeFormat {
    pub const ALL: [TimeFormat; 5] = [TimeFormat::BarsBeats, TimeFormat::MinSecs, TimeFormat::Timecode, TimeFormat::FeetFrames, TimeFormat::Samples];

    pub fn label(self) -> &'static str {
        match self {
            TimeFormat::BarsBeats => "Bars|Beats",
            TimeFormat::MinSecs => "Min:Secs",
            TimeFormat::Timecode => "Timecode",
            TimeFormat::FeetFrames => "Feet+Frames",
            TimeFormat::Samples => "Samples",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            TimeFormat::BarsBeats => "bars_beats",
            TimeFormat::MinSecs => "min_secs",
            TimeFormat::Timecode => "timecode",
            TimeFormat::FeetFrames => "feet_frames",
            TimeFormat::Samples => "samples",
        }
    }

    pub fn from_id(id: &str) -> Option<TimeFormat> {
        TimeFormat::ALL.into_iter().find(|f| f.id() == id || f.label().eq_ignore_ascii_case(id))
    }
}

fn min_secs(s: Samples, sr: SampleRate) -> String {
    let neg = s < 0;
    // Integer maths: a float `seconds * 1000` lands just below exact millisecond positions.
    let ms_total = i64::try_from(i128::from(s).abs() * 1000 / i128::from(sr.hz().max(1))).unwrap_or(i64::MAX);
    let mins = ms_total / 60_000;
    let secs = (ms_total / 1000) % 60;
    let ms = ms_total % 1000;
    format!("{}{mins}:{secs:02}.{ms:03}", if neg { "-" } else { "" })
}

/// Format an absolute position. `tc_start` is the timecode of sample 0 in samples, `ff_start` the
/// feet+frames reading of sample 0 in frames.
pub fn format_position(s: Samples, fmt: TimeFormat, sr: SampleRate, map: &TempoMap, rate: FrameRate, tc_start: Samples, ff_start: i64) -> String {
    match fmt {
        TimeFormat::Samples => s.to_string(),
        TimeFormat::MinSecs => min_secs(s, sr),
        TimeFormat::Timecode => Timecode::from_samples(s.saturating_add(tc_start), rate, sr).format(rate),
        TimeFormat::FeetFrames => {
            let (ft, fr) = feet_frames(s, rate, sr, ff_start);
            format!("{ft}+{fr:02}")
        }
        TimeFormat::BarsBeats => {
            let bb = map.bar_beat_at(s, sr);
            format!("{}| {}| {:03}", bb.bar, bb.beat, bb.tick)
        }
    }
}

/// Format a length starting at `start` (Bars|Beats lengths count from 0|0|000).
pub fn format_length(start: Samples, len: Samples, fmt: TimeFormat, sr: SampleRate, map: &TempoMap, rate: FrameRate) -> String {
    match fmt {
        TimeFormat::BarsBeats => {
            let t0 = map.samples_to_ticks(start, sr);
            let t1 = map.samples_to_ticks(start.saturating_add(len), sr);
            let m = map.meter_at_tick(t0);
            let d = t1.saturating_sub(t0).max(0);
            let bars = d / m.ticks_per_bar().max(1);
            let rem = d % m.ticks_per_bar().max(1);
            format!("{bars}| {}| {:03}", rem / m.ticks_per_beat().max(1), rem % m.ticks_per_beat().max(1))
        }
        TimeFormat::Timecode => Timecode::from_samples(len, rate, sr).format(rate),
        other => format_position(len, other, sr, map, rate, 0, 0),
    }
}

/// Parse a position typed into a counter or dialog.
pub fn parse_position(
    text: &str,
    fmt: TimeFormat,
    sr: SampleRate,
    map: &TempoMap,
    rate: FrameRate,
    tc_start: Samples,
    ff_start: i64,
) -> Result<Samples, TimeError> {
    let t = text.trim();
    let err = || TimeError::Parse(t.to_string(), fmt.label());
    match fmt {
        TimeFormat::Samples => t.replace([',', '_'], "").parse::<i64>().map_err(|_| err()),
        TimeFormat::MinSecs => {
            let (neg, body) = match t.strip_prefix('-') {
                Some(r) => (true, r),
                None => (false, t),
            };
            let (m, s) = match body.split_once(':') {
                Some((m, s)) => (m.trim().parse::<f64>().map_err(|_| err())?, s.trim().parse::<f64>().map_err(|_| err())?),
                None => (0.0, body.parse::<f64>().map_err(|_| err())?),
            };
            if !m.is_finite() || !s.is_finite() || m < 0.0 || s < 0.0 {
                return Err(err());
            }
            let v = sr.samples(m * 60.0 + s);
            Ok(if neg { -v } else { v })
        }
        TimeFormat::Timecode => {
            let tc = Timecode::parse(t)?;
            if tc.minutes >= 60 || tc.seconds >= 60 || tc.frames >= rate.nominal().max(1) {
                return Err(err());
            }
            if rate.is_drop() && tc.seconds == 0 && tc.minutes % 10 != 0 {
                let dropped = if rate.nominal() >= 60 { 4 } else { 2 };
                if tc.frames < dropped {
                    return Err(err());
                }
            }
            Ok(tc.to_samples(rate, sr).saturating_sub(tc_start))
        }
        TimeFormat::FeetFrames => {
            let (ft, fr) = t.split_once('+').ok_or_else(err)?;
            let ft: i64 = ft.trim().parse().map_err(|_| err())?;
            let fr: i64 = fr.trim().parse().map_err(|_| err())?;
            Ok(rate.samples_at_frame(ft.saturating_mul(16).saturating_add(fr).saturating_sub(ff_start), sr))
        }
        TimeFormat::BarsBeats => {
            let parts: Vec<i64> =
                t.split(['|', '.', ' ']).filter(|p| !p.is_empty()).map(str::parse::<i64>).collect::<Result<_, _>>().map_err(|_| err())?;
            let bb = BarBeat {
                bar: *parts.first().ok_or_else(err)?,
                beat: parts.get(1).copied().unwrap_or(1).max(1),
                tick: parts.get(2).copied().unwrap_or(0).max(0),
            };
            Ok(map.samples_at_bar_beat(bb, sr))
        }
    }
}


