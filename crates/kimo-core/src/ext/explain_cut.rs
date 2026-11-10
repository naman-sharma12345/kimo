//! v7.2 additions: explain_cut -- best-effort "why is there a cut here" for one clip's incoming edit
//! point. Not a new detector: reads the clip's own data (transition_in, source) and correlates it with
//! what's already on the timeline (nearby markers, add_marker/mark_scene_cuts) and what detect_cuts can
//! tell you about the footage right around that moment (the composite's own strongest visual change in
//! a small window, via dispatch_nested -- no new scoring logic). An AI driving kimo alone, with no human
//! eye on the monitor, can't glance at the cut and judge it; this gives it the evidence a human would
//! have used to judge the same cut, and says plainly when the evidence runs out (clip_id with no
//! predecessor on its track, no media to analyse, a detect_cuts call that can't resolve anything useful).
//! Read-only. Explicitly best-effort / advisory (same posture detect_cuts itself takes): a transition or
//! a marker is a fact; "detected_as_strong_visual_cut" is one signal among several, not a verdict.
use super::kit::*;
use crate::commands::{dispatch_nested, Session};
use crate::ext::names::resolve_ref;
use crate::model::Source;

fn clip_source_asset(c: &crate::model::Clip) -> Option<String> {
    match &c.source { Source::Media { asset } => Some(asset.clone()), _ => None }
}

