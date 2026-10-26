//! fit_clip_duration: shrink ONE media clip to a target duration by trimming its own pauses (the
//! audio_probe RMS silence rule) first -- the "lowest-value" seconds a short can lose without cutting
//! speech -- largest pause first, taking only as much of the last one as needed for an exact fit.
//!
//! New, narrow tool. Composes two EXISTING tools (`audio_speech_intervals`, `trim_filler_words`) through
//! `dispatch_nested`, the same "no existing primitive changes" pattern `trim_filler_words` itself already
//! follows for `lift_range` (see filler_trim.rs's own doc comment) -- this file does not touch
//! auto_rough_cut, lint_edit, or any ripple/split/trim primitive's own behaviour, and adds nothing to the
//! DIRECTOR list.
//!
//! Coordinate: `audio_speech_intervals` with `target: {clip_id}` decodes exactly this clip's own current
//! source span (`crate::audio::strict_decode` drops everything before `in_point` -- see its own doc
//! comment), so its silence `start`/`end` land in "source time since this clip's own in_point", the exact
//! coordinate `trim_filler_words`'s own `spans` already expects (see filler_trim.rs). Feeding one tool's
//! silence seconds straight into the other's spans, after a small inward safety margin, is the whole
//! mechanism -- no new coordinate math.
//!
//! `threshold_db: "auto"` (KF7, additive): real phone footage sits its room tone well above the tool's
//! -60 dBFS default (a previous lane needed threshold_db:-30 by hand on real clip3), so a small model
//! calling this with the default finds zero pauses on noisy audio. "auto" measures the clip's OWN noise
//! floor instead of guessing: one `audio_probe` call (read-only, `dispatch_nested`, same pattern as the
//! two calls above) returns `envelope.rms_db[]`, the per-100ms-window RMS dB series -- the SAME
//! measurement `vad::silence_probe` (see perception/vad.rs's own doc comment: "Matches audio_probe's
//! default 100ms stereo-RMS silence windows exactly") compares `threshold_db` against when mode is
//! "silence". `resolve_auto_threshold` below sorts that series and reads two percentiles off it directly
//! (no new audio-analysis primitive -- audio_probe already exposes the raw per-window series, it just
//! doesn't report a percentile of it, so the percentile math lives here): AUTO_FLOOR_PERCENTILE (a LOW
//! percentile, the clip's own quiet room tone) plus a fixed AUTO_MARGIN_DB margin, capped at
//! AUTO_SPEECH_GUARD_DB below AUTO_SPEECH_PERCENTILE (a HIGH percentile, a proxy for the clip's own loud
//! / speech frames) so the threshold can never climb into actual speech even on a clip with little
//! dynamic range. This file does not touch audio_probe.rs or perception/vad.rs: it only reads back data
//! they already compute and expose.
use super::kit::*;
use crate::commands::dispatch_nested as call;
use crate::model::{Project, Source, TimeMode};

/// Low percentile of the clip's own 100ms-window RMS dB series used as its measured noise floor in
/// "auto" mode -- a minority of most talking-head clips is actual pause/room-tone, so a low (not
/// median) percentile is what lands on it rather than on speech.
const AUTO_FLOOR_PERCENTILE: f64 = 20.0;
/// High percentile of the same series, read as a proxy for the clip's own loud (speech) frames --
/// used only to cap the auto threshold below it, never as a value trimmed to.
const AUTO_SPEECH_PERCENTILE: f64 = 90.0;
/// Fixed, documented margin added above the measured floor (within the 6-10 dB band this feature was
/// scoped to): enough above typical phone-mic room tone to make pauses newly visible to the -60
/// default's RMS rule, without being so large it risks nearing quiet speech.
const AUTO_MARGIN_DB: f64 = 8.0;
/// How far below the clip's own loud-frame percentile the auto threshold must stay, so a clip with
/// little headroom between its noise floor and its speech level (loud room tone, a quiet talker) still
/// gets a threshold that sits under speech rather than floor_db + AUTO_MARGIN_DB regardless.
const AUTO_SPEECH_GUARD_DB: f64 = 10.0;
/// threshold_db used for "auto" on a clip with no measurable per-window RMS at all (every window is
/// digital silence, so there is no noise floor to read a percentile off) -- the tool's own long-standing
/// default, so this degenerate case behaves exactly as it always has rather than erroring.
const AUTO_FALLBACK_DB: f64 = -60.0;
/// Placeholder "measured floor" reported alongside AUTO_FALLBACK_DB in that same degenerate case, so the
/// result is honest that nothing was actually measured (matches vad.rs's own sentinel for "no levels").
const AUTO_FALLBACK_FLOOR_DB: f64 = -200.0;

