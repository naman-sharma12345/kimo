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
//!
//! KF13 fix: on a REAL auto_rough_cut timeline, clip boundaries carry ~1ms float drift (e.g. one clip
//! records its end as 31.8670s while the next clip's own recorded start is 31.8660s -- not bit-identical
//! "touching"). `ripple_delete`'s own `shift_after` (edit.rs) decides whether a surviving clip moves by
//! comparing its start against the cut point with `EPS=1e-6` -- far tighter than that ~1ms drift -- so
//! the ONE clip immediately after an already-shifted drop can fail its own `start >= cut - EPS` check
//! and never move, leaving a real gap on the track (confirmed by eye: a solid black hold on export,
//! see lane KF12's and KF13's proof/ frames). By the monotonicity of clip starts on a valid track, this
//! can only ever strand the SINGLE clip immediately after the cut (if that one clears the threshold,
//! every later clip's strictly larger start clears it too) -- everything else `ripple_delete` already
//! shifts correctly. We do NOT touch `ripple_delete`/`shift_after` or its EPS (no threshold loosening):
//! after each call, we re-read the track's ACTUAL current positions (not the deleted clip's own,
//! possibly-drifted, recorded end) and, only if a real gap remains between the previous surviving clip's
//! end and its new successor's start, snap that one successor's own `start` field directly (via
//! `Project::find_clip_mut`, the same direct-mutation path `filler_trim.rs`'s own retime step already
//! uses to reposition a clip after a ripple) to close it -- nothing else on the track is read from or
//! written to. A negative "gap" (overlap) is left alone: it is not the shape this bug produces (confirmed
//! only as a gap, never an overlap, on the real footage) and there is no existing primitive to resolve an
//! overlap without trimming a kept clip's own duration, which would change content -- out of scope here.
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

/// This track's clips, in TIMELINE order, as (id, start, duration) -- a fresh read, not a cache, since
/// positions move as the loop below ripples drops closed one at a time.
fn ordered_clips(ss: &Session, ti: usize) -> Vec<(String, f64, f64)> {
    let mut v: Vec<(String, f64, f64)> = ss.project.tracks[ti].clips.iter().map(|c| (c.id.clone(), c.start, c.duration)).collect();
    v.sort_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal));
    v
}

/// After `ripple_delete` just removed a clip, re-read the track's REAL current positions starting from
/// `anchor_id` (a clip we know is already correctly placed -- the one immediately before the clip that
/// was just deleted) and close any residual gap(s) chained from there by snapping only the stranded
/// successor's own start -- see the module doc comment for why this is the complete fix (at most one
/// clip strands per delete) and why we loop anyway (cheap insurance, not a known requirement). Returns
/// how many clip starts were corrected. Touches nothing before `anchor_id` and nothing that was already
/// touching (gap <= EPS) or overlapping (gap < 0, left alone -- not this bug's shape).
fn close_gaps_from(ss: &mut Session, ti: usize, anchor_id: &str) -> usize {
    let mut fixed = 0usize;
    let mut anchor = anchor_id.to_string();
    let cap = ss.project.tracks[ti].clips.len() + 1; // can't chain more fixes than there are clips
    for _ in 0..cap {
        let ordered = ordered_clips(ss, ti);
        let Some(ai) = ordered.iter().position(|(id, _, _)| *id == anchor) else { break };
        let anchor_end = ordered[ai].1 + ordered[ai].2;
        let Some((next_id, next_start, _)) = ordered.get(ai + 1).cloned() else { break };
        let gap = next_start - anchor_end;
        if gap <= EPS { break; } // already touching (within float noise), or an overlap -- not our fix
        if let Some(cm) = ss.project.find_clip_mut(&next_id) { cm.start = anchor_end; }
        fixed += 1;
        anchor = next_id;
    }
    fixed
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
    let ordered = ordered_clips(ss, ti);
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

    // drop_ids is already in left-to-right (ascending start) order, since `ordered` is sorted and we
    // walked it in that order above -- so each fix-up below always sees a track that's already correct
    // up to this point, and each step's own "anchor" (the clip right before the one just dropped) is
    // itself trustworthy.
    let mut gaps_closed = 0usize;
    for id in &drop_ids {
        // The clip immediately before `id`, by CURRENT position, captured before the delete -- this is
        // the trusted anchor the fix-up re-ripples from if `ripple_delete`'s own shift misses its target.
        let anchor_id: Option<String> = {
            let ord = ordered_clips(ss, ti);
            ord.iter().position(|(cid, _, _)| cid == id).and_then(|i| if i > 0 { Some(ord[i - 1].0.clone()) } else { None })
        };

        // ripple_delete looks the clip up by id and shifts whatever is left of the track at call time --
        // the exact "reuse the existing path" the brief asks for. Left unmodified; see the module doc
        // comment for why its own shift can still strand the clip immediately after on real footage.
        call(ss, "ripple_delete", &json!({"clip_id": id}))?;

        // KF13 fix-up: close any residual gap this specific delete left, without moving anything that
        // wasn't after it. If `id` had no predecessor (it was the track's own first clip -- only possible
        // with keep_first_and_last:false), there is no trusted anchor to re-ripple from, so nothing is
        // touched here: conservative by design, matching "never move clips that weren't after a dropped
        // clip" when we can't establish a real anchor.
        if let Some(aid) = anchor_id { gaps_closed += close_gaps_from(ss, ti, &aid); }
    }

    // Re-read the track rather than subtracting durations locally -- the authoritative post-drop numbers
    // are whatever ripple_delete (and the fix-up above) actually left on the track, the same "re-read,
    // don't recompute" discipline remove_silence_cuts' own caller-facing clip list already follows (see
    // edit.rs).
    let (kept_count, total_after_s) = ss.project.tracks.iter().find(|t| t.id == track_id)
        .map(|t| (t.clips.len(), t.clips.iter().map(|c| c.duration).sum::<f64>())).unwrap_or((0, 0.0));

    Ok(json!({
        "track_id": track_id, "dry_run": false, "dropped": dropped, "kept_count": kept_count,
        "total_before_s": total_before_s, "total_after_s": total_after_s, "gaps_closed": gaps_closed,
    }))
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "drop_short_segments",
        description: "Delete clips shorter than min_duration_s from a track and ripple later clips left to close the gaps. Removes the tiny leftover false-start fragments a rough cut leaves between real takes (on real phone footage: 0.73s, 0.87s, 2.33s, 3.07s stray clips among the keepers), reusing the exact ripple_delete path a manual clean-up would use, one clip at a time, so later clips shift left with no gap -- including closing a residual float-drift gap that path itself can leave on real footage, reported in the result as gaps_closed. track_id defaults to the track with the most media clips (the main video/dialogue track from auto_rough_cut) when omitted. keep_first_and_last (default true) protects a short opening or closing clip even if it is under min_duration_s, since a true first/last shot is not a false-start fragment. dry_run (default false) reports exactly what would be dropped and changes nothing. Captions get no special handling here, the same as today's ripple_delete: a dropped clip's own linked captions and any later clip's captions are untouched, exactly as the existing delete/ripple path leaves them. Deterministic (same input, same drops) and undoable with undo like any other mutating tool.",
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
