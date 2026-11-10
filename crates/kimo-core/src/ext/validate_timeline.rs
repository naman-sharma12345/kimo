//! v7.2 additions: validate_timeline -- a structural health check of the timeline, independent of
//! `lint_edit` (which is a platform/export-readiness lint: dark frames, text under the UI, b-roll over
//! silence, loudness/captions/fonts for tiktok/reels/shorts; confirmed by reading its full source, it
//! never looks at raw clip start/duration structure) and independent of `list_dependencies` (which
//! already finds missing/changed media files -- this tool does not re-check file presence).
//! Read-only. Checks, per track: gaps (empty time between consecutive clips), overlaps (two clips
//! sharing time on the same track), and per clip anywhere: zero/negative duration, negative start.
use super::kit::*;
use crate::commands::Session;
use crate::ext::names::resolve_ref;

const EPS: f64 = 1e-6;

fn clip_brief(id: &str, name: &Option<String>, track: &str, start: f64, end: f64) -> Value {
    json!({"id": id, "name": name, "track": track, "start": start, "end": end})
}

fn validate_timeline(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let min_gap = match a.get("min_gap_seconds") {
        None => 0.0,
        Some(v) => v.as_f64().filter(|x| x.is_finite() && *x >= 0.0).ok_or("min_gap_seconds must be a finite number >= 0")?,
    };
    let track_filter = match a.get("track_id").and_then(|v| v.as_str()) {
        Some(t) => Some(resolve_ref(&ss.project, "track", t)?),
        None => None,
    };

    let mut issues: Vec<Value> = vec![];
    let (mut n_gaps, mut n_overlaps, mut n_zero_or_neg_duration, mut n_negative_start) = (0usize, 0usize, 0usize, 0usize);

    for t in &ss.project.tracks {
        if let Some(tf) = &track_filter { if t.id != *tf { continue; } }
        // Clip-level checks, in id order (stable, independent of start-sort below).
        for c in &t.clips {
            if c.duration <= EPS {
                n_zero_or_neg_duration += 1;
                issues.push(json!({"kind": "zero_or_negative_duration", "clip": clip_brief(&c.id, &c.name, &t.id, c.start, c.start + c.duration), "duration": c.duration}));
            }
            if c.start < -EPS {
                n_negative_start += 1;
                issues.push(json!({"kind": "negative_start", "clip": clip_brief(&c.id, &c.name, &t.id, c.start, c.start + c.duration)}));
            }
        }
        // Gap/overlap checks need clips in timeline order.
        let mut ordered: Vec<&std::sync::Arc<crate::model::Clip>> = t.clips.iter().collect();
        ordered.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
        for w in ordered.windows(2) {
            let (prev, cur) = (&w[0], &w[1]);
            let prev_end = prev.start + prev.duration;
            let gap = cur.start - prev_end;
            if gap > EPS {
                if gap >= min_gap {
                    n_gaps += 1;
                    issues.push(json!({"kind": "gap", "track": t.id, "after": clip_brief(&prev.id, &prev.name, &t.id, prev.start, prev_end), "before": clip_brief(&cur.id, &cur.name, &t.id, cur.start, cur.start + cur.duration), "gap_seconds": gap}));
                }
            } else if gap < -EPS {
                n_overlaps += 1;
                issues.push(json!({"kind": "overlap", "track": t.id, "first": clip_brief(&prev.id, &prev.name, &t.id, prev.start, prev_end), "second": clip_brief(&cur.id, &cur.name, &t.id, cur.start, cur.start + cur.duration), "overlap_seconds": -gap}));
            }
        }
    }

    let total = issues.len();
    Ok(json!({
        "ok": total == 0,
        "summary": {"gaps": n_gaps, "overlaps": n_overlaps, "zero_or_negative_duration": n_zero_or_neg_duration, "negative_start": n_negative_start, "total": total},
        "issues": issues,
        "note": "structural only: not checked here -- missing/changed media files (use list_dependencies), platform/export readiness (use lint_edit)",
    }))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "validate_timeline", description: "Structural health check of the timeline: gaps and overlaps between clips on the same track, zero/negative-duration clips, negative clip starts. Structural health check, read-only, independent of lint_edit (platform/export readiness: dark frames, loudness, captions, fonts) and list_dependencies (missing/changed media files) -- neither of those looks at raw clip placement. A gap is empty time between two clips on the same track; min_gap_seconds (default 0) ignores gaps smaller than this (e.g. sub-frame rounding gaps from other tools). An overlap is two clips on the same track sharing time. track_id scopes the check to one track; omitted checks every track. Returns ok (no issues), a summary count per kind, and the full issues list, each with the clip(s) involved (id, name, track, start/end).", mutates: false,
        schema: obj(vec![("track_id", s("optional: only check this track, id or @name")), ("min_gap_seconds", n("ignore gaps smaller than this, default 0"))], &[]), run: validate_timeline },
] }
