//! Premiere-style editing tools: ripple/roll/slip/slide, markers, track control, range ops.
use crate::commands::Command;
use crate::model::*;
use serde_json::{json, Value};

fn s(d: &str) -> Value { json!({"type":"string","description":d}) }
fn n(d: &str) -> Value { json!({"type":"number","description":d}) }
fn obj(props: Vec<(&str, Value)>, req: &[&str]) -> Value {
    let mut m = serde_json::Map::new(); for (k, v) in props { m.insert(k.into(), v); }
    json!({"type":"object","properties":m,"required":req})
}
fn sa<'a>(a: &'a Value, k: &str) -> Result<&'a str, String> { a.get(k).and_then(|v| v.as_str()).ok_or(format!("missing string '{k}'")) }
fn na(a: &Value, k: &str) -> Result<f64, String> { a.get(k).and_then(|v| v.as_f64()).ok_or(format!("missing number '{k}'")) }
fn on(a: &Value, k: &str, d: f64) -> f64 { a.get(k).and_then(|v| v.as_f64()).unwrap_or(d) }
const EPS: f64 = 1e-6;

/// (track index, clip index)
fn loc(p: &Project, id: &str) -> Result<(usize, usize), String> {
    for (ti, t) in p.tracks.iter().enumerate() { if let Some(ci) = t.clips.iter().position(|c| c.id == id) { return Ok((ti, ci)); } }
    Err(format!("no clip '{id}'"))
}
fn unlocked(p: &Project, ti: usize) -> Result<(), String> { if p.tracks[ti].locked { Err(format!("track {} is locked", p.tracks[ti].id)) } else { Ok(()) } }
/// Shift every clip on the track that starts at/after `t` by `d`.
fn shift_after(tr: &mut Track, t: f64, d: f64, skip: &str) { for c in tr.clips.iter_mut() { if c.id != skip && c.start >= t - EPS { c.start = (c.start + d).max(0.0); } } }
fn new_marker_id(p: &mut Project) -> String { p.gen_id("marker") }

