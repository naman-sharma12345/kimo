//! U1: names, roles and tags. `set_meta` writes them, `find` queries them, `resolve_ref` turns "@name" into an id.
//! Names are optional, unique per kind (clip / track), and never required: every tool keeps accepting plain ids.
use super::kit::*;
use crate::commands::Session;

pub const MAX_FIND: usize = 500;
/// Clip colour labels (FEATURES.md L85 "clip labels/colors") -- Resolve/Premiere-style bin organisation colours.
/// Stored as a single reserved `color:<name>` tag (not a new Clip field), so it rides the existing tags
/// mechanism/serialization exactly; `set_meta`'s `color` param and `find`'s `color` field/filter are sugar over it.
pub const COLOR_LABELS: &[&str] = &["red", "orange", "yellow", "green", "cyan", "blue", "purple", "pink", "gray"];
fn check_color(s: &str) -> Result<(), String> { if COLOR_LABELS.contains(&s) { Ok(()) } else { Err(format!("invalid_value: color '{s}' not recognised. Valid: {}", COLOR_LABELS.join(", "))) } }
fn color_tag(c: &str) -> String { format!("color:{c}") }
/// The clip's colour label, if any, read back out of its tags.
pub fn clip_color(tags: &[String]) -> Option<String> { tags.iter().find_map(|t| t.strip_prefix("color:").map(String::from)) }
fn name_ok(s: &str, extra: &str) -> bool { !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c) || extra.contains(c)) }
fn check_name(s: &str) -> Result<(), String> { if name_ok(s, "") { Ok(()) } else { Err(format!("invalid_name: '{s}' must be 1..64 characters from A-Z a-z 0-9 _ . - (no leading @, no spaces)")) } }
fn check_tag(s: &str) -> Result<(), String> { if name_ok(s, ":=") { Ok(()) } else { Err(format!("invalid_name: tag '{s}' must be 1..64 characters from A-Z a-z 0-9 _ . - : =")) } }

fn lev(a: &str, b: &str) -> usize { let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect()); let mut d: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() { let mut prev = d[0]; d[0] = i; for j in 1..=b.len() { let t = d[j]; d[j] = (d[j] + 1).min(d[j - 1] + 1).min(prev + (a[i - 1] != b[j - 1]) as usize); prev = t; } } d[b.len()] }
/// (id, name) pairs of one kind, in timeline order.
fn named(p: &crate::model::Project, kind: &str) -> Vec<(String, String)> {
    if kind == "track" { p.tracks.iter().filter(|t| !t.name.is_empty()).map(|t| (t.id.clone(), t.name.clone())).collect() }
    else { p.tracks.iter().flat_map(|t| t.clips.iter()).filter_map(|c| c.name.as_ref().map(|n| (c.id.clone(), n.clone()))).collect() } }
/// Resolve a reference of `kind` ("clip" or "track"): a plain id passes through unchanged; "@name" becomes the id.
/// Errors: name_not_found (with the nearest names) and ambiguous_name (only possible in projects that predate uniqueness).
pub fn resolve_ref(p: &crate::model::Project, kind: &str, r: &str) -> Result<String, String> {
    let Some(name) = r.strip_prefix('@') else { return Ok(r.to_string()) };
    let all = named(p, kind); let hits: Vec<&(String, String)> = all.iter().filter(|(_, n)| n == name).collect();
    match hits.len() { 1 => Ok(hits[0].0.clone()),
        0 => { let mut near: Vec<(usize, &str)> = all.iter().map(|(_, n)| (lev(n, name), n.as_str())).collect(); near.sort(); near.truncate(3);
            Err(format!("name_not_found: no {kind} named '{name}'. Nearest: {}", if near.is_empty() { format!("(no {kind} has a name yet; set one with set_meta)") } else { near.iter().map(|(_, n)| format!("@{n}")).collect::<Vec<_>>().join(", ") })) }
        _ => Err(format!("ambiguous_name: {} {kind}s are named '{name}' ({}). Use an id.", hits.len(), hits.iter().map(|h| h.0.as_str()).collect::<Vec<_>>().join(", "))) } }

