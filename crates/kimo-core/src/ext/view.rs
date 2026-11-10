//! N1: read-only layer isolation. `isolate` / `hide` (clip ids or "@name") change what ONE render-type call draws; the project is never touched.
//! The call runs on a filtered copy of the project with private frame caches, then the real project and caches are restored (also on error).
//! isolate keeps only the listed clips; hide removes the listed clips (applied after isolate). The copy is the same project with the other
//! clips deleted, so the result is bit-equal to rendering that project. Documented rule for parents: an isolated child keeps its resolved world
//! transform (its parent chain stays in the copy as undrawn ghosts), and hiding a parent removes only that clip's own pixels, not its children's.
use super::kit::*;
use crate::commands::Session;

pub fn list_schema(what: &str) -> Value { json!({"type":"array","items":{"type":"string"},"description":what}) }
pub const ISOLATE_DOC: &str = "optional clip ids or @names: draw ONLY these clips (render-time override, the project is not changed)";
pub const HIDE_DOC: &str = "optional clip ids or @names: draw everything EXCEPT these clips (render-time override, the project is not changed; applied after isolate)";

fn refs(ss: &Session, a: &Value, k: &str) -> Result<Option<Vec<String>>, String> {
    let Some(v) = a.get(k).filter(|v| !v.is_null()) else { return Ok(None) };
    let arr = v.as_array().ok_or_else(|| format!("{k} must be an array of clip ids or @names"))?;
    if arr.is_empty() { return Err(format!("{k} must list at least one clip (an empty isolate would draw nothing; omit {k} to draw everything)")); }
    let mut out = vec![]; for x in arr { let r = x.as_str().ok_or_else(|| format!("{k} must be an array of clip ids or @names"))?; let id = super::names::resolve_ref(&ss.project, "clip", r)?; if ss.project.find_clip(&id).is_none() { return Err(format!("no clip '{id}' in {k}")); } out.push(id); }
    Ok(Some(out))
}
/// The project as a render-type call should see it. Pure: never mutates `p`.
/// D = drawn clips (isolate, or all, minus hide). A clip outside D is deleted, EXCEPT an ancestor of a drawn clip (A5 parent chain): it stays as a
/// ghost, so the child's resolved world transform is unchanged, but it draws nothing (invisible 1x1 shape, no effects/mask/text/transition; its own
/// x/y/scale/rotation/opacity and parent link are kept because children compose with them).
pub fn view_of(p: &crate::model::Project, isolate: Option<&[String]>, hide: Option<&[String]>) -> crate::model::Project {
    let drawn = |c: &crate::model::Clip| isolate.map_or(true, |i| i.contains(&c.id)) && !hide.map_or(false, |h| h.contains(&c.id));
    let mut ghosts: std::collections::HashSet<String> = Default::default();
    for t in &p.tracks { for c in &t.clips { if drawn(c) { let mut cur = c.parent.clone(); for _ in 0..64 { let Some(id) = cur else { break }; if !ghosts.insert(id.clone()) { break; } cur = p.find_clip(&id).and_then(|x| x.parent.clone()); } } } }
    let mut v = p.clone();
    for t in v.tracks.iter_mut() {
        t.clips.retain(|c| drawn(c) || ghosts.contains(&c.id));
        for c in t.clips.iter_mut() { if !drawn(c) { let m = std::sync::Arc::make_mut(c); m.source = crate::model::Source::Shape { shape: "rect".into(), width: 1.0, height: 1.0, fill: [0.0; 3], stroke_width: 0.0, stroke_color: [0.0; 3], sides: 4, corner_radius: 0.0 };
            for k in ["fill_alpha", "stroke_alpha"] { m.props.insert(k.into(), crate::keyframe::Prop::new(0.0)); } m.effects.clear(); m.mask = None; m.text_fx = None; m.transition_in = None; } }
    }
    v
}
pub fn with_view(ss: &mut Session, a: &Value, f: impl FnOnce(&mut Session) -> Result<Value, String>) -> Result<Value, String> {
    let (iso, hid) = (refs(ss, a, "isolate")?, refs(ss, a, "hide")?);
    if iso.is_none() && hid.is_none() { return f(ss); }
    let view = view_of(&ss.project, iso.as_deref(), hid.as_deref());
    let real = std::mem::replace(&mut ss.project, view);
    let cache = std::mem::replace(&mut ss.cache, crate::render::Cache::new());
    let fcache = std::mem::replace(&mut ss.fcache, crate::framecache::FrameCache::new(crate::framecache::default_cap()));
    let mut out = f(ss);
    ss.project = real; ss.cache = cache; ss.fcache = fcache;
    if let Ok(o) = out.as_mut() { if let Some(m) = o.as_object_mut() { m.insert("view".into(), json!({"isolate": iso, "hide": hid, "note": "render-time override; the project was not changed"})); } }
    out
}
pub fn commands() -> Vec<Command> { vec![] }
