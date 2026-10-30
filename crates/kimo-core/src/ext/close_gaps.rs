//! close_gaps (v7.5): general-purpose ripple-close for ANY gap on a track, across as many tracks as
//! named -- or, by default, every unlocked track in the project -- in one call. `ripple_delete` closes
//! the one gap left behind by deleting a single clip; `drop_short_segments` closes gaps left by removing
//! clips under a duration threshold. Neither helps a timeline that already has scattered gaps from a
//! manual delete, a trim, a move, or an import done clip-by-clip: today that is "select the clip after
//! the gap, note the gap size, move it, repeat" -- once per gap, once per track. This tool walks each
//! named track's clips in timeline order and snaps every clip to touch the one before it, leaving the
//! track's very first clip where it is (a real opening shot, not a stray gap) unless include_leading_gap
//! asks to snap it to 0 too. min_gap lets a deliberate small pause survive a sweep. Explicit track_ids
//! that name a locked track is an error (nothing else on the call is silently skipped because of it);
//! the default (no track_ids: "the whole timeline") instead skips a locked track and reports it, since a
//! whole-project sweep should not fail outright just because one track is protected.
use super::kit::*;

const EPS: f64 = 1e-6;

fn bool_arg_opt(a: &Value, k: &str) -> Result<Option<bool>, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => v.as_bool().map(Some).ok_or_else(|| crate::errors::arg_err(a, k, "boolean")) }
}
fn opt_num_checked(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => v.as_f64().ok_or_else(|| crate::errors::arg_err(a, k, "number")) }
}
fn r3(v: f64) -> f64 { (v * 1000.0).round() / 1000.0 }

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let dry_run = bool_arg_opt(a, "dry_run")?.unwrap_or(false);
    let include_leading_gap = bool_arg_opt(a, "include_leading_gap")?.unwrap_or(false);
    let min_gap = opt_num_checked(a, "min_gap", 0.0)?;
    if !min_gap.is_finite() || min_gap < 0.0 { return Err("min_gap must be a finite number >= 0".into()); }
    let explicit = matches!(a.get("track_ids"), Some(v) if !v.is_null());

    let track_ids: Vec<String> = if explicit {
        a.get("track_ids").and_then(|v| v.as_array()).ok_or_else(|| crate::errors::arg_err(a, "track_ids", "array of track id strings"))?
            .iter().map(|v| v.as_str().map(String::from).ok_or_else(|| crate::errors::arg_err(a, "track_ids", "array of track id strings")))
            .collect::<Result<_, _>>()?
    } else {
        ss.project.tracks.iter().map(|t| t.id.clone()).collect()
    };
    if track_ids.is_empty() { return Err(if explicit { "track_ids is empty: name at least one track".into() } else { "no tracks to close gaps on: the project has no unlocked tracks".into() }); }

    // Work on a clone so a dry_run (or an error partway through) never leaves a partial edit on the live
    // project -- the same "build the answer, then commit once" shape close_gaps' own callers expect.
    let mut work = ss.project.clone();
    let mut tracks_out: Vec<Value> = vec![];
    let mut tracks_skipped_locked: Vec<String> = vec![];
    let mut total_gaps = 0usize;
    let mut total_shift = 0.0f64;

    for tid in &track_ids {
        let ti = work.tracks.iter().position(|t| &t.id == tid).ok_or_else(|| format!("no such track '{tid}'"))?;
        if work.tracks[ti].locked {
            if explicit { return Err(format!("track '{tid}' is locked: unlock it with set_track locked=false, or omit track_ids to sweep only unlocked tracks")); }
            tracks_skipped_locked.push(tid.clone());
            continue;
        }
        let mut order: Vec<usize> = (0..work.tracks[ti].clips.len()).collect();
        order.sort_by(|&x, &y| work.tracks[ti].clips[x].start.partial_cmp(&work.tracks[ti].clips[y].start).unwrap_or(std::cmp::Ordering::Equal));
        let mut cursor = if include_leading_gap { 0.0 } else { order.first().map(|&i| work.tracks[ti].clips[i].start).unwrap_or(0.0) };
        let mut gaps_closed = 0usize;
        let mut shifted = 0.0f64;
        let mut moved_clip_ids: Vec<String> = vec![];
        for &idx in &order {
            let (start, dur, id) = { let c = &work.tracks[ti].clips[idx]; (c.start, c.duration, c.id.clone()) };
            let gap = start - cursor;
            if gap > min_gap + EPS {
                std::sync::Arc::make_mut(&mut work.tracks[ti].clips[idx]).start = cursor;
                gaps_closed += 1;
                shifted += gap;
                moved_clip_ids.push(id);
                cursor += dur;
            } else {
                cursor = start + dur;
            }
        }
        total_gaps += gaps_closed;
        total_shift += shifted;
        tracks_out.push(json!({"track_id": tid, "gaps_closed": gaps_closed, "shifted_seconds": r3(shifted), "moved_clip_ids": moved_clip_ids}));
    }

    let mut result = json!({
        "dry_run": dry_run,
        "tracks": tracks_out,
        "total_gaps_closed": total_gaps,
        "total_shifted_seconds": r3(total_shift),
        "tracks_skipped_locked": tracks_skipped_locked,
    });
    if dry_run {
        result["no_history"] = json!(true);
        return Ok(result);
    }
    ss.project = work;
    Ok(result)
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "close_gaps",
        description: "Ripple-close every gap on one or more tracks in one call: each clip snaps to touch the clip before it, so there is no empty space left between them. Unlike ripple_delete (closes the single gap a just-deleted clip left) or drop_short_segments (closes gaps left by removing short clips), this closes ANY existing gap regardless of how it got there -- after a manual delete, a trim, a move, or a clip-by-clip import left several scattered across one or many tracks. track_ids: the tracks to sweep; omit it to sweep every unlocked track in the project (\"the whole timeline\") in this one call. Each track's own first clip (by timeline order) is left where it is -- a real opening shot, not a stray gap -- unless include_leading_gap (default false) asks to snap it to 0 too. min_gap (default 0, seconds) lets a deliberate small pause survive the sweep. Naming a locked track in track_ids is an error; omitting track_ids instead skips a locked track and lists it in tracks_skipped_locked, so a whole-project sweep does not fail outright over one protected track. dry_run reports exactly what would move (gaps_closed, shifted_seconds, moved_clip_ids per track) and changes nothing. Deterministic and undoable with undo like any other mutating tool.",
        mutates: true,
        schema: obj(vec![
            ("track_ids", json!({"type":"array","items":{"type":"string"},"description":"tracks to close gaps on; omit for every unlocked track in the project"})),
            ("min_gap", n("gaps at or below this many seconds are left alone; default 0 (close any gap at all)")),
            ("include_leading_gap", json!({"type":"boolean","description":"also snap each track's first clip to 0 if it does not already start there. Default false."})),
            ("dry_run", json!({"type":"boolean","description":"report what would move, change nothing. Default false."})),
        ], &[]),
        run,
    }]
}