fn str_list(a: &Value, k: &str) -> Result<Option<Vec<String>>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(Value::Array(v)) => v.iter().map(|x| x.as_str().map(String::from).ok_or_else(|| format!("{k} must be an array of strings"))).collect::<Result<Vec<_>, _>>().map(Some), _ => Err(format!("{k} must be an array of strings")) } }
fn set_meta(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let kind = a.get("target").and_then(|v| v.as_str()).unwrap_or("clip"); if !["clip", "track"].contains(&kind) { return Err("target is not allowed. Valid: clip, track".into()); }
    let id = resolve_ref(&ss.project, kind, str_arg(a, "id")?)?;
    // validate everything before touching anything
    let name = match a.get("name") { None => None, Some(Value::Null) => Some(None), Some(Value::String(s)) => { check_name(s)?; Some(Some(s.clone())) } _ => return Err("name must be a string or null (clear)".into()) };
    let role = match a.get("role") { None => None, Some(Value::Null) => Some(None), Some(Value::String(s)) => { check_name(s)?; Some(Some(s.clone())) } _ => return Err("role must be a string or null (clear)".into()) };
    let color = match a.get("color") { None => None, Some(Value::Null) => Some(None), Some(Value::String(s)) => { check_color(s)?; Some(Some(s.clone())) } _ => return Err("color must be a string or null (clear)".into()) };
    let (tags, add, rem) = (str_list(a, "tags")?, str_list(a, "add_tags")?, str_list(a, "remove_tags")?);
    for t in tags.iter().chain(add.iter()).flatten() { check_tag(t)?; }
    if kind == "track" && (role.is_some() || color.is_some() || tags.is_some() || add.is_some() || rem.is_some()) { return Err("tracks only carry a name; role, color and tags are for clips".into()); }
    if let Some(Some(n)) = &name { if named(&ss.project, kind).iter().any(|(i, x)| x == n && *i != id) { return Err(format!("name_taken: '{n}' is already used by another {kind}")); } }
    if kind == "track" { let t = ss.project.tracks.iter_mut().find(|t| t.id == id).ok_or_else(|| format!("no track '{id}'"))?; if let Some(n) = name { t.name = n.unwrap_or_default(); } return Ok(json!({"target": "track", "id": id, "name": t.name})); }
    let c = ss.project.find_clip_mut(&id).ok_or_else(|| format!("no clip '{id}'"))?;
    if let Some(n) = name { c.name = n; } if let Some(r) = role { c.role = r; }
    if let Some(t) = tags { let mut v = t; v.dedup(); c.tags = v; } if let Some(add) = add { for t in add { if !c.tags.contains(&t) { c.tags.push(t); } } } if let Some(rem) = rem { c.tags.retain(|t| !rem.contains(t)); }
    if let Some(col) = color { c.tags.retain(|t| !t.starts_with("color:")); if let Some(col) = col { c.tags.push(color_tag(&col)); } }
    Ok(json!({"target": "clip", "id": id, "name": c.name, "role": c.role, "tags": c.tags, "color": clip_color(&c.tags)}))
}
fn glob(pat: &str, s: &str) -> bool { let (p, t): (Vec<char>, Vec<char>) = (pat.chars().collect(), s.chars().collect()); let (mut pi, mut ti, mut star, mut mark) = (0, 0, usize::MAX, 0);
    while ti < t.len() { if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) { pi += 1; ti += 1; } else if pi < p.len() && p[pi] == '*' { star = pi; mark = ti; pi += 1; } else if star != usize::MAX { pi = star + 1; mark += 1; ti = mark; } else { return false; } }
    while pi < p.len() && p[pi] == '*' { pi += 1; } pi == p.len() }
