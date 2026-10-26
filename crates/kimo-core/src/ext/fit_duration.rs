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
use super::kit::*;
use crate::commands::dispatch_nested as call;
use crate::model::{Project, Source, TimeMode};

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
    let threshold_db = opt_num(a, "threshold_db", -60.0);
    let safety_margin_s = opt_num(a, "safety_margin_s", 0.05);
    if !safety_margin_s.is_finite() || safety_margin_s < 0.0 {
        return Err("safety_margin_s must be a finite number >= 0".into());
    }
    let retime_captions = match a.get("retime_captions") {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => *b,
        _ => return Err("retime_captions must be a boolean".into()),
    };

    if original_duration <= max_duration + 1e-9 {
        return Ok(json!({
            "clip_id": clip_id, "original_duration": original_duration, "max_duration": max_duration,
            "needed_seconds": 0.0, "silence_candidates": 0, "silence_available_seconds": 0.0,
            "spans_removed": 0, "removed_seconds": 0.0, "new_duration": original_duration,
            "met_target": true, "shortfall_seconds": 0.0, "retime_captions": retime_captions,
            "captions_shifted": 0, "captions_trimmed": 0, "captions_removed": 0
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
            "retime_captions": retime_captions, "captions_shifted": 0, "captions_trimmed": 0, "captions_removed": 0
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
        "captions_removed": trim.get("captions_removed").cloned().unwrap_or(json!(0))
    }))
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "fit_clip_duration",
        description: "Shrink one media clip to at most max_duration seconds by trimming its own quietest pauses. Largest pause first, the audio_probe RMS silence rule, partial on only the last pause needed for an exact fit -- never cuts into detected speech. Caption/text clips auto_captions built from this clip retime with it (retime_captions, default true), same as trim_filler_words. No-op if the clip already fits. Reports met_target and shortfall_seconds honestly when there isn't enough silence to reach the target -- never cuts speech to force it.",
        mutates: true,
        schema: obj(vec![
            ("clip_id", s("the media clip to shrink")),
            ("max_duration", n("target duration in seconds, > 0")),
            ("min_silence_ms", n("minimum pause length counted as trimmable, default 300 ms")),
            ("threshold_db", n("RMS silence threshold, default -60 dBFS")),
            ("safety_margin_s", n("seconds shaved off each end of every detected pause before it's used, so a trim never lands right at speech's edge; default 0.05")),
            ("retime_captions", json!({"type":"boolean","description":"also retime caption/text clips auto_captions built from this clip's own words, same as trim_filler_words. Default true."})),
        ], &["clip_id", "max_duration"]),
        run,
    }]
}
