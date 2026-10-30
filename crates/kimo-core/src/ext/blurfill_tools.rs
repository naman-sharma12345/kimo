//! add_blur_fill: the vertical-video fill. A clip that does not fill the frame (a landscape shot on a 9:16 canvas) is shown
//! over a blurred, darkened, cover-scaled copy of itself instead of black bars. The copy goes on a new track directly under the
//! clip's track and plays the same frames (same source, in point, speed and timing); it is muted, untransformed and carries a
//! blur and an exposure effect tagged as a fill. The clip itself is set to fit contain. A second call updates the fill; remove
//! deletes it (and its track when that is left empty).
//! The fill is linked to its clip (strings.fill_of on the fill's effects, a "blur_fill:<clip id>" tag on the clip): after every
//! successful edit, sync_fills carries each fill along with its clip's timing (start, duration, in point, speed, retiming,
//! source), keeps it on a track under the clip's, deletes it with its clip, and gives a copy of the clip (the right half of a
//! split, a duplicate) a fill of its own with the same settings.
use super::kit::*;
use crate::keyframe::Prop;
use crate::model::*;

const ARGS: &[&str] = &["clip_id", "blur", "dim", "zoom", "remove"];
fn num(a: &Value, k: &str, d: f64, lo: f64, hi: f64) -> Result<f64, String> { match a.get(k) { None => Ok(d), Some(v) => { let x = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{k} must be a number"))?; if x < lo || x > hi { Err(format!("{k} must be within {lo}..{hi}, got {x}")) } else { Ok(x) } } } }
fn is_fill(e: &Effect) -> bool { e.strings.get("fill").map_or(false, |v| v == "1") }
const TAG: &str = "blur_fill:";
/// The clip a fill is linked to (strings.fill_of on its fill effects).
fn link_of(c: &Clip) -> Option<&str> { c.effects.iter().filter(|e| is_fill(e)).find_map(|e| e.strings.get("fill_of")).map(|s| s.as_str()) }
/// The clip id in a framed clip's tag (its own id, or the id of the clip it was copied from).
fn tag_of(c: &Clip) -> Option<&str> { c.tags.iter().find_map(|t| t.strip_prefix(TAG)) }
fn set_tag(c: &mut Clip, id: Option<&str>) { c.tags.retain(|t| !t.starts_with(TAG)); if let Some(id) = id { c.tags.push(format!("{TAG}{id}")); } }
fn pos(p: &Project, id: &str) -> Option<(usize, usize)> { p.tracks.iter().enumerate().find_map(|(ti, t)| t.clips.iter().position(|c| c.id == id).map(|ci| (ti, ci))) }
/// (track index, clip index) of `fg`'s fill: the fill linked to it, else (a fill made before the link) a clip tagged as a fill
/// with the same source, start, duration and in point.
fn find_fill(p: &Project, fg: &Clip) -> Option<(usize, usize)> {
    for (ti, t) in p.tracks.iter().enumerate() { for (ci, c) in t.clips.iter().enumerate() { if c.id != fg.id && link_of(c) == Some(fg.id.as_str()) { return Some((ti, ci)); } } }
    for (ti, t) in p.tracks.iter().enumerate() { for (ci, c) in t.clips.iter().enumerate() {
        if c.id != fg.id && link_of(c).is_none() && c.effects.iter().any(is_fill) && c.start == fg.start && c.duration == fg.duration && c.in_point == fg.in_point && serde_json::to_string(&c.source).ok() == serde_json::to_string(&fg.source).ok() { return Some((ti, ci)); } } }
    None
}
fn fx(p: &mut Project, kind: &str, k: &str, v: f64, of: &str) -> Effect { let mut e = Effect { id: p.gen_id("fx"), kind: kind.into(), params: Default::default(), strings: Default::default() }; e.params.insert(k.into(), Prop::new(v)); e.strings.insert("fill".into(), "1".into()); e.strings.insert("fill_of".into(), of.into()); e }
/// The timing a fill copies from its clip.
fn timing(c: &Clip) -> Value { json!([c.start, c.duration, c.in_point, c.speed, c.speed_curve, c.time_remap, c.time_mode, c.source, c.posterize_fps, c.speed_blend]) }
fn take_timing(f: &mut Clip, fg: &Clip) { f.start = fg.start; f.duration = fg.duration; f.in_point = fg.in_point; f.speed = fg.speed; f.speed_curve = fg.speed_curve.clone(); f.speed_blend = fg.speed_blend; f.time_remap = fg.time_remap.clone(); f.time_mode = fg.time_mode.clone(); f.source = fg.source.clone(); f.posterize_fps = fg.posterize_fps; }
/// Drop track `ti` when it is an emptied fill track.
fn drop_if_empty(p: &mut Project, ti: usize) { if ti < p.tracks.len() && p.tracks[ti].clips.is_empty() && p.tracks[ti].name == "blur fill" && !p.tracks[ti].locked { p.tracks.remove(ti); } }

/// Keep every linked blur fill with its clip (run after each successful edit, commands::run_core). A no-op without fills.
/// Fills on locked tracks are left as they are.
pub fn sync_fills(p: &mut Project) {
    if !p.tracks.iter().any(|t| t.clips.iter().any(|c| tag_of(c).is_some() || link_of(c).is_some())) { return; }
    // 1. a copy of a framed clip (the right half of a split, a duplicate) carries the tag of the clip it came from: give it a fill
    //    like that clip's, on the same fill track when that is under it; a framed clip whose fill was deleted drops its tag
    let tagged: Vec<(String, String)> = p.tracks.iter().flat_map(|t| t.clips.iter()).filter_map(|c| tag_of(c).map(|o| (c.id.clone(), o.to_string()))).collect();
    for (cid, from) in tagged {
        let has = |p: &Project, id: &str| p.tracks.iter().flat_map(|t| t.clips.iter()).any(|x| link_of(x) == Some(id));
        if has(p, &cid) { if from != cid { if let Some((ti, ci)) = pos(p, &cid) { set_tag(p.tracks[ti].cm(ci), Some(&cid)); } } continue; }
        let tpl = if from != cid { p.tracks.iter().flat_map(|t| t.clips.iter()).find(|x| link_of(x) == Some(from.as_str())).map(|x| (**x).clone()) } else { None };
        let Some((ti, ci)) = pos(p, &cid) else { continue };
        let Some(tpl) = tpl else { if !p.tracks[ti].locked { set_tag(p.tracks[ti].cm(ci), None); } continue };
        let mut nf = tpl.clone(); nf.id = p.gen_id("clip"); for e in nf.effects.iter_mut().filter(|e| is_fill(e)) { e.id = p.gen_id("fx"); e.strings.insert("fill_of".into(), cid.clone()); }
        let fg = (*p.tracks[ti].clips[ci]).clone(); take_timing(&mut nf, &fg);
        let ftrack = pos(p, &tpl.id).map(|x| x.0).filter(|&ft| ft < ti && !p.tracks[ft].locked);
        let ft = match ftrack { Some(ft) => ft, None => { let tid = p.gen_id("track"); p.tracks.insert(ti, Track { id: tid, clips: vec![], locked: false, muted: false, solo: false, name: "blur fill".into() }); ti } };
        p.tracks[ft].clips.push(std::sync::Arc::new(nf));
        if let Some((ti, ci)) = pos(p, &cid) { set_tag(p.tracks[ti].cm(ci), Some(&cid)); }
    }
    // 2. every linked fill follows its clip, stays under it, and goes when it is deleted
    let fills: Vec<(String, String)> = p.tracks.iter().flat_map(|t| t.clips.iter()).filter_map(|c| link_of(c).map(|o| (c.id.clone(), o.to_string()))).collect();
    for (fid, of) in fills {
        let Some((fti, fci)) = pos(p, &fid) else { continue }; if p.tracks[fti].locked { continue; }
        let Some((gti, gci)) = pos(p, &of) else { p.tracks[fti].clips.remove(fci); drop_if_empty(p, fti); continue };
        let fg = p.tracks[gti].clips[gci].clone();
        if timing(&p.tracks[fti].clips[fci]) != timing(&fg) { take_timing(p.tracks[fti].cm(fci), &fg); }
        if fti >= gti { // the clip moved onto or under its fill's track: the fill gets a new track directly under the clip
            let c = p.tracks[fti].clips.remove(fci); let tid = p.gen_id("track"); p.tracks.insert(gti, Track { id: tid, clips: vec![c], locked: false, muted: false, solo: false, name: "blur fill".into() });
            drop_if_empty(p, fti + 1); }
    }
}

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
                let fid = ss.project.tracks[fti].clips[fci].id.clone(); ss.project.tracks[fti].clips.remove(fci); set_tag(ss.project.find_clip_mut(&cid).unwrap(), None);
                let dropped = ss.project.tracks[fti].clips.is_empty() && ss.project.tracks[fti].name == "blur fill"; if dropped { ss.project.tracks.remove(fti); }
                return Ok(json!({"removed": 1, "fill_clip_id": fid, "track_removed": dropped})); }
            let effects = vec![fx(&mut ss.project, "blur", "radius", blur, &cid), fx(&mut ss.project, "exposure", "stops", -dim, &cid)];
            let (fti, fci, created) = match find_fill(&ss.project, &fg) { Some((t, c)) => (t, c, false), None => {
                let tid = ss.project.gen_id("track"); ss.project.tracks.insert(ti, Track { id: tid, clips: vec![], locked: false, muted: false, solo: false, name: "blur fill".into() });
                let mut c = fg.clone(); c.id = ss.project.gen_id("clip"); c.transition_in = None; c.mask = None; c.motion_path = None; c.echo = None; c.text_fx = None; c.corners = None; c.parent = None; c.audio.mute = true;
                ss.project.tracks[ti].clips.push(std::sync::Arc::new(c)); (ti, 0, true) } };
            let c = ss.project.tracks[fti].cm(fci); c.effects = effects;
            for (k, v) in [("fit", 1.0), ("scale", zoom), ("x", 0.0), ("y", 0.0), ("rotation", 0.0), ("opacity", 1.0)] { c.props.insert(k.into(), Prop::new(v)); }
            let fid = c.id.clone(); let tid = ss.project.tracks[fti].id.clone();
            { let me = ss.project.find_clip_mut(&cid).unwrap(); me.props.insert("fit".into(), Prop::new(0.0)); set_tag(me, Some(&cid)); }
            Ok(json!({"fill_clip_id": fid, "track_id": tid, "created": created, "blur": blur, "dim": dim, "zoom": zoom})) } },
] }
