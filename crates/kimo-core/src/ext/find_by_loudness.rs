//! v7.2 additions: find_by_loudness -- the north star's own example of "whole-project reasoning in one
//! call" ("all clips under -20 LUFS"): measure_loudness already takes a clip_id (per-clip measurement
//! existed before this pass), but answering that question today means calling it once per clip and
//! correlating results by hand. This queries every media clip in one call. Reuses measure_loudness
//! itself through dispatch_nested (no second loudness implementation) -- same EBU R128 method, same
//! 0.4s minimum, so results are identical to calling measure_loudness directly, clip by clip.
use super::kit::*;
use crate::commands::{dispatch_nested, Session};
use crate::model::Source;

fn find_by_loudness(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let min_lufs = match a.get("min_lufs") { None | Some(Value::Null) => None, Some(v) => Some(v.as_f64().filter(|x| x.is_finite()).ok_or("min_lufs must be a finite number")?) };
    let max_lufs = match a.get("max_lufs") { None | Some(Value::Null) => None, Some(v) => Some(v.as_f64().filter(|x| x.is_finite()).ok_or("max_lufs must be a finite number")?) };
    if let (Some(lo), Some(hi)) = (min_lufs, max_lufs) { if lo > hi { return Err(format!("min_lufs ({lo}) must be <= max_lufs ({hi})")); } }
    let track_filter = a.get("track_id").and_then(|v| v.as_str()).map(String::from);
    let true_peak = a.get("true_peak").and_then(|v| v.as_bool()).unwrap_or(false);

    // Collect (clip_id, track_id) for every media clip first -- the project borrow must end before
    // dispatch_nested needs &mut Session to decode each one.
    let mut candidates: Vec<(String, String)> = vec![];
    for t in &ss.project.tracks {
        if let Some(tf) = &track_filter { if t.id != *tf { continue; } }
        for c in &t.clips { if matches!(c.source, Source::Media { .. }) { candidates.push((c.id.clone(), t.id.clone())); } }
    }

    let mut matches = vec![]; let mut skipped = vec![]; let mut all = vec![];
    for (id, track) in &candidates {
        match dispatch_nested(ss, "measure_loudness", &json!({"clip_id": id, "true_peak": true_peak})) {
            Err(e) => skipped.push(json!({"id": id, "track": track, "reason": format!("measure_loudness failed: {e}")})),
            Ok(r) => {
                let lufs = r["integrated_lufs"].as_f64();
                match lufs {
                    None => skipped.push(json!({"id": id, "track": track, "reason": "too short or silent for an integrated loudness reading (minimum 0.4s)"})),
                    Some(l) => {
                        let entry = json!({"id": id, "track": track, "integrated_lufs": l, "sample_peak_dbfs": r["sample_peak_dbfs"], "true_peak_dbfs": r["true_peak_dbfs"]});
                        all.push(entry.clone());
                        let in_range = min_lufs.is_none_or(|lo| l >= lo) && max_lufs.is_none_or(|hi| l <= hi);
                        if in_range { matches.push(entry); }
                    }
                }
            }
        }
    }
    let filtering = min_lufs.is_some() || max_lufs.is_some();
    Ok(json!({
        "measured": all.len(), "skipped": skipped.len(), "total_media_clips": candidates.len(),
        "matches": if filtering { matches.clone() } else { all.clone() },
        "match_count": if filtering { matches.len() } else { all.len() },
        "filtered": filtering,
        "skipped_clips": skipped,
        "method": "EBU R128 stereo 48k, same as measure_loudness (reused per clip, not re-implemented)",
    }))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "find_by_loudness", description: "Measure every media clip's integrated loudness in one call, optionally filtered to a range (e.g. \"every clip under -20 LUFS\"). Reuses measure_loudness per clip (same EBU R128 method, same 0.4s minimum) rather than a second implementation, so results match calling it one clip at a time -- this just does the whole project in one round trip and does the range filtering for you. min_lufs/max_lufs bound the range (both optional; omit both to get every media clip's reading, a per-clip loudness readout of the whole project); track_id scopes to one track; true_peak also computes each clip's true-peak estimate. A clip too short or silent for an integrated reading (under 0.4s), or one measure_loudness itself fails on, is listed in skipped_clips with why, not treated as a match or a failure of the whole call. Read-only.", mutates: false,
        schema: obj(vec![("min_lufs", n("only clips at or above this (e.g. -70)")), ("max_lufs", n("only clips at or below this (e.g. -20, the north star's own example)")), ("track_id", s("optional: only this track")), ("true_peak", json!({"type":"boolean","description":"also compute true-peak dBFS per clip (slower)"}))], &[]), run: find_by_loudness },
] }