fn explain_cut(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let clip_id = resolve_ref(&ss.project, "clip", str_arg(a, "clip_id")?)?;
    let window = opt_num(a, "window_seconds", 0.5).clamp(0.05, 5.0);

    let (track_id, idx, ordered_ids): (String, usize, Vec<String>) = {
        let mut found: Option<(String, usize, Vec<String>)> = None;
        for t in &ss.project.tracks {
            let mut ordered: Vec<&std::sync::Arc<crate::model::Clip>> = t.clips.iter().collect();
            ordered.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
            if let Some(i) = ordered.iter().position(|c| c.id == clip_id) {
                found = Some((t.id.clone(), i, ordered.iter().map(|c| c.id.clone()).collect()));
                break;
            }
        }
        found.ok_or_else(|| format!("no clip '{clip_id}' on any track"))?
    };

    let (cur_start, cur_name, cur_asset, has_transition, transition_kind) = {
        let t = ss.project.tracks.iter().find(|t| t.id == track_id).unwrap();
        let c = t.clips.iter().find(|c| c.id == clip_id).unwrap();
        (c.start, c.name.clone(), clip_source_asset(c), c.transition_in.is_some(), c.transition_in.as_ref().map(|tr| tr.kind.clone()))
    };

    let prev = if idx == 0 { None } else {
        let pid = &ordered_ids[idx - 1];
        let t = ss.project.tracks.iter().find(|t| t.id == track_id).unwrap();
        t.clips.iter().find(|c| c.id == *pid).map(|c| (c.id.clone(), c.start + c.duration, clip_source_asset(c)))
    };

    if prev.is_none() {
        return Ok(json!({"clip_id": clip_id, "at": cur_start, "track": track_id, "is_first_on_track": true,
            "explanation": "this is the first clip on its track -- there is no incoming cut to explain",
            "note": "best-effort: reads transition_in, nearby markers and detect_cuts around the boundary; not a new detector"}));
    }
    let (prev_id, prev_end, prev_asset) = prev.unwrap();
    let at = cur_start; // the actual edit point: wherever this clip's predecessor ends and it begins (equal when there's no gap)
    let same_asset_as_previous = match (&prev_asset, &cur_asset) { (Some(p), Some(c)) => Some(p == c), _ => None };

    // Nearby markers: anything within `window` of the cut point, not just an exact-time match -- a
    // mark_scene_cuts marker sits at detect_cuts' own detected time, which need not equal the clip
    // boundary to the sample.
    let nearby_markers: Vec<Value> = ss.project.markers.iter()
        .filter(|m| (m.time - at).abs() <= window)
        .map(|m| json!({"id": m.id, "time": m.time, "name": m.name, "color": m.color, "offset_from_cut_s": r3(m.time - at)}))
        .collect();

    // detect_cuts on the timeline composite in a small window around the edit point: does the footage
    // itself show a strong visual change right here, or does this look like a cut hidden between two
    // visually similar moments (a J/L-style edit, or a cut with no real content change to back it)?
    let probe_start = (at - window).max(0.0);
    let probe_end = at + window;
    let detect = dispatch_nested(ss, "detect_cuts", &json!({"source": "timeline", "start": probe_start, "end": probe_end, "kind_hints": true}));
    let (detected_as_visual_cut, visual_cut_detail) = match &detect {
        Ok(d) => {
            let cuts = d["cuts"].as_array().cloned().unwrap_or_default();
            let nearest = cuts.iter().min_by(|a, b| {
                let da = (a["time"].as_f64().unwrap_or(f64::MAX) - at).abs();
                let db = (b["time"].as_f64().unwrap_or(f64::MAX) - at).abs();
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            }).cloned();
            match nearest {
                Some(c) if (c["time"].as_f64().unwrap_or(f64::MAX) - at).abs() <= window => (Some(true), Some(c)),
                _ => (Some(false), None),
            }
        }
        Err(e) => { (None, Some(json!({"detect_cuts_error": e}))) } // e.g. no decodable media at this range -- reported, not hidden
    };

    let mut reasons: Vec<String> = vec![];
    if has_transition { reasons.push(format!("an explicit {} transition is set, not a hard cut", transition_kind.as_deref().unwrap_or("transition"))); }
    if let Some(false) = same_asset_as_previous { reasons.push("cuts to a different source asset than the clip before it".into()); }
    if let Some(true) = same_asset_as_previous { reasons.push("same source asset as the clip before it (a split within one shot, or a repeated/re-used take)".into()); }
    if !nearby_markers.is_empty() {
        let names: Vec<String> = nearby_markers.iter().filter_map(|m| m["name"].as_str()).map(String::from).collect();
        reasons.push(format!("marked on the timeline: {}", names.join(", ")));
    }
    match detected_as_visual_cut {
        Some(true) => reasons.push("detect_cuts finds a real visual change in the footage right at this point".into()),
        Some(false) => reasons.push("detect_cuts finds no strong visual change here -- the cut may be led by audio/story rather than picture, or is a soft match-cut".into()),
        None => {}
    }
    let explanation = if reasons.is_empty() {
        "no transition, no nearby marker, and detect_cuts found nothing conclusive in this window -- this cut's reason isn't recoverable from the timeline alone".to_string()
    } else { reasons.join("; ") };

    Ok(json!({
        "clip_id": clip_id, "clip_name": cur_name, "at": r3(at), "track": track_id,
        "previous_clip_id": prev_id, "gap_before_s": r3((at - prev_end).max(0.0)),
        "has_incoming_transition": has_transition, "transition_kind": transition_kind,
        "same_asset_as_previous": same_asset_as_previous,
        "nearby_markers": nearby_markers,
        "detected_as_visual_cut": detected_as_visual_cut, "visual_cut_detail": visual_cut_detail,
        "explanation": explanation,
        "note": "best-effort: reads transition_in, nearby markers (add_marker/mark_scene_cuts) and detect_cuts around the boundary; not a new detector, and an empty explanation means the evidence ran out, not that the cut was wrong"
    }))
}

pub fn commands() -> Vec<Command> {
    vec![
        Command { name: "explain_cut", description: "BETA, best-effort: why a clip's incoming cut might exist. Reads its transition, nearby timeline markers, whether it's the same source asset as the clip before it, and whether detect_cuts finds a real visual change right there.", mutates: false,
            schema: obj(vec![("clip_id", s("the clip whose incoming cut to explain")), ("window_seconds", n("how close counts as \"nearby\" for markers and detect_cuts, default 0.5, 0.05..5"))], &["clip_id"]), run: explain_cut },
    ]
}

fn r3(x: f64) -> f64 { (x * 1000.0).round() / 1000.0 }
