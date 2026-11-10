//! beat_cut: cut on the beat (Premiere "Automate to Sequence" at markers, CapCut beat sync). The listed clips are cut up and laid
//! end to end on the beats of a music track: one cut per beat (or every Nth), sources taken in turn, each source picking up where
//! it left off (looping back to its in point once used up), with an optional zoom bump on every cut (scale 1 + bump on the cut,
//! easing back to 1 over bump_decay seconds, a tagged expression op layered on the clip's own scale).
//! Beats: an explicit list (timeline seconds), a bpm grid (bpm + offset, until end), or a music clip on the timeline whose audio is
//! beat-tracked (motion.rs detect_beats; good on clear percussive music). Cut times snap to frames. The source clips are replaced by
//! the cut-up montage on their track (or track_id); each cut is a copy of its source (effects, grade, masks, keys local to the cut).
use super::kit::*;
use crate::keyframe::{Op, Prop};
use crate::model::*;

const ARGS: &[&str] = &["clip_ids", "beats", "bpm", "offset", "audio_clip_id", "every", "start", "end", "track_id", "bump", "bump_decay"];
const TAG: &str = "beat_bump";
fn num(a: &Value, k: &str, lo: f64, hi: f64) -> Result<Option<f64>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => { let x = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{k} must be a number"))?; if x < lo || x > hi { Err(format!("{k} must be within {lo}..{hi}, got {x}")) } else { Ok(Some(x)) } } } }

pub fn commands() -> Vec<Command> { vec![
    Command { name: "beat_cut", description: "Cut on the beat: lay clips end to end on a music track's beats (every Nth), sources in turn, optional zoom bump per cut.", mutates: true,
        schema: obj(vec![("clip_ids", json!({"type":"array","items":{"type":"string"},"description":"the clips to cut up, used in this order (they are replaced by the montage)"})),
            ("beats", json!({"type":"array","items":{"type":"number"},"description":"beat times, timeline seconds, increasing (e.g. from track_beats); the last one ends the montage unless end is given"})),
            ("bpm", n("a beat grid instead: beats per minute, 20..400 (needs end)")),
            ("offset", n("bpm grid: timeline time of the first beat, default 0")),
            ("audio_clip_id", s("or a music clip on the timeline: its audio is beat-tracked (clear percussive music works best); the montage ends with it")),
            ("every", n("cut on every Nth beat, 1..64, default 1")),
            ("start", n("ignore beats before this timeline time")),
            ("end", n("timeline time the montage stops (the last cut runs to it)")),
            ("track_id", s("track to lay the montage on, default the first clip's track")),
            ("bump", n("zoom bump on every cut: scale 1 + bump on the cut, 0..1, default 0 (off)")),
            ("bump_decay", n("seconds the bump takes to settle back to 1, 0.02..4, default a quarter of the beat gap"))], &["clip_ids"]),
        run: |ss, a| { let o = a.as_object().ok_or("beat_cut: arguments must be an object")?;
            for k in o.keys() { if !ARGS.contains(&k.as_str()) { return Err(format!("beat_cut: unknown argument '{k}' (valid: {})", ARGS.join(", "))); } }
            // sources
            let arr = a.get("clip_ids").and_then(|v| v.as_array()).ok_or("beat_cut needs clip_ids (the clips to cut up)")?;
            if arr.is_empty() { return Err("clip_ids is empty: give the clips to cut up".into()); }
            let mut srcs: Vec<Clip> = vec![];
            for x in arr { let id = x.as_str().ok_or("clip_ids must be clip ids (strings)")?; let c = ss.project.find_clip(id).ok_or_else(|| format!("no such clip {id}"))?;
                if srcs.iter().any(|s| s.id == id) { return Err(format!("clip {id} is listed twice in clip_ids")); }
                if c.props.get("audio_only").is_some_and(|q| q.base >= 0.5) { return Err(format!("clip {id} is audio only; beat_cut cuts picture clips")); }
                if c.time_remap.is_some() || c.speed_curve.is_some() { return Err(format!("clip {id} has a time remap / speed ramp; beat_cut needs a constant speed clip")); }
                srcs.push(c.clone()); }
            let every = num(a, "every", 1.0, 64.0)?.unwrap_or(1.0); if every.fract() != 0.0 { return Err(format!("every must be a whole number 1..64, got {every}")); } let every = every as usize;
            let bump = num(a, "bump", 0.0, 1.0)?.unwrap_or(0.0); let bump_decay = num(a, "bump_decay", 0.02, 4.0)?;
            let t_start = num(a, "start", 0.0, 1e7)?; let t_end = num(a, "end", 0.0, 1e7)?;
            let given = ["beats", "bpm", "audio_clip_id"].iter().filter(|k| a.get(**k).is_some_and(|v| !v.is_null())).count();
            if given == 0 { return Err("beat_cut needs the beats: beats (a list of timeline seconds), bpm (+ offset, end) or audio_clip_id (a music clip to beat-track)".into()); }
            if given > 1 { return Err("give only one of beats, bpm or audio_clip_id".into()); }
            if a.get("offset").is_some() && a.get("bpm").is_none() { return Err("offset goes with bpm".into()); }
            let (mut beats, default_end, mut bpm): (Vec<f64>, Option<f64>, Option<f64>) = if let Some(b) = a.get("beats").filter(|v| !v.is_null()) {
                let b = b.as_array().ok_or("beats must be a list of timeline seconds")?; let mut v = vec![];
                for x in b { let t = x.as_f64().filter(|t| t.is_finite() && *t >= 0.0).ok_or("beats must be timeline seconds >= 0")?; if let Some(p) = v.last() { if t <= *p { return Err(format!("beats must be increasing ({t} after {p})")); } } v.push(t); }
                if v.len() < 2 { return Err("beats needs at least 2 beats (a cut runs from one beat to the next)".into()); }
                let last = *v.last().unwrap(); (v, Some(last), None)
            } else if a.get("bpm").is_some() {
                let bpm = num(a, "bpm", 20.0, 400.0)?.unwrap(); let off = num(a, "offset", 0.0, 1e7)?.unwrap_or(0.0);
                let end = t_end.ok_or("bpm needs end (the timeline time where the montage stops)")?; let per = 60.0 / bpm; let mut v = vec![]; let mut k = 0usize;
                loop { let t = off + k as f64 * per; if t >= end - 1e-9 || v.len() > 100000 { break; } v.push(t); k += 1; }
                (v, Some(end), Some(bpm))
            } else {
                let mid = a.get("audio_clip_id").and_then(|v| v.as_str()).ok_or("audio_clip_id must be a clip id")?;
                let m = ss.project.find_clip(mid).ok_or_else(|| format!("audio_clip_id: no such clip {mid}"))?;
                if srcs.iter().any(|s| s.id == mid) { return Err(format!("audio_clip_id {mid} is also in clip_ids")); }
                let aid = match &m.source { Source::Media { asset } => asset.clone(), _ => return Err(format!("audio_clip_id: clip {mid} has no audio (not a media clip)")) };
                let path = ss.project.resolve(&ss.project.assets.get(&aid).ok_or_else(|| format!("audio_clip_id: asset {aid} is missing"))?.path);
                let sp = if m.speed > 0.0 { m.speed } else { 1.0 };
                let (bpm, b, _conf) = crate::motion::detect_beats(&path, m.in_point, m.duration * sp, 70.0, 180.0).map_err(|e| format!("audio_clip_id: beat tracking failed: {e}"))?;
                let v: Vec<f64> = b.iter().map(|t| m.start + (t - m.in_point) / sp).filter(|t| *t >= m.start - 1e-9 && *t < m.start + m.duration).collect();
                (v, Some(m.start + m.duration), Some(bpm)) };
            if let Some(s0) = t_start { beats.retain(|t| *t >= s0 - 1e-9); }
            let end = t_end.or(default_end).unwrap(); beats.retain(|t| *t < end - 1e-9);
            let sel: Vec<f64> = beats.iter().cloned().step_by(every).collect();
            if sel.is_empty() { return Err(format!("no beats left between start and end ({end})")); }
            if bpm.is_none() && beats.len() > 1 { let mut g: Vec<f64> = beats.windows(2).map(|w| w[1] - w[0]).collect(); g.sort_by(|x, y| x.total_cmp(y)); bpm = Some(60.0 / g[g.len() / 2]); }
            let fps = if ss.project.fps > 0.0 { ss.project.fps } else { 30.0 }; let snap = |t: f64| (t * fps).round() / fps;
            let mut bounds: Vec<f64> = sel.iter().map(|t| snap(*t)).collect(); bounds.push(snap(end)); bounds.dedup_by(|b, a| (*b - *a).abs() < 1e-9);
            if bounds.len() < 2 { return Err("the beats leave no room for a cut (end is on the first beat)".into()); }
            let gap = { let mut g: Vec<f64> = bounds.windows(2).map(|w| w[1] - w[0]).collect(); g.sort_by(|x, y| x.total_cmp(y)); g[g.len() / 2] / every as f64 };
            let decay = bump_decay.unwrap_or((gap * 0.25).clamp(0.02, 4.0));
            // destination track: free of other picture clips over the montage
            let tid = match a.get("track_id").filter(|v| !v.is_null()) { Some(v) => v.as_str().ok_or("track_id must be a string")?.to_string(), None => ss.project.tracks.iter().find(|t| t.clips.iter().any(|c| c.id == srcs[0].id)).map(|t| t.id.clone()).ok_or("source track not found")? };
            let ti = ss.project.tracks.iter().position(|t| t.id == tid).ok_or_else(|| format!("no such track {tid}"))?;
            if ss.project.tracks[ti].locked { return Err(format!("track {tid} is locked")); }
            let (m0, m1) = (bounds[0], *bounds.last().unwrap());
            if let Some(c) = ss.project.tracks[ti].clips.iter().find(|c| !srcs.iter().any(|s| s.id == c.id) && c.start < m1 - 1e-9 && c.start + c.duration > m0 + 1e-9) { return Err(format!("track {tid} is busy over {m0}..{m1}s: clip {} is in the way (give another track_id)", c.id)); }
            // remove the sources, then lay the cuts
            let ids: Vec<String> = srcs.iter().map(|s| s.id.clone()).collect();
            for t in ss.project.tracks.iter_mut() { t.clips.retain(|c| !ids.contains(&c.id)); }
            let mut cursor: Vec<f64> = srcs.iter().map(|s| s.in_point).collect(); let mut out = vec![];
            for (i, w) in bounds.windows(2).enumerate() {
                let k = i % srcs.len(); let s = &srcs[k]; let len = w[1] - w[0]; let sp = if s.speed > 0.0 { s.speed } else { 1.0 };
                let mut looped = false; if cursor[k] + len * sp > s.in_point + s.duration * sp + 1e-9 && cursor[k] > s.in_point + 1e-9 { cursor[k] = s.in_point; looped = true; }
                let nid = ss.project.gen_id("clip"); let mut c = s.clone(); c.id = nid.clone(); c.start = w[0]; c.duration = len; c.in_point = cursor[k]; c.transition_in = None; c.name = None;
                if let Some(q) = c.props.get_mut("scale") { q.expr.retain(|o| !matches!(o, Op::Bezier { tag, .. } if tag == TAG)); }
                if bump > 0.0 { let q = c.props.entry("scale".into()).or_insert_with(|| Prop::new(1.0)); q.expr.push(Op::Bezier { x1: 0.2, y1: 0.8, x2: 0.3, y2: 1.0, from: 1.0 + bump, to: 1.0, t0: 0.0, duration: decay.min(len), combine: "mul".into(), tag: TAG.into() }); }
                out.push(json!({"clip_id": nid, "start": w[0], "duration": len, "source_clip_id": s.id, "in_point": cursor[k], "looped": looped}));
                cursor[k] += len * sp; ss.project.tracks[ti].clips.push(std::sync::Arc::new(c)); ss.project.grade_scopes.copy_binding(&s.id, &nid);
            }
            Ok(json!({"cuts": out, "track_id": tid, "beats_used": bounds.len() - 1, "every": every, "bpm": bpm, "end": m1, "bump": bump, "bump_decay": if bump > 0.0 { json!(decay) } else { Value::Null }})) } },
] }
