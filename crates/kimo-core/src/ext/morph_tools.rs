//! add_morph_cut (Premiere Pro Morph Cut): a jump cut in a talking-head shot (two abutting pieces of the same take, e.g. after
//! remove_silence_cuts) gets a transition_in of kind morph_cut. Over its duration the renderer morphs the outgoing clip's last
//! frame into the incoming clip with dense optical flow (flow.rs, both directions, mixed in linear light), so the head glides
//! instead of jumping; no overlap or handles are needed.
use super::kit::*;
use crate::model::*;

fn jump_cuts(p: &Project, track: &Track) -> Vec<String> {
    track.clips.iter().filter(|c| matches!(c.source, Source::Media { .. })).filter(|c| match crate::transitions::morph_prev(p, c) {
        Some(a) => matches!((&a.source, &c.source), (Source::Media { asset: x }, Source::Media { asset: y }) if x == y), None => false }).map(|c| c.id.clone()).collect()
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_morph_cut", description: "Morph Cut: smooth a talking-head jump cut with an optical-flow morph from the outgoing frame. clip_id, or track_id for every jump cut.", mutates: true,
        schema: obj(vec![("clip_id", s("the clip AFTER the cut (it must start where the previous clip on its track ends)")),
            ("track_id", s("instead of clip_id: every jump cut on this track (abutting clips from the same media)")),
            ("duration", n("seconds of morph after the cut, 0.1..2, default 0.4")),
            ("remove", json!({"type":"boolean","description":"true removes the morph cut from clip_id (or from every clip on track_id)"}))], &[]),
        run: |ss, a| {
            for k in a.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default() { if !["clip_id", "track_id", "duration", "remove"].contains(&k.as_str()) { return Err(format!("add_morph_cut: unknown argument '{k}' (valid: clip_id, track_id, duration, remove)")); } }
            let cid = a.get("clip_id").and_then(|v| v.as_str()).map(String::from); let tid = a.get("track_id").and_then(|v| v.as_str()).map(String::from);
            let ids: Vec<String> = match (&cid, &tid) {
                (Some(_), Some(_)) => return Err("give clip_id (one cut) or track_id (every jump cut on the track), not both".into()),
                (None, None) => return Err("add_morph_cut needs clip_id (the clip after the cut) or track_id".into()),
                (Some(id), None) => vec![id.clone()],
                (None, Some(t)) => { let tr = ss.project.tracks.iter().find(|x| &x.id == t).ok_or_else(|| format!("no such track {t}"))?;
                    if a.get("remove").and_then(|v| v.as_bool()).unwrap_or(false) { tr.clips.iter().filter(|c| c.transition_in.as_ref().is_some_and(|x| x.kind == "morph_cut")).map(|c| c.id.clone()).collect() } else { jump_cuts(&ss.project, tr) } } };
            if a.get("remove").and_then(|v| v.as_bool()).unwrap_or(false) {
                let mut n = 0; for id in &ids { let c = ss.project.find_clip_mut(id).ok_or_else(|| format!("no such clip {id}"))?; if c.transition_in.as_ref().is_some_and(|x| x.kind == "morph_cut") { c.transition_in = None; n += 1; } }
                return Ok(json!({"removed": n})); }
            let d = match a.get("duration") { None => 0.4, Some(v) if v.is_null() => 0.4, Some(v) => v.as_f64().ok_or("duration must be a number of seconds (0.1..2)")? };
            if !(0.1..=2.0).contains(&d) { return Err(format!("duration must be between 0.1 and 2 seconds, got {d}")); }
            if ids.is_empty() { return Err(format!("track {} has no jump cuts: a jump cut is a media clip that starts where a clip of the same media ends", tid.unwrap_or_default())); }
            for id in &ids {
                let c = ss.project.find_clip(id).ok_or_else(|| format!("no such clip {id}"))?;
                if !matches!(c.source, Source::Media { .. }) { return Err(format!("add_morph_cut needs a media clip: '{id}' is not one")); }
                if crate::transitions::morph_prev(&ss.project, c).is_none() { return Err(format!("clip {id} has no clip ending where it starts on its track: a morph cut joins two abutting shots (a jump cut). Close the gap first (ripple_delete or move_clip)")); }
                if d > c.duration + 1e-9 { return Err(format!("duration {d} s is longer than clip {id} ({:.3} s)", c.duration)); }
            }
            let tr: Transition = serde_json::from_value(json!({"kind": "morph_cut", "duration": d, "ease": "in_out"})).map_err(|e| e.to_string())?;
            for id in &ids { ss.project.find_clip_mut(id).unwrap().transition_in = Some(tr.clone()); }
            Ok(json!({"clip_ids": ids, "duration": d})) } },
] }
