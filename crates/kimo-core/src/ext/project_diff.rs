//! N1.2: project_diff - what changed between two revisions of a project. Read-only: nothing is loaded into the session, no undo entry.
//! A revision is "current", "undo:<n>" (n steps back in undo history; undo:1 is the state before the last change), "checkpoint:<name>",
//! or "file:<path.kimo|.json>". Clips are matched by id; every field comparison is on the serialized clip, so nothing is skipped.
use super::kit::*;
use crate::commands::Session;
use crate::model::Project;

pub fn resolve_rev(ss: &Session, r: &str) -> Result<Project, String> {
    if r == "current" { return Ok(ss.project.clone()); }
    if let Some(n) = r.strip_prefix("undo:") { let n: usize = n.parse().ok().filter(|n| *n >= 1).ok_or("undo:<n> needs n >= 1 (undo:1 = the state before the last change)")?; return ss.undo.len().checked_sub(n).and_then(|i| ss.undo.get(i)).cloned().ok_or_else(|| format!("undo:{n} is out of range: {} undo entries exist", ss.undo.len())); }
    if let Some(n) = r.strip_prefix("checkpoint:") { return ss.checkpoints.iter().find(|c| c.0 == n).map(|c| c.1.clone()).ok_or_else(|| format!("no checkpoint '{n}'. Existing: {}", ss.checkpoints.iter().map(|c| c.0.as_str()).collect::<Vec<_>>().join(", "))); }
    if let Some(pth) = r.strip_prefix("file:") { let t = std::fs::read_to_string(pth).map_err(|e| format!("cannot read {pth}: {e}"))?; return serde_json::from_str(&t).map_err(|e| format!("{pth} is not a project file: {e}")); }
    Err(format!("revision '{r}' is not allowed. Valid: current, undo:<n>, checkpoint:<name>, file:<path.kimo>"))
}
fn rev_arg(ss: &Session, a: &Value, k: &str) -> Result<Project, String> { resolve_rev(ss, a.get(k).and_then(|v| v.as_str()).ok_or_else(|| format!("{k} must be a revision string: current, undo:<n>, checkpoint:<name> or file:<path.kimo>"))?) }

