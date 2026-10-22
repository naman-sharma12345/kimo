//! add_blur_fill: the vertical-video fill. A clip that does not fill the frame (a landscape shot on a 9:16 canvas) is shown
//! over a blurred, darkened, cover-scaled copy of itself instead of black bars. The copy goes on a new track directly under the
//! clip's track and plays the same frames (same source, in point, speed and timing); it is muted, untransformed and carries a
//! blur and an exposure effect tagged as a fill. The clip itself is set to fit contain. A second call updates the fill; remove
//! deletes it (and its track when that is left empty). Moving or trimming the clip later does not move the fill.
use super::kit::*;
use crate::keyframe::Prop;
use crate::model::*;

const ARGS: &[&str] = &["clip_id", "blur", "dim", "zoom", "remove"];
fn num(a: &Value, k: &str, d: f64, lo: f64, hi: f64) -> Result<f64, String> { match a.get(k) { None => Ok(d), Some(v) => { let x = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{k} must be a number"))?; if x < lo || x > hi { Err(format!("{k} must be within {lo}..{hi}, got {x}")) } else { Ok(x) } } } }
fn is_fill(e: &Effect) -> bool { e.strings.get("fill").map_or(false, |v| v == "1") }
/// (track index, clip index) of `fg`'s fill: a clip tagged as a fill with the same source, start, duration and in point.
fn find_fill(p: &Project, fg: &Clip) -> Option<(usize, usize)> {
    for (ti, t) in p.tracks.iter().enumerate() { for (ci, c) in t.clips.iter().enumerate() {
        if c.id != fg.id && c.effects.iter().any(is_fill) && c.start == fg.start && c.duration == fg.duration && c.in_point == fg.in_point && serde_json::to_string(&c.source).ok() == serde_json::to_string(&fg.source).ok() { return Some((ti, ci)); } } }
    None
}
fn fx(p: &mut Project, kind: &str, k: &str, v: f64) -> Effect { let mut e = Effect { id: p.gen_id("fx"), kind: kind.into(), params: Default::default(), strings: Default::default() }; e.params.insert(k.into(), Prop::new(v)); e.strings.insert("fill".into(), "1".into()); e }

pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_blur_fill", description: "Blur fill: show a clip that does not fill the frame (e.g. landscape in 9:16) over a blurred, darkened copy of itself.", mutates: true,
        schema: obj(vec![("clip_id", s("the media clip to frame (set to fit contain)")),
            ("blur", n("blur radius of the fill in px, 0..300, default 4% of the canvas width")),
            ("dim", n("how much darker the fill is, in stops, 0..6, default 1")),
            ("zoom", n("extra zoom of the fill over cover, 1..3, default 1")),
            ("remove", json!({"type":"boolean","description":"true deletes the clip's fill"}))], &["clip_id"]),
        run: |ss, a| { let o = a.as_object().ok_or("add_blur_fill: arguments must be an object")?;
            for k in o.keys() { if !ARGS.contains(&k.as_str()) { return Err(format!("add_blur_fill: unknown argument '{k}' (valid: {})", ARGS.join(", "))); } }
            let cid = str_arg(a, "clip_id")?.to_string(); let fg = ss.project.find_clip(&cid).ok_or_else(|| format!("no such clip {cid}"))?.clone();
            if !matches!(fg.source, Source::Media { .. }) { return Err(format!("clip {cid} is not a media clip; a blur fill copies a video or image")); }
            if fg.effects.iter().any(is_fill) { return Err(format!("clip {cid} is itself a blur fill; give the clip it frames")); }
            let remove = match a.get("remove") { None => false, Some(v) => v.as_bool().ok_or("remove must be true or false")? };
            let blur = num(a, "blur", (ss.project.width as f64 * 0.04).round().max(1.0), 0.0, 300.0)?; let dim = num(a, "dim", 1.0, 0.0, 6.0)?; let zoom = num(a, "zoom", 1.0, 1.0, 3.0)?;
            let ti = ss.project.tracks.iter().position(|t| t.clips.iter().any(|c| c.id == cid)).unwrap();
            if remove { let Some((fti, fci)) = find_fill(&ss.project, &fg) else { return Ok(json!({"removed": 0})) };
                let fid = ss.project.tracks[fti].clips[fci].id.clone(); ss.project.tracks[fti].clips.remove(fci);
                let dropped = ss.project.tracks[fti].clips.is_empty() && ss.project.tracks[fti].name == "blur fill"; if dropped { ss.project.tracks.remove(fti); }
                return Ok(json!({"removed": 1, "fill_clip_id": fid, "track_removed": dropped})); }
            let effects = vec![fx(&mut ss.project, "blur", "radius", blur), fx(&mut ss.project, "exposure", "stops", -dim)];
            let (fti, fci, created) = match find_fill(&ss.project, &fg) { Some((t, c)) => (t, c, false), None => {
                let tid = ss.project.gen_id("track"); ss.project.tracks.insert(ti, Track { id: tid, clips: vec![], locked: false, muted: false, name: "blur fill".into() });
                let mut c = fg.clone(); c.id = ss.project.gen_id("clip"); c.transition_in = None; c.mask = None; c.motion_path = None; c.echo = None; c.text_fx = None; c.corners = None; c.parent = None; c.audio.mute = true;
                ss.project.tracks[ti].clips.push(std::sync::Arc::new(c)); (ti, 0, true) } };
            let c = ss.project.tracks[fti].cm(fci); c.effects = effects;
            for (k, v) in [("fit", 1.0), ("scale", zoom), ("x", 0.0), ("y", 0.0), ("rotation", 0.0), ("opacity", 1.0)] { c.props.insert(k.into(), Prop::new(v)); }
            let fid = c.id.clone(); let tid = ss.project.tracks[fti].id.clone();
            ss.project.find_clip_mut(&cid).unwrap().props.insert("fit".into(), Prop::new(0.0));
            Ok(json!({"fill_clip_id": fid, "track_id": tid, "created": created, "blur": blur, "dim": dim, "zoom": zoom})) } },
] }