/// Split `c` at timeline time `at`; returns right half.
fn cut(c: &mut Clip, at: f64, id: String) -> Clip {
    let mut r = c.clone(); r.id = id; r.start = at; r.duration = c.start + c.duration - at; r.in_point = c.in_point + (at - c.start) * c.speed; r.transition_in = None;
    c.duration = at - c.start; r
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "ripple_delete", description: "Delete a clip and close the gap: later clips on the same track shift left (Premiere ripple delete).", mutates: true,
        schema: obj(vec![("clip_id", s("clip"))], &["clip_id"]),
        run: |ss, a| { let id = sa(a, "clip_id")?; let (ti, ci) = loc(&ss.project, id)?; unlocked(&ss.project, ti)?;
            let c = ss.project.tracks[ti].clips.remove(ci); shift_after(&mut ss.project.tracks[ti], c.start + c.duration, -c.duration, ""); Ok(json!({"shifted_by":-c.duration})) } },
    Command { name: "ripple_trim", description: "Ripple trim: change the clip's head or tail by `delta` seconds (negative shortens). Later clips on the track shift to follow. edge: tail | head.", mutates: true,
        schema: obj(vec![("clip_id", s("clip")), ("edge", json!({"type":"string","enum":["head","tail"]})), ("delta", n("seconds, +grow / -shrink"))], &["clip_id","edge","delta"]),
        run: |ss, a| { let id = sa(a, "clip_id")?; let d = na(a, "delta")?; let (ti, ci) = loc(&ss.project, id)?; unlocked(&ss.project, ti)?;
            let c = &mut ss.project.tracks[ti].clips[ci]; let old_end = c.start + c.duration;
            if c.duration + d < 0.04 { return Err("clip would be shorter than 1 frame".into()); }
            match sa(a, "edge")? { "tail" => { c.duration += d; } "head" => { c.in_point = (c.in_point - d * c.speed).max(0.0); c.duration += d; } o => return Err(format!("bad edge {o}")) }
            shift_after(&mut ss.project.tracks[ti], old_end, d, id); Ok(json!({"duration": ss.project.tracks[ti].clips[ci].duration})) } },
    Command { name: "roll_edit", description: "Rolling edit: move the cut between two adjacent clips by `delta` seconds; total length is unchanged.", mutates: true,
        schema: obj(vec![("left_clip_id", s("clip before the cut")), ("right_clip_id", s("clip after the cut")), ("delta", n("seconds, + moves cut later"))], &["left_clip_id","right_clip_id","delta"]),
        run: |ss, a| { let d = na(a, "delta")?; let (t1, i1) = loc(&ss.project, sa(a, "left_clip_id")?)?; let (t2, i2) = loc(&ss.project, sa(a, "right_clip_id")?)?; if t1 != t2 { return Err("clips must be on the same track".into()); } unlocked(&ss.project, t1)?;
            let tr = &mut ss.project.tracks[t1]; if (tr.clips[i1].start + tr.clips[i1].duration - tr.clips[i2].start).abs() > 0.02 { return Err("clips are not adjacent".into()); }
            if tr.clips[i1].duration + d < 0.04 || tr.clips[i2].duration - d < 0.04 { return Err("roll too large".into()); }
            tr.clips[i1].duration += d; let r = &mut tr.clips[i2]; r.start += d; r.duration -= d; r.in_point = (r.in_point + d * r.speed).max(0.0); Ok(json!({"ok":true})) } },
    Command { name: "slip_clip", description: "Slip: shift which part of the source shows inside the clip (in-point += delta) without moving or resizing it.", mutates: true,
        schema: obj(vec![("clip_id", s("clip")), ("delta", n("source seconds"))], &["clip_id","delta"]),
        run: |ss, a| { let (ti, ci) = loc(&ss.project, sa(a, "clip_id")?)?; unlocked(&ss.project, ti)?; let c = &mut ss.project.tracks[ti].clips[ci]; c.in_point = (c.in_point + na(a, "delta")?).max(0.0); Ok(json!({"in_point":c.in_point})) } },
    Command { name: "slide_clip", description: "Slide: move a clip by `delta`; the previous clip's tail and next clip's head absorb the change so total length is unchanged.", mutates: true,
        schema: obj(vec![("clip_id", s("clip")), ("delta", n("seconds"))], &["clip_id","delta"]),
        run: |ss, a| { let d = na(a, "delta")?; let (ti, ci) = loc(&ss.project, sa(a, "clip_id")?)?; unlocked(&ss.project, ti)?;
            let tr = &mut ss.project.tracks[ti]; let (st, en) = (tr.clips[ci].start, tr.clips[ci].start + tr.clips[ci].duration);
            let prev = tr.clips.iter().position(|c| (c.start + c.duration - st).abs() < 0.02 && c.id != tr.clips[ci].id); let next = tr.clips.iter().position(|c| (c.start - en).abs() < 0.02 && c.id != tr.clips[ci].id);
            if let Some(p) = prev { if tr.clips[p].duration + d < 0.04 { return Err("previous clip too short".into()); } tr.clips[p].duration += d; }
            if let Some(x) = next { if tr.clips[x].duration - d < 0.04 { return Err("next clip too short".into()); } let r = &mut tr.clips[x]; r.start += d; r.duration -= d; r.in_point = (r.in_point + d * r.speed).max(0.0); }
            tr.clips[ci].start += d; Ok(json!({"ok":true})) } },
    Command { name: "insert_gap", description: "Insert empty time on a track at `at`: clips starting at/after it shift later by `duration` (a clip spanning `at` is split).", mutates: true,
        schema: obj(vec![("track_id", s("track")), ("at", n("seconds")), ("duration", n("seconds"))], &["track_id","at","duration"]),
        run: |ss, a| { let (at, d) = (na(a, "at")?, na(a, "duration")?); let tid = sa(a, "track_id")?.to_string(); let nid = ss.project.gen_id("clip");
            let ti = ss.project.tracks.iter().position(|t| t.id == tid).ok_or("no such track")?; unlocked(&ss.project, ti)?; let tr = &mut ss.project.tracks[ti];
            if let Some(ci) = tr.clips.iter().position(|c| c.start < at - EPS && c.start + c.duration > at + EPS) { let original=tr.clips[ci].id.clone();let r = cut(&mut tr.clips[ci], at, nid.clone()); tr.clips.push(r);ss.project.grade_scopes.copy_binding(&original,&nid); }
            shift_after(tr, at, d, ""); Ok(json!({"ok":true})) } },
    Command { name: "lift_range", description: "Lift: remove all material between `start` and `end` on a track, leaving a gap (clips are cut/trimmed as needed). extract=true closes the gap (ripple).", mutates: true,
        schema: obj(vec![("track_id", s("track")), ("start", n("s")), ("end", n("s")), ("extract", json!({"type":"boolean"}))], &["track_id","start","end"]),
        run: |ss, a| { let (st, en) = (na(a, "start")?, na(a, "end")?); if en <= st { return Err("end must be > start".into()); } let ex = a.get("extract").and_then(|v| v.as_bool()).unwrap_or(false);
            let tid = sa(a, "track_id")?.to_string(); let ids = [ss.project.gen_id("clip"), ss.project.gen_id("clip")];
            let ti = ss.project.tracks.iter().position(|t| t.id == tid).ok_or("no such track")?; unlocked(&ss.project, ti)?; let tr = &mut ss.project.tracks[ti]; let mut out: Vec<Clip> = vec![]; let mut k = 0;
            for mut c in tr.clips.drain(..) { let ce = c.start + c.duration;
                if ce <= st + EPS || c.start >= en - EPS { out.push(c); continue; }
                if c.start < st - EPS && ce > en + EPS { let mut r = cut(&mut c, en, ids[k].clone()); k += 1; c.duration = st - c.start; r.transition_in = None; out.push(c); out.push(r); }
                else if c.start < st - EPS { c.duration = st - c.start; out.push(c); }
                else if ce > en + EPS { let r = cut(&mut c, en, ids[k].clone()); k += 1; out.push(r); }
            }
            tr.clips = out; if ex { for c in tr.clips.iter_mut() { if c.start >= en - EPS { c.start -= en - st; } } } Ok(json!({"ok":true})) } },
    Command { name: "duplicate_clip", description: "Copy a clip (all effects, keyframes, masks) to a new start time, optionally onto another track. Returns the new clip id.", mutates: true,
        schema: obj(vec![("clip_id", s("clip")), ("start", n("new timeline start")), ("track_id", s("optional destination track"))], &["clip_id","start"]),
        run: |ss, a| { let (ti, ci) = loc(&ss.project, sa(a, "clip_id")?)?; let nid = ss.project.gen_id("clip"); let mut c = ss.project.tracks[ti].clips[ci].clone(); c.id = nid.clone(); c.start = na(a, "start")?; c.transition_in = None;
            let dst = match a.get("track_id").and_then(|v| v.as_str()) { Some(t) => ss.project.tracks.iter().position(|x| x.id == t).ok_or("no such track")?, None => ti }; unlocked(&ss.project, dst)?; ss.project.tracks[dst].clips.push(c); Ok(json!({"clip_id":nid})) } },
    Command { name: "rate_stretch", description: "Rate stretch: make a clip fit `duration` seconds by changing its speed (source range preserved).", mutates: true,
        schema: obj(vec![("clip_id", s("clip")), ("duration", n("new timeline duration"))], &["clip_id","duration"]),
        run: |ss, a| { let d = na(a, "duration")?; if d < 0.04 { return Err("duration too short".into()); } let (ti, ci) = loc(&ss.project, sa(a, "clip_id")?)?; let c = &mut ss.project.tracks[ti].clips[ci]; let sp = c.speed * c.duration / d; if !(0.1..=100.0).contains(&sp) { return Err(format!("resulting speed {sp:.2} out of 0.1..100")); } c.speed = sp; c.duration = d; Ok(json!({"speed":sp})) } },
    Command { name: "add_marker", description: "Add a timeline marker (cut points, beats, notes). Returns marker id.", mutates: true,
        schema: obj(vec![("time", n("seconds")), ("name", s("label")), ("duration", n("range length, 0 = point")), ("color", s("color name"))], &["time"]),
        run: |ss, a| { let id = new_marker_id(&mut ss.project); ss.project.markers.push(Marker { id: id.clone(), time: na(a, "time")?, duration: on(a, "duration", 0.0), name: a.get("name").and_then(|v| v.as_str()).unwrap_or("").into(), color: a.get("color").and_then(|v| v.as_str()).unwrap_or("blue").into() }); ss.project.markers.sort_by(|x, y| x.time.partial_cmp(&y.time).unwrap()); Ok(json!({"marker_id":id})) } },
    Command { name: "list_markers", description: "List all markers sorted by time.", mutates: false, schema: obj(vec![], &[]),
        run: |ss, _| Ok(json!({"markers": ss.project.markers})) },
    Command { name: "remove_marker", description: "Delete a marker by id.", mutates: true, schema: obj(vec![("marker_id", s("id"))], &["marker_id"]),
        run: |ss, a| { let id = sa(a, "marker_id")?; let l = ss.project.markers.len(); ss.project.markers.retain(|m| m.id != id); if l == ss.project.markers.len() { Err("no such marker".into()) } else { Ok(json!({"ok":true})) } } },
    Command { name: "snap_time", description: "Find the nearest edit point (clip edges, markers, playhead 0) to `time` within `tolerance` seconds. Use for precise placement.", mutates: false,
        schema: obj(vec![("time", n("seconds")), ("tolerance", n("seconds, default 0.1"))], &["time"]),
        run: |ss, a| { let t = na(a, "time")?; let tol = on(a, "tolerance", 0.1); let mut pts = vec![0.0]; for tr in &ss.project.tracks { for c in &tr.clips { pts.push(c.start); pts.push(c.start + c.duration); } } for m in &ss.project.markers { pts.push(m.time); }
            let best = pts.into_iter().filter(|p| (p - t).abs() <= tol).min_by(|x, y| (x - t).abs().partial_cmp(&(y - t).abs()).unwrap()); Ok(json!({"snapped": best.is_some(), "time": best.unwrap_or(t)})) } },
    Command { name: "list_edit_points", description: "All cut points across tracks (sorted, deduped) - for scene-aware AI edits.", mutates: false, schema: obj(vec![], &[]),
        run: |ss, _| { let mut v: Vec<f64> = vec![]; for tr in &ss.project.tracks { for c in &tr.clips { v.push(c.start); v.push(c.start + c.duration); } } v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v.dedup_by(|a, b| (*a - *b).abs() < 1e-3); Ok(json!({"points": v})) } },
    Command { name: "set_track", description: "Track controls: lock (blocks edits from the timeline tools), name, reorder (z-position index, 0 = bottom).", mutates: true,
        schema: obj(vec![("track_id", s("track")), ("locked", json!({"type":"boolean"})), ("name", s("name")), ("index", n("new z index"))], &["track_id"]),
        run: |ss, a| { let tid = sa(a, "track_id")?; let ti = ss.project.tracks.iter().position(|t| t.id == tid).ok_or("no such track")?;
            if let Some(l) = a.get("locked").and_then(|v| v.as_bool()) { ss.project.tracks[ti].locked = l; } if let Some(nm) = a.get("name").and_then(|v| v.as_str()) { ss.project.tracks[ti].name = nm.into(); }
            if let Some(ix) = a.get("index").and_then(|v| v.as_f64()) { let t = ss.project.tracks.remove(ti); let ix = (ix as usize).min(ss.project.tracks.len()); ss.project.tracks.insert(ix, t); } Ok(json!({"ok":true})) } },
    Command { name: "delete_track", description: "Delete a track and all its clips.", mutates: true, schema: obj(vec![("track_id", s("track"))], &["track_id"]),
        run: |ss, a| { let tid = sa(a, "track_id")?; let l = ss.project.tracks.len(); ss.project.tracks.retain(|t| t.id != tid); if l == ss.project.tracks.len() { Err("no such track".into()) } else { Ok(json!({"ok":true})) } } },
    Command { name: "split_all_at", description: "Razor: split every unlocked clip crossing `at` on all tracks (Premiere 'razor all').", mutates: true,
        schema: obj(vec![("at", n("seconds"))], &["at"]),
        run: |ss, a| { let at = na(a, "at")?; let mut cnt = 0; let mut tracks = std::mem::take(&mut ss.project.tracks);
            for tr in tracks.iter_mut() { if tr.locked { continue; } if let Some(ci) = tr.clips.iter().position(|c| c.start < at - EPS && c.start + c.duration > at + EPS) { let id = ss.project.gen_id("clip"); let original=tr.clips[ci].id.clone();let r = cut(&mut tr.clips[ci], at, id.clone()); tr.clips.push(r); ss.project.grade_scopes.copy_binding(&original,&id);cnt += 1; } }
            ss.project.tracks = tracks; Ok(json!({"split": cnt})) } },
    Command { name: "remove_silence_cuts", description: "Given [start,end] ranges (e.g. from silence detection), ripple-extract them from a track.", mutates: true,
        schema: obj(vec![("track_id", s("track")), ("ranges", json!({"type":"array","items":{"type":"array","items":{"type":"number"}}}))], &["track_id","ranges"]),
        run: |ss, a| { let tid = sa(a, "track_id")?.to_string(); let mut r: Vec<(f64, f64)> = a.get("ranges").and_then(|v| v.as_array()).ok_or("ranges")?.iter().filter_map(|p| { let p = p.as_array()?; Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?)) }).collect();
            r.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap()); for (st, en) in &r { let ids = [ss.project.gen_id("clip"), ss.project.gen_id("clip")]; let _ = ids; crate::commands::dispatch(ss, "lift_range", &json!({"track_id": tid, "start": st, "end": en, "extract": true}))?; } Ok(json!({"removed": r.len()})) } },
] }
