//! add_punch_ins: the talking-head jump-cut zoom. Each clip of a track (or a given list) is set wide or punched in, alternating
//! from cut to cut (or all punched), the punch scaling about a focus point of the canvas that stays put (a face, say), so the
//! picture never shows an edge. Sets each clip's scale / x / y (wide: base_scale at x = y = 0; punched: base_scale * scale about
//! the focus), so a second call with the same arguments gives the same result and remove puts every clip back to wide.
//! ease > 0 makes each punch a quick push-in from wide over that many seconds instead of a hard punch on the cut.
use super::kit::*;
use crate::keyframe::{Ease, Prop};

const ARGS: &[&str] = &["track_id", "clip_ids", "scale", "pattern", "start", "focus", "base_scale", "ease", "remove"];
fn num(a: &Value, k: &str, d: f64, lo: f64, hi: f64) -> Result<f64, String> { match a.get(k) { None => Ok(d), Some(v) => { let x = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{k} must be a number"))?; if x < lo || x > hi { Err(format!("{k} must be within {lo}..{hi}, got {x}")) } else { Ok(x) } } } }
fn word(a: &Value, k: &str, d: &str, ok: &[&str]) -> Result<String, String> { let v = match a.get(k) { None => d.to_string(), Some(v) => v.as_str().ok_or_else(|| format!("{k} must be a string"))?.to_string() }; if ok.contains(&v.as_str()) { Ok(v) } else { Err(format!("{k} must be one of {}, got '{v}'", ok.join(", "))) } }

/// Offset (px) that keeps canvas point (fx, fy) in place when a clip at offset 0 is scaled by k about the canvas centre (cx, cy).
pub fn focus_offset(fx: f64, fy: f64, cx: f64, cy: f64, k: f64) -> (f64, f64) { ((fx - cx) * (1.0 - k), (fy - cy) * (1.0 - k)) }

pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_punch_ins", description: "Jump-cut punch-ins: alternate clips of a track between wide and zoomed in about a focus point (talking-head style).", mutates: true,
        schema: obj(vec![("track_id", s("punch the clips of this track, in time order (or give clip_ids)")),
            ("clip_ids", json!({"type":"array","items":{"type":"string"},"description":"the clips to punch, taken in time order (instead of track_id)"})),
            ("scale", n("the punched clips' zoom over the wide ones, 1.01..3, default 1.15")),
            ("pattern", json!({"type":"string","enum":["alternate","all"],"description":"alternate (default): wide, punched, wide...; all: every clip punched"})),
            ("start", json!({"type":"string","enum":["wide","tight"],"description":"alternate: whether the first clip is wide (default) or punched"})),
            ("focus", json!({"type":"array","items":{"type":"number"},"description":"[x, y] 0..1 of the canvas that stays put when punching (e.g. the face), default [0.5, 0.5]"})),
            ("base_scale", n("the wide clips' scale, 1..3, default 1")),
            ("ease", n("seconds each punch takes as a quick push-in from wide, 0 (default) = hard punch on the cut, max 2")),
            ("remove", json!({"type":"boolean","description":"true sets the clips back to wide (base_scale, x = y = 0)"}))], &[]),
        run: |ss, a| { let o = a.as_object().ok_or("add_punch_ins: arguments must be an object")?;
            for k in o.keys() { if !ARGS.contains(&k.as_str()) { return Err(format!("add_punch_ins: unknown argument '{k}' (valid: {})", ARGS.join(", "))); } }
            let ids: Vec<String> = match (a.get("track_id"), a.get("clip_ids")) {
                (Some(_), Some(_)) => return Err("give track_id or clip_ids, not both".into()),
                (None, None) => return Err("give track_id (punch its clips) or clip_ids".into()),
                (Some(t), None) => { let tid = t.as_str().ok_or("track_id must be a string")?; let tr = ss.project.tracks.iter().find(|x| x.id == tid).ok_or_else(|| format!("no such track {tid}"))?;
                    let mut v: Vec<(f64, String)> = tr.clips.iter().filter(|c| c.props.get("audio_only").map_or(true, |q| q.base < 0.5)).map(|c| (c.start, c.id.clone())).collect(); v.sort_by(|x, y| x.0.total_cmp(&y.0)); v.into_iter().map(|x| x.1).collect() }
                (None, Some(l)) => { let arr = l.as_array().ok_or("clip_ids must be an array of clip ids")?; let mut v = vec![];
                    for x in arr { let id = x.as_str().ok_or("clip_ids must be strings")?; let c = ss.project.find_clip(id).ok_or_else(|| format!("no such clip {id}"))?; if v.iter().any(|(_, i): &(f64, String)| i == id) { return Err(format!("clip {id} is listed twice")); } v.push((c.start, id.to_string())); }
                    v.sort_by(|x, y| x.0.total_cmp(&y.0)); v.into_iter().map(|x| x.1).collect() } };
            if ids.is_empty() { return Err("no clips to punch (the track is empty)".into()); }
            let k = num(a, "scale", 1.15, 1.01, 3.0)?; let base = num(a, "base_scale", 1.0, 1.0, 3.0)?; let ease = num(a, "ease", 0.0, 0.0, 2.0)?;
            let pattern = word(a, "pattern", "alternate", &["alternate", "all"])?; let start = word(a, "start", "wide", &["wide", "tight"])?;
            let remove = match a.get("remove") { None => false, Some(v) => v.as_bool().ok_or("remove must be true or false")? };
            let (fx, fy) = match a.get("focus") { None => (0.5, 0.5), Some(v) => { let arr = v.as_array().filter(|x| x.len() == 2).ok_or("focus must be [x, y] in 0..1")?;
                let (x, y) = (arr[0].as_f64().ok_or("focus must be numbers")?, arr[1].as_f64().ok_or("focus must be numbers")?); if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) { return Err(format!("focus must be [x, y] within 0..1, got [{x}, {y}]")); } (x, y) } };
            // a clip whose transform is animated (keys or an expression) is refused rather than overwritten
            for id in &ids { let c = ss.project.find_clip(id).unwrap(); for key in ["scale", "x", "y"] { if let Some(q) = c.props.get(key) { /* xform-lint: ok (animation check, not a transform read) */ if !q.keys.is_empty() && !(q.keys.len() == 2 && q.keys[0].t == 0.0) || !q.expr.is_empty() { return Err(format!("clip {id} has {key} animated (keys or an expression); clear it first, punch-ins set {key} on every clip")); } } } }
            let (w, h) = (ss.project.width as f64, ss.project.height as f64); let (ox, oy) = focus_offset(fx * w, fy * h, w / 2.0, h / 2.0, k);
            let mut out = vec![]; let mut punched = 0;
            for (i, id) in ids.iter().enumerate() {
                let tight = !remove && (pattern == "all" || (i % 2 == 1) == (start == "wide"));
                let c = ss.project.find_clip_mut(id).unwrap(); let e = ease.min(c.duration / 2.0);
                let (sv, xv, yv) = if tight { (base * k, ox, oy) } else { (base, 0.0, 0.0) };
                for (key, wide, v) in [("scale", base, sv), ("x", 0.0, xv), ("y", 0.0, yv)] { let mut q = Prop::new(v); if tight && e > 0.0 { q.set_key(0.0, wide, Ease::InOut); q.set_key(e, v, Ease::InOut); } c.props.insert(key.into(), q); }
                if tight { punched += 1; } out.push(json!({"clip_id": id, "punched": tight, "scale": sv, "x": xv, "y": yv}));
            }
            Ok(json!({"clips": out, "punched": punched, "wide": ids.len() - punched, "focus": [fx, fy], "ease": ease})) } },
] }