enum ThresholdSpec {
    Fixed(f64),
    Auto,
}

/// Parses `threshold_db` on its own, before any audio is touched, so a bad value (anything that is
/// neither a finite number nor the string "auto") is rejected with a clear error up front -- including
/// on a clip that would otherwise take the already-fits no-op path below.
fn parse_threshold_spec(a: &Value) -> Result<ThresholdSpec, String> {
    match a.get("threshold_db") {
        None | Some(Value::Null) => Ok(ThresholdSpec::Fixed(-60.0)),
        Some(Value::String(s)) if s == "auto" => Ok(ThresholdSpec::Auto),
        Some(Value::Number(n)) => match n.as_f64() {
            Some(f) if f.is_finite() => Ok(ThresholdSpec::Fixed(f)),
            _ => Err("threshold_db must be a finite number or the string \"auto\"".into()),
        },
        _ => Err("threshold_db must be a number or the string \"auto\" (got something else)".into()),
    }
}

/// Nearest-rank percentile of an ascending-sorted, non-empty slice; `p` in [0, 100].
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// This clip's own per-100ms-window RMS dB series, via one read-only `audio_probe` call -- ascending
/// sorted, with the (rare) digital-silence `null` windows dropped. `max_points: 2000` (audio_probe's own
/// cap) keeps windows at their native 100ms resolution for any clip under ~200s, so the series handed
/// back is the exact one `vad::silence_probe`'s threshold comparison itself uses, not a downsampled
/// stand-in for it.
fn window_db_sorted(ss: &mut Session, clip_id: &str) -> Result<Vec<f64>, String> {
    let probe = call(ss, "audio_probe", &json!({
        "target": {"clip_id": clip_id}, "window_ms": 100.0, "max_points": 2000
    }))?;
    let mut vals: Vec<f64> = probe["envelope"]["rms_db"].as_array().cloned().unwrap_or_default()
        .iter().filter_map(Value::as_f64).filter(|v| v.is_finite()).collect();
    vals.sort_by(f64::total_cmp);
    Ok(vals)
}

/// Resolves "auto" to an actual (threshold_db, measured_noise_floor_db) pair. See the module doc comment
/// for the percentile/margin/cap rule; this is the only place that rule lives.
fn resolve_auto_threshold(ss: &mut Session, clip_id: &str) -> Result<(f64, f64), String> {
    let sorted = window_db_sorted(ss, clip_id)?;
    if sorted.is_empty() {
        return Ok((AUTO_FALLBACK_DB, AUTO_FALLBACK_FLOOR_DB));
    }
    let floor = percentile(&sorted, AUTO_FLOOR_PERCENTILE);
    let speech_ref = percentile(&sorted, AUTO_SPEECH_PERCENTILE);
    let threshold = (floor + AUTO_MARGIN_DB).min(speech_ref - AUTO_SPEECH_GUARD_DB);
    Ok((threshold, floor))
}

/// Fixed(f) is already resolved; Auto measures the clip now (one audio_probe call) and returns the
/// floor alongside the threshold chosen from it, so both ride the result honestly.
fn resolve_threshold(ss: &mut Session, clip_id: &str, spec: &ThresholdSpec) -> Result<(f64, Option<f64>), String> {
    match spec {
        ThresholdSpec::Fixed(f) => Ok((*f, None)),
        ThresholdSpec::Auto => {
            let (t, floor) = resolve_auto_threshold(ss, clip_id)?;
            Ok((t, Some(floor)))
        }
    }
}

fn floor_json(floor: Option<f64>) -> Value {
    match floor {
        Some(f) => json!(f),
        None => Value::Null,
    }
}