fn find(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let kind = a.get("kind").and_then(|v| v.as_str()).unwrap_or("clip"); if !["clip", "track"].contains(&kind) { return Err("kind is not allowed. Valid: clip, track".into()); }
    let limit = match a.get("limit") { None | Some(Value::Null) => 100, Some(v) => v.as_u64().filter(|n| (1..=MAX_FIND as u64).contains(n)).ok_or_else(|| format!("limit must be an integer 1..{MAX_FIND}"))? as usize };
    let opt = |k: &str| -> Result<Option<String>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(Value::String(s)) => Ok(Some(s.clone())), _ => Err(format!("{k} must be a string")) } };
    let (name, name_glob, role, tag, track, source_kind, text, color) = (opt("name")?, opt("name_glob")?, opt("role")?, opt("tag")?, opt("track")?, opt("source_kind")?, opt("text_contains")?, opt("color")?);
    if let Some(c) = &color { if !COLOR_LABELS.contains(&c.as_str()) { return Err(format!("invalid_value: color '{c}' not recognised. Valid: {}", COLOR_LABELS.join(", "))); } }
    let range = match a.get("time_range") { None | Some(Value::Null) => None, Some(v) => { let r = v.as_array().filter(|x| x.len() == 2).and_then(|x| Some((x[0].as_f64()?, x[1].as_f64()?))).filter(|(lo, hi)| lo.is_finite() && hi.is_finite() && lo <= hi).ok_or("time_range must be [start, end] seconds with start <= end")?; Some(r) } };
    let track = match track { Some(t) => Some(resolve_ref(&ss.project, "track", &t)?), None => None };
    let mut out = vec![]; let mut total = 0usize;
    if kind == "track" { for t in &ss.project.tracks { if name.as_ref().is_some_and(|n| *n != t.name) || name_glob.as_ref().is_some_and(|g| !glob(g, &t.name)) { continue; } total += 1; if out.len() < limit { out.push(json!({"kind": "track", "id": t.id, "name": t.name, "clips": t.clips.len()})); } } }
    else { for t in &ss.project.tracks { if track.as_ref().is_some_and(|x| *x != t.id) { continue; } for c in &t.clips {
        let cn = c.name.clone().unwrap_or_default(); let sk = serde_json::to_value(&c.source).ok().and_then(|v| v.get("type").and_then(|x| x.as_str().map(String::from))).unwrap_or_default();
        let cc = clip_color(&c.tags);
        if name.as_ref().is_some_and(|n| *n != cn) || name_glob.as_ref().is_some_and(|g| !glob(g, &cn)) || role.as_ref().is_some_and(|r| c.role.as_ref() != Some(r)) || tag.as_ref().is_some_and(|x| !c.tags.contains(x)) || source_kind.as_ref().is_some_and(|k| *k != sk) || color.as_ref().is_some_and(|w| cc.as_ref() != Some(w)) { continue; }
        if let Some((lo, hi)) = range { if c.start + c.duration < lo || c.start > hi { continue; } }
        if let Some(x) = &text { let has = match &c.source { crate::model::Source::Text { text, .. } => text.to_lowercase().contains(&x.to_lowercase()), _ => false }; if !has { continue; } }
        total += 1; if out.len() < limit { out.push(json!({"kind": "clip", "id": c.id, "name": c.name, "role": c.role, "tags": c.tags, "color": cc, "track": t.id, "start": c.start, "end": c.start + c.duration, "source_type": sk})); } } } }
    Ok(json!({"matches": out, "count": out.len(), "total": total, "truncated": total > out.len()}))
}
pub fn commands() -> Vec<Command> { vec![
    Command { name: "set_meta", description: "Give a clip or track a name, and a clip a role, a colour label and tags, so later calls can say \"@name\" instead of an id. target clip (default) or track; id may be an id or @name. name: 1..64 characters of A-Z a-z 0-9 _ . - and unique among clips (or tracks); null clears. role: same characters, free label. color: a bin colour label (red, orange, yellow, green, cyan, blue, purple, pink, gray; null clears) -- stored as a reserved tag, shows up in find's output and color filter. tags replaces the tag list; add_tags / remove_tags edit it (tags may also contain : and =, e.g. lint:allow_bleed; a color:* tag is managed by `color`, not tags). Errors: name_taken, invalid_name, invalid_value, name_not_found, ambiguous_name. Tracks carry only a name. Nothing is required: ids keep working everywhere.", mutates: true,
        schema: obj(vec![("target", json!({"type":"string","enum":["clip","track"]})), ("id", s("clip or track id, or @name")), ("name", json!({"type":["string","null"]})), ("role", json!({"type":["string","null"]})), ("color", json!({"type":["string","null"],"enum":["red","orange","yellow","green","cyan","blue","purple","pink","gray",null],"description":"bin colour label; null clears"})), ("tags", json!({"type":"array","items":{"type":"string"}})), ("add_tags", json!({"type":"array","items":{"type":"string"}})), ("remove_tags", json!({"type":"array","items":{"type":"string"}}))], &["id"]), run: set_meta },
    Command { name: "find", description: "Read-only search for clips (default) or tracks by name, name_glob (* and ?), role, color (bin colour label), tag, track (id or @name), source_kind (the source type, e.g. text, solid, media), time_range [start,end] seconds (overlap) and text_contains (case-insensitive, text clips). Returns up to limit matches (default 100, max 500) with id, name, role, color, tags, track, start, end; total and truncated say whether more matched. Nothing is changed.", mutates: false,
        schema: obj(vec![("kind", json!({"type":"string","enum":["clip","track"]})), ("name", s("exact name")), ("name_glob", s("name pattern with * and ?")), ("role", s("exact role")), ("color", s("bin colour label: red, orange, yellow, green, cyan, blue, purple, pink, gray")), ("tag", s("tag the clip carries")), ("track", s("track id or @name")), ("source_kind", s("source type")), ("time_range", json!({"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2})), ("text_contains", s("substring of a text clip's text")), ("limit", json!({"type":"integer","minimum":1,"maximum":500}))], &[]), run: find },
] }
