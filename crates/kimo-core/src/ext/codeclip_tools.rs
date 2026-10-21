//! Code clip tools (Source::Code, see codeclip.rs): add a web project as a clip, keyframe its declared params, and check that it
//! draws the same pixels for the same t no matter what was drawn before.
use super::kit::*;
use crate::codeclip::{self, CodeClip};
use crate::keyframe::{Ease, Prop};
use crate::model::*;
use std::collections::BTreeMap;

fn code_of<'a>(p: &'a Project, id: &str) -> Result<(&'a Clip, &'a CodeClip), String> {
    let c = p.find_clip(id).ok_or_else(|| format!("no such clip {id}"))?;
    match &c.source { Source::Code(cc) => Ok((c, cc)), _ => Err(format!("clip {id} is not a code clip (add_code_clip makes one)")) }
}
fn in_range(cc: &CodeClip, name: &str, v: f64) -> Result<(), String> {
    if !v.is_finite() { return Err(format!("param '{name}' value must be a finite number")); }
    if let Some([lo, hi]) = cc.ranges.get(name) { if v < *lo || v > *hi { return Err(format!("param '{name}' must be between {lo} and {hi} (kimo.json), got {v}")); } }
    Ok(())
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_code_clip", description: "Add a code clip: a web project folder (three.js etc.) whose window.renderFrame(t, params) draws each frame, in a sandboxed browser.", mutates: true,
        schema: obj(vec![("track_id", s("track")), ("path", s("project folder holding index.html (absolute, or relative to the project file)")), ("start", n("timeline seconds (default 0)")), ("duration", n("seconds")),
            ("width", n("render width px (default canvas)")), ("height", n("render height px (default canvas)")), ("entry", s("entry page (default kimo.json entry or index.html)")),
            ("params", json!({"type":"object","description":"initial values {name: number}; names must be declared in kimo.json when it declares params"}))], &["track_id", "path", "duration"]),
        run: |ss, a| { let tid = str_arg(a, "track_id")?.to_string(); let path = str_arg(a, "path")?.to_string();
            let (start, dur) = (opt_num(a, "start", 0.0), num_arg(a, "duration")?);
            if !start.is_finite() || start < 0.0 || !dur.is_finite() || dur <= 0.0 { return Err("start must be >= 0 and duration > 0 (seconds)".into()); }
            let ti = ss.project.tracks.iter().position(|t| t.id == tid).ok_or_else(|| format!("no such track {tid}"))?;
            if ss.project.tracks[ti].locked { return Err(format!("track {tid} is locked")); }
            let dir = std::path::PathBuf::from(ss.project.resolve(&path));
            let man = codeclip::read_manifest(&dir)?;
            let entry = a.get("entry").and_then(|v| v.as_str()).map(String::from).unwrap_or(man.entry.clone());
            codeclip::validate_dir(&dir, &entry)?;
            let (w, h) = (opt_num(a, "width", ss.project.width as f64), opt_num(a, "height", ss.project.height as f64));
            if !(16.0..=4096.0).contains(&w) || !(16.0..=4096.0).contains(&h) || w.fract() != 0.0 || h.fract() != 0.0 { return Err(format!("width and height must be between 16 and 4096 (whole pixels), got {w}x{h}")); }
            let mut cc = CodeClip { dir: path.clone(), entry, width: w as u32, height: h as u32, params: BTreeMap::new(), ranges: BTreeMap::new() };
            for (k, (d, r)) in &man.params { cc.params.insert(k.clone(), Prop::new(*d)); if let Some(r) = r { cc.ranges.insert(k.clone(), *r); } }
            if let Some(ps) = a.get("params") { let o = ps.as_object().ok_or("argument 'params' must be an object (got another type)")?;
                for (k, v) in o { let v = v.as_f64().ok_or_else(|| format!("param '{k}' value must be a number"))?;
                    if !man.params.is_empty() && !man.params.contains_key(k) { return Err(format!("unknown param '{k}': kimo.json does not declare it. Valid: {}", man.params.keys().cloned().collect::<Vec<_>>().join(", "))); }
                    codeclip::check_param_name(k)?; in_range(&cc, k, v)?; cc.params.insert(k.clone(), Prop::new(v)); } }
            let id = ss.project.gen_id("clip");
            let c = Clip { id: id.clone(), source: Source::Code(cc.clone()), start, duration: dur, in_point: 0.0, speed: 1.0, speed_curve: None, time_remap: None, props: default_props(), grade_graph: None, effects: vec![], blend: "normal".into(), mask: None, time_mode: TimeMode::Normal, corners: None, audio: Audio::default(), motion_blur: 0.0, echo: None, motion_path: None, posterize_fps: 0.0, text_fx: None, parent: None, inherit: Default::default(), name: None, role: None, tags: vec![], transition_in: None };
            ss.project.tracks[ti].clips.push(std::sync::Arc::new(c));
            let params: BTreeMap<String, f64> = cc.params.iter().map(|(k, p)| (k.clone(), p.base)).collect();
            Ok(json!({"clip_id": id, "entry": cc.entry, "width": cc.width, "height": cc.height, "params": params, "ranges": cc.ranges})) } },
    Command { name: "set_code_clip_param", description: "Set a code clip param: constant, or a keyframe at code time t (with ease). The page reads it in renderFrame(t, params).", mutates: true,
        schema: obj(vec![("clip_id", s("code clip")), ("name", s("declared param")), ("value", n("number")), ("time", n("code clip seconds (the t renderFrame gets); omit for a constant (clears keys)")),
            ("ease", json!({"type":"string","enum":crate::keyframe::EASE_NAMES}))], &["clip_id", "name", "value"]),
        run: |ss, a| { let id = str_arg(a, "clip_id")?.to_string(); let name = str_arg(a, "name")?.to_string(); let v = num_arg(a, "value")?;
            let (c, cc) = code_of(&ss.project, &id)?;
            if !cc.params.contains_key(&name) { let names: Vec<String> = cc.params.keys().cloned().collect(); return Err(format!("unknown param '{name}' on code clip {id}. Valid: {}", if names.is_empty() { "(none declared; declare them in kimo.json or add_code_clip params)".to_string() } else { names.join(", ") })); }
            in_range(cc, &name, v)?;
            let _ = c; let t = match a.get("time") { None | Some(Value::Null) => None, Some(_) => { let t = num_arg(a, "time")?; if !t.is_finite() || t < 0.0 { return Err(format!("time must be >= 0 (code clip seconds), got {t}")); } Some(t) } };
            let e: Ease = serde_json::from_value(a.get("ease").cloned().unwrap_or(json!("linear"))).map_err(|e| format!("unknown ease: {e}"))?;
            let cm = ss.project.find_clip_mut(&id).ok_or("no such clip")?; let Source::Code(cc) = &mut cm.source else { unreachable!() };
            let p = cc.params.get_mut(&name).expect("checked");
            match t { None => { *p = Prop::new(v); } Some(t) => p.set_key(t, v, e) }
            let keys = p.keys.len();
            Ok(json!({"clip_id": id, "name": name, "value": v, "time": t, "keys": keys})) } },
    Command { name: "check_code_clip_determinism", description: "Render a code clip frame, then earlier frames, then it again and on a fresh page; errors unless all match (renderFrame must use only t, params).", mutates: false,
        schema: obj(vec![("clip_id", s("code clip")), ("time", n("clip-local seconds (default 0)")), ("before", n("earlier frames drawn in between (default 10, max 120)"))], &["clip_id"]),
        run: |ss, a| { let id = str_arg(a, "clip_id")?.to_string(); let (c, cc) = code_of(&ss.project, &id)?;
            let t = opt_num(a, "time", 0.0); if !t.is_finite() || t < 0.0 || t >= c.duration { return Err(format!("time must be >= 0 and < the clip duration {} (clip-local seconds), got {t}", c.duration)); }
            let before = opt_num(a, "before", 10.0); if !(0.0..=120.0).contains(&before) || before.fract() != 0.0 { return Err(format!("before must be between 0 and 120 (whole frames), got {before}")); }
            let k = codeclip::slot_of(c.display_time(t), ss.project.fps).min(codeclip::slot_count(c, ss.project.fps) - 1);
            codeclip::check_determinism(&ss.project, c, cc, k, before as usize) } },
] }
