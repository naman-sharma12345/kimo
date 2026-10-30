//! v7.3: `batch_normalize_loudness` -- normalize_loudness (pre-v7.3) takes one clip_id at a time; hitting
//! a target LUFS across many/all clips means calling it once per clip and correlating results by hand,
//! the same gap find_by_loudness (v7.2) closed on the read side. This closes it on the write side: measures
//! and normalizes every candidate clip in one call, reusing normalize_loudness itself through
//! dispatch_nested (no second gain-staging implementation) so results are identical to calling it one clip
//! at a time. A clip already within `tolerance_db` of the target is left alone (no redundant gain change);
//! a clip normalize_loudness itself cannot handle (silent, too short, or would need to clip above -1 dBFS)
//! is reported in `skipped`/`failed` with why, never guessed at, and does not stop the rest.
use super::kit::*;
use crate::commands::{dispatch_nested, Session};
use crate::model::Source;

fn batch_normalize_loudness(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let target = a.get("target_lufs").and_then(|v| v.as_f64()).filter(|x| x.is_finite()).unwrap_or(-14.0);
    let tolerance = a.get("tolerance_db").and_then(|v| v.as_f64()).filter(|x| x.is_finite() && *x >= 0.0).unwrap_or(0.5);
    let track_filter = a.get("track_id").and_then(|v| v.as_str()).map(String::from);
    let explicit_ids: Option<Vec<String>> = a.get("clip_ids").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|c| c.as_str().map(String::from)).collect());
    let dry_run = a.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);

    // Collect (clip_id, track_id) candidates first -- the project borrow must end before dispatch_nested needs &mut Session.
    let mut candidates: Vec<(String, String)> = vec![];
    if let Some(ids) = &explicit_ids {
        for id in ids {
            match ss.project.tracks.iter().find(|t| t.clips.iter().any(|c| &c.id == id)) {
                Some(t) => candidates.push((id.clone(), t.id.clone())),
                None => return Err(format!("unknown clip_id: no clip '{id}' on the timeline")),
            }
        }
    } else {
        for t in &ss.project.tracks {
            if let Some(tf) = &track_filter { if t.id != *tf { continue; } }
            for c in &t.clips { if matches!(c.source, Source::Media { .. }) { candidates.push((c.id.clone(), t.id.clone())); } }
        }
    }

    let mut applied = vec![]; let mut already_on_target = vec![]; let mut skipped = vec![]; let mut failed = vec![];
    for (id, track) in &candidates {
        let measured = match dispatch_nested(ss, "measure_loudness", &json!({"clip_id": id})) {
            Ok(r) => r, Err(e) => { skipped.push(json!({"id": id, "track": track, "reason": format!("measure_loudness failed: {e}")})); continue; }
        };
        let Some(lufs) = measured["integrated_lufs"].as_f64() else {
            skipped.push(json!({"id": id, "track": track, "reason": "too short or silent for an integrated loudness reading (minimum 0.4s)"})); continue;
        };
        if (lufs - target).abs() <= tolerance {
            already_on_target.push(json!({"id": id, "track": track, "measured_lufs": lufs}));
            continue;
        }
        if dry_run {
            applied.push(json!({"id": id, "track": track, "measured_lufs": lufs, "would_gain_db": target - lufs})); continue;
        }
        match dispatch_nested(ss, "normalize_loudness", &json!({"clip_id": id, "target_lufs": target})) {
            Ok(r) => applied.push(json!({"id": id, "track": track, "measured_lufs": r["measured_lufs"], "gain_db": r["gain_db"]})),
            Err(e) => failed.push(json!({"id": id, "track": track, "measured_lufs": lufs, "reason": e})),
        }
    }

    Ok(json!({
        "ok": failed.is_empty(),
        "dry_run": dry_run,
        "target_lufs": target, "tolerance_db": tolerance,
        "total_candidates": candidates.len(),
        "applied": applied, "already_on_target": already_on_target, "skipped": skipped, "failed": failed,
        "method": "reuses measure_loudness + normalize_loudness per clip (not re-implemented); EBU R128 stereo 48k",
    }))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "batch_normalize_loudness", description: "Normalize every media clip (or a given clip_ids list, or one track) to a target LUFS in one call, instead of calling normalize_loudness once per clip and correlating results by hand -- the write-side counterpart to find_by_loudness's read-side batch query. target_lufs defaults to -14 (a common delivery target); tolerance_db (default 0.5) leaves a clip already within that of the target untouched (listed in already_on_target, no redundant gain change). clip_ids (optional) scopes to an explicit list (an unknown id errors); otherwise track_id (optional) scopes to one track, or every media clip on the timeline by default. Reuses measure_loudness and normalize_loudness per clip through dispatch_nested (no second gain-staging implementation), so results match calling normalize_loudness directly, clip by clip. A clip too short/silent for a reading, or one normalize_loudness itself refuses (would need over 40dB gain, or would clip above -1 dBFS), is reported in skipped/failed with why and does not stop the rest. dry_run reports measured_lufs and would_gain_db for every clip that would change, without mutating anything.", mutates: true,
        schema: obj(vec![("target_lufs", n("target integrated LUFS, default -14")), ("tolerance_db", n("skip a clip already within this many dB of target, default 0.5")), ("clip_ids", json!({"type":"array","items":{"type":"string"},"description":"explicit clips to normalize; default: every media clip (optionally scoped by track_id)"})), ("track_id", s("optional: only this track (ignored if clip_ids given)")), ("dry_run", json!({"type":"boolean","description":"report measured_lufs/would_gain_db per clip, mutate nothing"}))], &[]), run: batch_normalize_loudness },
] }