/// Field-level differences between two JSON values as (path, before, after); arrays report their length change and then differing elements.
fn jdiff(path: &str, a: &Value, b: &Value, out: &mut Vec<(String, Value, Value)>, cap: usize) {
    if a == b || out.len() >= cap { return; }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => { let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect(); keys.sort(); keys.dedup(); for k in keys { jdiff(&format!("{path}{}{k}", if path.is_empty() { "" } else { "." }), x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), out, cap); } }
        (Value::Array(x), Value::Array(y)) => { if x.len() != y.len() { out.push((format!("{path}.len"), json!(x.len()), json!(y.len()))); } for i in 0..x.len().min(y.len()) { jdiff(&format!("{path}[{i}]"), &x[i], &y[i], out, cap); } }
        _ => out.push((path.to_string(), a.clone(), b.clone())),
    }
}
fn clips(p: &Project) -> Vec<(String, String, Value)> { p.tracks.iter().flat_map(|t| t.clips.iter().map(move |c| (c.id.clone(), t.id.clone(), serde_json::to_value(&**c).unwrap_or(Value::Null)))).collect() }
fn brief(id: &str, track: &str, v: &Value) -> Value { json!({"id": id, "name": v.get("name"), "track": track, "start": v.get("start"), "end": v["start"].as_f64().zip(v["duration"].as_f64()).map(|(s, d)| s + d), "source_type": v["source"].get("type")}) }
fn project_diff(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let (pa, pb) = (rev_arg(ss, a, "a")?, rev_arg(ss, a, "b")?);
    let limit = match a.get("limit") { None | Some(Value::Null) => 100, Some(v) => v.as_u64().filter(|n| (1..=500).contains(n)).ok_or("limit must be an integer 1..500")? as usize };
    let (ca, cb) = (clips(&pa), clips(&pb));
    let (mut added, mut removed, mut changed) = (vec![], vec![], vec![]); let (mut n_added, mut n_removed, mut n_changed) = (0, 0, 0);
    for (id, tr, v) in &cb { if !ca.iter().any(|x| x.0 == *id) { n_added += 1; if added.len() < limit { added.push(brief(id, tr, v)); } } }
    for (id, tr, v) in &ca { match cb.iter().find(|x| x.0 == *id) { None => { n_removed += 1; if removed.len() < limit { removed.push(brief(id, tr, v)); } }
        Some((_, tr2, v2)) => { let mut d = vec![]; jdiff("", v, v2, &mut d, 60); if tr != tr2 { d.push(("track".into(), json!(tr), json!(tr2))); } if !d.is_empty() { n_changed += 1; if changed.len() < limit { changed.push(json!({"id": id, "name": v2.get("name"), "changes": d.iter().map(|(f, x, y)| json!({"field": f, "before": x, "after": y})).collect::<Vec<_>>(), "changes_truncated": d.len() >= 60})); } } } } }
    let tr = |p: &Project| -> Vec<(String, String)> { p.tracks.iter().map(|t| (t.id.clone(), t.name.clone())).collect() }; let (ta, tb) = (tr(&pa), tr(&pb));
    let tracks = json!({"added": tb.iter().filter(|t| !ta.iter().any(|x| x.0 == t.0)).map(|t| json!({"id": t.0, "name": t.1})).collect::<Vec<_>>(), "removed": ta.iter().filter(|t| !tb.iter().any(|x| x.0 == t.0)).map(|t| json!({"id": t.0, "name": t.1})).collect::<Vec<_>>(),
        "renamed": ta.iter().filter_map(|t| tb.iter().find(|x| x.0 == t.0 && x.1 != t.1).map(|x| json!({"id": t.0, "before": t.1, "after": x.1}))).collect::<Vec<_>>()});
    // clip order inside a track (stacking order) among the clips both revisions share
    let order = |p: &Project, tid: &str, other: &Project| -> Vec<String> { p.tracks.iter().find(|t| t.id == tid).map(|t| t.clips.iter().filter(|c| other.find_clip(&c.id).is_some()).map(|c| c.id.clone()).collect()).unwrap_or_default() };
    let reordered: Vec<Value> = pa.tracks.iter().filter(|t| pb.tracks.iter().any(|x| x.id == t.id)).filter_map(|t| { let (x, y) = (order(&pa, &t.id, &pb), order(&pb, &t.id, &pa)); let (mut xs, mut ys) = (x.clone(), y.clone()); xs.sort(); ys.sort(); if x != y && xs == ys { Some(json!({"track": t.id, "before": x, "after": y})) } else { None } }).collect();
    let canvas = if (pa.width, pa.height, pa.fps) != (pb.width, pb.height, pb.fps) { json!({"before": [pa.width, pa.height, pa.fps], "after": [pb.width, pb.height, pb.fps]}) } else { Value::Null };
    Ok(json!({"added": added, "removed": removed, "changed": changed, "counts": {"added": n_added, "removed": n_removed, "changed": n_changed}, "tracks": tracks, "reordered": reordered, "canvas": canvas, "truncated": n_added > added.len() || n_removed > removed.len() || n_changed > changed.len(), "identical": n_added + n_removed + n_changed == 0 && tracks["added"].as_array().map_or(true, |x| x.is_empty()) && tracks["removed"].as_array().map_or(true, |x| x.is_empty()) && tracks["renamed"].as_array().map_or(true, |x| x.is_empty()) && reordered.is_empty() && canvas.is_null()}))
}
pub fn commands() -> Vec<Command> { vec![
    Command { name: "project_diff", description: "Read-only: list the clips added, removed and changed between two revisions of the project (\"what changed since the previous revision\"). a and b are each current, undo:<n> (undo:1 = the state before the last change), checkpoint:<name>, or file:<path.kimo>. Clips are matched by id; changed clips list every field difference (name, role, tags, start, duration, track, props, key counts and values, effects, parent) as field/before/after, capped at 60 per clip; track adds, removes and renames, clip order changes inside a track (reordered) and canvas changes are reported too. At most limit clips per list (default 100, max 500); counts and truncated give the totals. Nothing is loaded or changed.", mutates: false,
        schema: obj(vec![("a", s("older revision: current | undo:<n> | checkpoint:<name> | file:<path.kimo>")), ("b", s("newer revision, same forms")), ("limit", json!({"type":"integer","minimum":1,"maximum":500}))], &["a", "b"]), run: project_diff },
] }