fn loc(p: &Project, id: &str) -> Result<(usize, usize), String> {
    for (ti, t) in p.tracks.iter().enumerate() {
        if let Some(ci) = t.clips.iter().position(|c| c.id == id) {
            return Ok((ti, ci));
        }
    }
    Err(format!("no such clip '{id}'"))
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let clip_id = str_arg(a, "clip_id")?.to_string();
    let max_duration = num_arg(a, "max_duration")?;
    if !(max_duration.is_finite() && max_duration > 0.0) {
        return Err("max_duration must be a finite number > 0".into());
    }
    let threshold_spec = parse_threshold_spec(a)?;
    let (ti, ci) = loc(&ss.project, &clip_id)?;
    if ss.project.tracks[ti].locked {
        return Err(format!("track {} is locked", ss.project.tracks[ti].id));
    }
    let c = ss.project.tracks[ti].clips[ci].clone();
    if !matches!(c.source, Source::Media { .. }) {
        return Err(format!("fit_clip_duration needs a media clip: '{clip_id}' is not one"));
    }
    if !matches!(c.time_mode, TimeMode::Normal) {
        return Err("fit_clip_duration needs time_mode normal (no freeze/reverse)".into());
    }
    if c.time_remap.is_some() || c.speed_curve.is_some() {
        return Err("fit_clip_duration needs a clip with a plain speed (no time remap or speed ramp)".into());
    }
    if !(c.speed.is_finite() && c.speed > 0.0) {
        return Err("clip speed must be a finite number > 0".into());
    }

    let original_duration = c.duration;
    let min_silence_ms = opt_num(a, "min_silence_ms", 300.0);
    let safety_margin_s = opt_num(a, "safety_margin_s", 0.05);
    if !safety_margin_s.is_finite() || safety_margin_s < 0.0 {
        return Err("safety_margin_s must be a finite number >= 0".into());
    }
    let retime_captions = match a.get("retime_captions") {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => *b,
        _ => return Err("retime_captions must be a boolean".into()),
    };

    // Resolved once, ahead of the already-fits check below, so "auto"'s threshold_db and
    // measured_noise_floor_db are reported the same way on every return path (a no-op clip still
    // reports what its own noise floor measured at, it just never acts on it).
    let (threshold_db, measured_noise_floor_db) = resolve_threshold(ss, &clip_id, &threshold_spec)?;

    if original_duration <= max_duration + 1e-9 {
        return Ok(json!({
            "clip_id": clip_id, "original_duration": original_duration, "max_duration": max_duration,
            "needed_seconds": 0.0, "silence_candidates": 0, "silence_available_seconds": 0.0,
            "spans_removed": 0, "removed_seconds": 0.0, "new_duration": original_duration,
            "met_target": true, "shortfall_seconds": 0.0, "retime_captions": retime_captions,
            "captions_shifted": 0, "captions_trimmed": 0, "captions_removed": 0,
            "threshold_db": threshold_db, "measured_noise_floor_db": floor_json(measured_noise_floor_db)
        }));
    }

    // Timeline seconds needed, converted to this clip's own source-time clock (the same conversion
    // trim_filler_words' `spans` already live in -- see its `to_timeline`/`source_time` use).
    let needed = (original_duration - max_duration) * c.speed;

    let intervals = call(ss, "audio_speech_intervals", &json!({
        "target": {"clip_id": clip_id}, "mode": "silence",
        "min_silence_ms": min_silence_ms, "threshold_db": threshold_db, "max_items": 1000
    }))?;
    let mut silences: Vec<(f64, f64)> = intervals["silence"].as_array().cloned().unwrap_or_default()
        .iter()
        .filter_map(|v| Some((v.get("start")?.as_f64()?, v.get("end")?.as_f64()?)))
        // inward safety margin: never trim right up to the detected edge of a pause, where the
        // window-granularity RMS measure could still be touching speech.
        .map(|(s, e)| (s + safety_margin_s, e - safety_margin_s))
        .filter(|(s, e)| e - s > 1e-6)
        .collect();
    silences.sort_by(|x, y| (y.1 - y.0).partial_cmp(&(x.1 - x.0)).unwrap());

    let silence_candidates = silences.len();
    let silence_available: f64 = silences.iter().map(|(s, e)| e - s).sum();

    let mut remaining = needed;
    let mut spans: Vec<(f64, f64)> = vec![];
    for (s, e) in &silences {
        if remaining <= 1e-9 {
            break;
        }
        let dur = e - s;
        if dur <= remaining + 1e-9 {
            spans.push((*s, *e));
            remaining -= dur;
        } else {
            // partial: take only the tail of this pause (right up to where speech resumes), so the last
            // span spends only as much silence as actually needed for an exact fit.
            spans.push((e - remaining, *e));
            remaining = 0.0;
        }
    }
    spans.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());

    if spans.is_empty() {
        return Ok(json!({
            "clip_id": clip_id, "original_duration": original_duration, "max_duration": max_duration,
            "needed_seconds": needed, "silence_candidates": silence_candidates,
            "silence_available_seconds": silence_available, "spans_removed": 0, "removed_seconds": 0.0,
            "new_duration": original_duration, "met_target": false,
            "shortfall_seconds": needed / c.speed,
            "retime_captions": retime_captions, "captions_shifted": 0, "captions_trimmed": 0, "captions_removed": 0,
            "threshold_db": threshold_db, "measured_noise_floor_db": floor_json(measured_noise_floor_db)
        }));
    }

    let trim = call(ss, "trim_filler_words", &json!({
        "clip_id": clip_id,
        "spans": spans.iter().map(|(s, e)| json!({"start": s, "end": e})).collect::<Vec<_>>(),
        "pad": 0.0, "retime_captions": retime_captions
    }))?;

    // `clip_id` may now name only ONE of several fragments left behind by multiple ripple-deletes (the
    // same reason trim_filler_words itself returns a `clip_ids` list) -- the authoritative total is its
    // own `removed_seconds` (already in timeline seconds; see filler_trim.rs's `total_removed`), not a
    // single fragment's own `duration` looked up again.
    let removed_seconds_timeline = trim.get("removed_seconds").and_then(Value::as_f64).unwrap_or(0.0);
    let new_duration = (original_duration - removed_seconds_timeline).max(0.0);
    let shortfall = (new_duration - max_duration).max(0.0);

    Ok(json!({
        "clip_id": clip_id, "original_duration": original_duration, "max_duration": max_duration,
        "needed_seconds": needed, "silence_candidates": silence_candidates,
        "silence_available_seconds": silence_available,
        "spans_removed": trim.get("removed_spans").cloned().unwrap_or(json!(spans.len())),
        "removed_seconds": trim.get("removed_seconds").cloned().unwrap_or(json!(0.0)),
        "new_duration": new_duration, "met_target": shortfall <= 1e-6, "shortfall_seconds": shortfall,
        "retime_captions": retime_captions,
        "captions_shifted": trim.get("captions_shifted").cloned().unwrap_or(json!(0)),
        "captions_trimmed": trim.get("captions_trimmed").cloned().unwrap_or(json!(0)),
        "captions_removed": trim.get("captions_removed").cloned().unwrap_or(json!(0)),
        "threshold_db": threshold_db, "measured_noise_floor_db": floor_json(measured_noise_floor_db)
    }))
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "fit_clip_duration",
        description: "Shrink one media clip to at most max_duration seconds by trimming its own quietest pauses. Largest pause first, the audio_probe RMS silence rule, partial on only the last pause needed for an exact fit -- never cuts into detected speech. threshold_db also accepts \"auto\": it measures this clip's own noise floor (a low percentile of its window RMS, the exact measurement the silence rule itself compares against) and sets the threshold a fixed margin above it, capped below the clip's own speech level, so a noisy phone recording's room tone doesn't hide every pause behind the -60 default; the chosen threshold_db and measured_noise_floor_db ride the result either way. Caption/text clips auto_captions built from this clip retime with it (retime_captions, default true), same as trim_filler_words. No-op if the clip already fits. Reports met_target and shortfall_seconds honestly when there isn't enough silence to reach the target -- never cuts speech to force it.",
        mutates: true,
        schema: obj(vec![
            ("clip_id", s("the media clip to shrink")),
            ("max_duration", n("target duration in seconds, > 0")),
            ("min_silence_ms", n("minimum pause length counted as trimmable, default 300 ms")),
            ("threshold_db", json!({"description":"RMS silence threshold in dBFS, default -60. Or the string \"auto\": measures this clip's own noise floor (a low percentile of its own window RMS) and sets the threshold a fixed margin above that floor, capped below the clip's own speech level, instead of a fixed guess -- useful for phone-mic room tone the -60 default can't see pauses under."})),
            ("safety_margin_s", n("seconds shaved off each end of every detected pause before it's used, so a trim never lands right at speech's edge; default 0.05")),
            ("retime_captions", json!({"type":"boolean","description":"also retime caption/text clips auto_captions built from this clip's own words, same as trim_filler_words. Default true."})),
        ], &["clip_id", "max_duration"]),
        run,
    }]
}
