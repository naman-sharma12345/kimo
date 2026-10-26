//! drop_short_segments (KF12): remove tiny false-start fragments left on a track after a rough cut
//! (auto_rough_cut's own dead-air removal still leaves a few sub-second/sub-second-ish takes between
//! real ones -- on Naman's 6 real clips: 0.73s, 0.87s, 2.33s, 3.07s among 10 segments) and ripple the
//! gap closed, the same way a human editor would clean a rough cut before trimming anything else.
//!
//! Narrow, additive tool. Reuses the EXISTING ripple-delete path (`ripple_delete` in edit.rs -- delete
//! one clip, shift every later clip on the track left by its duration) through `dispatch_nested`, the
//! same "no existing primitive changes" pattern `trim_filler_words`/`fit_clip_duration` already follow
//! (see filler_trim.rs's own doc comment). `ripple_delete` has no caption awareness at all today (it
//! only touches the one track it's given), so a clip's own linked captions -- and any later clip's
//! captions on another track -- get exactly the handling that path already gives them: none. That is
//! deliberate and in scope for this lane (KF12 is "except captions"; KF10's own caption-ripple fix for
//! this exact gap is parked, unpushed, stash "kf10-caption-ripple-parked" -- this file does not touch
//! captions, retime_captions, or that stash).
use super::kit::*;
use crate::commands::dispatch_nested as call;
use crate::model::Source;

const EPS: f64 = 1e-6;

fn bool_arg_opt(a: &Value, k: &str) -> Result<Option<bool>, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => v.as_bool().map(Some).ok_or_else(|| crate::errors::arg_err(a, k, "boolean")) }
}
fn opt_num_checked(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => v.as_f64().ok_or_else(|| crate::errors::arg_err(a, k, "number")) }
}

/// The track with the most `Source::Media` clips (ties keep the lowest index) -- "where the footage
/// lives" without paying for a real speech-detection pass the way lint_edit's own dialogue-track default
/// does (see director_tools.rs's "3 speech: the dialogue track is the lowest track with speech"): a
/// rough-cut timeline's real clips are already the track with by far the most media clips on it, and
/// drop_short_segments never needs to know WHERE speech is, only which track the segments sit on.
fn main_track(ss: &Session) -> Option<String> {
    let mut best: Option<(String, usize)> = None;
    for t in &ss.project.tracks {
        let n = t.clips.iter().filter(|c| matches!(c.source, Source::Media { .. })).count();
        if n == 0 { continue; }
        if best.as_ref().map_or(true, |b| n > b.1) { best = Some((t.id.clone(), n)); }
    }
    best.map(|(id, _)| id)
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let track_id = match a.get("track_id") {
        None | Some(Value::Null) => main_track(ss).ok_or("track_id was not given and no track has a media clip to default to; give track_id")?,
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => return Err(crate::errors::arg_err(a, "track_id", "string")),
    };
    let min_duration_s = opt_num_checked(a, "min_duration_s", 1.0)?;
    if !(min_duration_s.is_finite() && min_duration_s > 0.0) { return Err("min_duration_s must be a finite number > 0".into()); }
    let dry_run = bool_arg_opt(a, "dry_run")?.unwrap_or(false);
    let keep_first_and_last = bool_arg_opt(a, "keep_first_and_last")?.unwrap_or(true);

    let ti = ss.project.tracks.iter().position(|t| t.id == track_id).ok_or_else(|| format!("no such track '{track_id}'"))?;

    // This track's own clips, in TIMELINE order -- Track.clips is insertion order, not start order (it
    // happens to be the same for an auto_rough_cut track, but nothing guarantees it in general), and both
    // "first"/"last" (keep_first_and_last) and a clean ripple need the real left-to-right order.
    let mut ordered: Vec<(String, f64, f64)> = ss.project.tracks[ti].clips.iter().map(|c| (c.id.clone(), c.start, c.duration)).collect();
    ordered.sort_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal));
    let n = ordered.len();
    let total_before_s: f64 = ordered.iter().map(|(_, _, d)| d).sum();

    let mut dropped: Vec<Value> = vec![];
    let mut drop_ids: Vec<String> = vec![];
    for (i, (id, start, dur)) in ordered.iter().enumerate() {
        if *dur >= min_duration_s - EPS { continue; }
        if keep_first_and_last && (i == 0 || i == n - 1) { continue; }
        dropped.push(json!({"clip_id": id, "start": start, "duration": dur}));
        drop_ids.push(id.clone());
    }

    if dry_run {
        let kept_count = n - drop_ids.len();
        let removed_s: f64 = dropped.iter().filter_map(|v| v["duration"].as_f64()).sum();
        return Ok(json!({
            "track_id": track_id, "dry_run": true, "dropped": dropped, "kept_count": kept_count,
            "total_before_s": total_before_s, "total_after_s": (total_before_s - removed_s).max(0.0),
        }));
    }

    if ss.project.tracks[ti].locked { return Err(format!("track '{track_id}' is locked")); }

    // ripple_delete looks the clip up by id and shifts whatever is left of the track at call time, so
    // calling it once per dropped id (in any order) closes every gap correctly without this tool having
    // to track running offsets itself -- the exact "reuse the existing path" the brief asks for.
    for id in &drop_ids { call(ss, "ripple_delete", &json!({"clip_id": id}))?; }

    // Re-read the track rather than subtracting durations locally -- the authoritative post-drop numbers
    // are whatever ripple_delete actually left on the track, the same "re-read, don't recompute" discipline
    // remove_silence_cuts' own caller-facing clip list already follows (see edit.rs).
    let (kept_count, total_after_s) = ss.project.tracks.iter().find(|t| t.id == track_id)
        .map(|t| (t.clips.len(), t.clips.iter().map(|c| c.duration).sum::<f64>())).unwrap_or((0, 0.0));

    Ok(json!({
        "track_id": track_id, "dry_run": false, "dropped": dropped, "kept_count": kept_count,
        "total_before_s": total_before_s, "total_after_s": total_after_s,
    }))
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "drop_short_segments",
        description: "Delete clips shorter than min_duration_s from a track and ripple later clips left to close the gaps. Removes the tiny leftover false-start fragments a rough cut leaves between real takes (on real phone footage: 0.73s, 0.87s, 2.33s, 3.07s stray clips among the keepers), reusing the exact ripple_delete path a manual clean-up would use, one clip at a time, so later clips shift left with no gap. track_id defaults to the track with the most media clips (the main video/dialogue track from auto_rough_cut) when omitted. keep_first_and_last (default true) protects a short opening or closing clip even if it is under min_duration_s, since a true first/last shot is not a false-start fragment. dry_run (default false) reports exactly what would be dropped and changes nothing. Captions get no special handling here, the same as today's ripple_delete: a dropped clip's own linked captions and any later clip's captions are untouched, exactly as the existing delete/ripple path leaves them. Deterministic (same input, same drops) and undoable with undo like any other mutating tool.",
        mutates: true,
        schema: obj(vec![
            ("track_id", s("track to clean up, default: the track with the most media clips")),
            ("min_duration_s", n("clips shorter than this (seconds) are dropped; default 1.0, must be > 0")),
            ("dry_run", json!({"type":"boolean","description":"report what would be dropped, change nothing. Default false."})),
            ("keep_first_and_last", json!({"type":"boolean","description":"never drop the track's own first or last clip even if it is short -- a real opening/closing shot, not a false start. Default true."})),
        ], &[]),
        run,
    }]
}
