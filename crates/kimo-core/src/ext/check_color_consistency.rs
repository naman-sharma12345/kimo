//! v7.2 additions: check_color_consistency -- the read-only counterpart to match_shots/match_color
//! (which already FIX a timeline's grade onto a hero clip, uniformly, mutating every clip). Nothing
//! today tells an agent WHICH clips are actually inconsistent before it reaches for either fix, or lets
//! it verify a fix actually closed the gap afterward. No new pixel analysis: samples each clip's own
//! midpoint through the existing get_scopes (timeline composite at a time, via dispatch_nested) and
//! compares luma_mean/avg_rgb against either a named reference clip or the checked group's own median.
//! Read-only (get_scopes is read-only; this adds no mutation).
use super::kit::*;
use crate::commands::{dispatch_nested, Session};
use crate::ext::names::resolve_ref;
use crate::model::Source;

fn median(mut v: Vec<f64>) -> f64 { v.sort_by(|a, b| a.partial_cmp(b).unwrap()); let n = v.len(); if n == 0 { 0.0 } else if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 } }

fn check_color_consistency(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let luma_tol = opt_num(a, "luma_tolerance", 0.08).clamp(0.0, 1.0);
    let rgb_tol = opt_num(a, "rgb_tolerance", 0.06).clamp(0.0, 1.0);

    // Which clips: an explicit list (any clip, any source type -- the caller's own call), or every
    // Source::Media clip on the timeline in track-then-start order (a title card or a solid isn't a
    // "shot" with continuity to check).
    let targets: Vec<(String, String, f64, f64)> = match a.get("clip_ids").and_then(|v| v.as_array()) {
        Some(arr) => {
            let mut out = vec![];
            for v in arr {
                let raw = v.as_str().ok_or("clip_ids must be strings")?;
                let id = resolve_ref(&ss.project, "clip", raw)?;
                let (track, c) = ss.project.tracks.iter().find_map(|t| t.clips.iter().find(|c| c.id == id).map(|c| (t.id.clone(), c.clone()))).ok_or_else(|| format!("no clip '{id}'"))?;
                out.push((id, track, c.start, c.start + c.duration));
            }
            out
        }
        None => {
            let mut out = vec![];
            for t in &ss.project.tracks {
                let mut ordered: Vec<&std::sync::Arc<crate::model::Clip>> = t.clips.iter().filter(|c| matches!(c.source, Source::Media { .. })).collect();
                ordered.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
                for c in ordered { out.push((c.id.clone(), t.id.clone(), c.start, c.start + c.duration)); }
            }
            out
        }
    };

    if targets.is_empty() {
        return Ok(json!({"ok": true, "checked": 0, "flagged": 0, "clips": [], "note": "no clips to check (empty clip_ids, or no media clips on the timeline)"}));
    }

    let ref_id = a.get("ref_clip_id").and_then(|v| v.as_str()).map(|r| resolve_ref(&ss.project, "clip", r)).transpose()?;
    if let Some(r) = &ref_id { if !targets.iter().any(|(id, ..)| id == r) { return Err(format!("ref_clip_id '{r}' is not among the clips being checked")); } }

    let mut samples: Vec<(String, String, f64, f64, f64, [f64; 3])> = vec![]; // id, track, start, end, luma, rgb
    for (id, track, start, end) in &targets {
        let mid = (start + end) / 2.0;
        let sc = dispatch_nested(ss, "get_scopes", &json!({"time": mid}))?;
        let luma = sc["luma_mean"].as_f64().unwrap_or(0.0);
        let rgb = sc["avg_rgb"].as_array().map(|a| [a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0), a[2].as_f64().unwrap_or(0.0)]).unwrap_or([0.0, 0.0, 0.0]);
        samples.push((id.clone(), track.clone(), *start, *end, luma, rgb));
    }

    let (ref_luma, ref_rgb, ref_desc) = match &ref_id {
        Some(r) => { let s = samples.iter().find(|s| &s.0 == r).unwrap(); (s.4, s.5, format!("clip {r}")) }
        None => {
            let l = median(samples.iter().map(|s| s.4).collect());
            let rgb = [median(samples.iter().map(|s| s.5[0]).collect()), median(samples.iter().map(|s| s.5[1]).collect()), median(samples.iter().map(|s| s.5[2]).collect())];
            (l, rgb, "the group's own median".to_string())
        }
    };

    let mut clips = vec![]; let mut flagged = 0usize;
    for (id, track, start, end, luma, rgb) in &samples {
        let luma_diff = (luma - ref_luma).abs();
        let rgb_diff = (0..3).map(|i| (rgb[i] - ref_rgb[i]).abs()).fold(0.0_f64, f64::max);
        let is_flagged = luma_diff > luma_tol || rgb_diff > rgb_tol;
        if is_flagged { flagged += 1; }
        clips.push(json!({"clip_id": id, "track": track, "start": r3(*start), "end": r3(*end), "luma_mean": r3(*luma), "avg_rgb": rgb.map(r3), "luma_diff": r3(luma_diff), "rgb_diff": r3(rgb_diff), "flagged": is_flagged}));
    }

    Ok(json!({
        "ok": flagged == 0, "checked": samples.len(), "flagged": flagged,
        "reference": ref_desc, "luma_tolerance": luma_tol, "rgb_tolerance": rgb_tol,
        "clips": clips,
        "note": "read-only diagnosis; match_color (one clip) and match_shots (every clip) are the fix -- this says which clips actually need it, sampling each clip's own midpoint frame"
    }))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "check_color_consistency", description: "Which clips don't match, before reaching for match_color/match_shots. Samples each clip's own midpoint frame (get_scopes) and flags any whose luma or average color strays past tolerance from a reference clip (ref_clip_id) or, by default, the checked group's own median -- the read-only diagnosis match_shots' uniform fix has no counterpart for today.", mutates: false,
        schema: obj(vec![("clip_ids", json!({"type":"array","items":{"type":"string"},"description":"clips to check; default: every media clip on the timeline"})), ("ref_clip_id", s("compare every clip to this one (the hero shot); default compares to the checked group's own median")), ("luma_tolerance", n("0..1, default 0.08")), ("rgb_tolerance", n("0..1 per channel, default 0.06"))], &[]), run: check_color_consistency },
] }

fn r3(x: f64) -> f64 { (x * 1000.0).round() / 1000.0 }
