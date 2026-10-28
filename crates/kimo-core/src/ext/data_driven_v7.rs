//! Data-driven animation, essential properties, responsive time and scripting hooks (FEATURES.md L239:
//! "responsive time / presets, essential properties, data-driven animation (JSON/CSV), scripting hooks").
//! "Presets" is covered by the pre-existing apply_motion_preset (After Effects / CapCut animation
//! vocabulary baked to dense keyframes) -- not duplicated here.
//!
//! animate_from_data: read a JSON array or a CSV table (inline string, or a path on disk, same convention
//! as import_media) and write real keyframes onto a clip's transform property or effect param -- one key
//! per row, either at each row's own time column (time_field, or an auto-detected 't'/'time' column) or
//! evenly spaced across a span when no time column is given or found.
//!
//! set_essential_properties / list_essential_properties / clear_essential_properties: mark which of a
//! clip's properties are the "knobs" an external caller (a MOGRT-style template consumer) should expose,
//! reusing the clip's existing tags mechanism (same pattern as names.rs's `color:<name>` label) rather than
//! a new project-format field -- zero risk to old project files.
//!
//! apply_responsive_time: retime a clip to a new duration while protecting `protect_in` seconds at the
//! start and `protect_out` seconds at the end from the stretch (After Effects "Responsive Design - Time"):
//! keyframes inside a protected zone keep their time untouched (the protect_out zone slides to stay flush
//! with the new end), the zone between them is scaled proportionally to fill the new middle span. With
//! protect_in = protect_out = 0 this is a plain uniform retime of every keyframe on the clip.
//!
//! register_hook / list_hooks / remove_hook / run_hooks: named lifecycle events, each a list of
//! template_apply calls (Project.hooks) that run when run_hooks(event) is invoked. Honest scope: these
//! fire when explicitly called, not automatically from the render or export pipeline -- wiring an
//! automatic call into render.rs/export_job.rs was judged too risky for this pass (those paths carry this
//! project's perf and determinism guarantees) and is left as a documented gap.
use super::kit::*;
use crate::keyframe::{Ease, Prop};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HookEntry { pub template: String, #[serde(default)] pub args: Value }

fn resolve_clip_id(ss: &Session, a: &Value) -> Result<String, String> {
    crate::ext::names::resolve_ref(&ss.project, "clip", str_arg(a, "clip_id")?)
}

// ---------------------------------------------------------------------------------------------
// Data-driven animation
// ---------------------------------------------------------------------------------------------

/// A tiny RFC4180-ish CSV reader: comma-separated, double-quoted fields may contain commas/newlines/escaped
/// `""`, CRLF or LF line endings. No dependency pulled in for this -- the project has no csv crate today and
/// the data this feature reads (a handful of numeric columns) does not need a general parser.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = vec![]; let mut row = vec![]; let mut field = String::new();
    let mut chars = text.chars().peekable(); let mut in_quotes = false; let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if in_quotes {
            if c == '"' { if chars.peek() == Some(&'"') { chars.next(); field.push('"'); } else { in_quotes = false; } }
            else { field.push(c); }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => { row.push(std::mem::take(&mut field)); }
                '\r' => {}
                '\n' => { row.push(std::mem::take(&mut field)); rows.push(std::mem::take(&mut row)); }
                _ => field.push(c),
            }
        }
    }
    if any && (!field.is_empty() || !row.is_empty()) { row.push(field); rows.push(row); }
    rows
}

/// Rows as a list of {field: value} JSON objects, from whichever source was given.
fn load_rows(a: &Value) -> Result<Vec<serde_json::Map<String, Value>>, String> {
    let given: Vec<&str> = ["data", "csv", "csv_path", "json_path"].iter().copied().filter(|k| a.get(*k).is_some_and(|v| !v.is_null())).collect();
    if given.len() != 1 { return Err(format!("give exactly one of data, csv, csv_path, json_path (got {})", if given.is_empty() { "none".into() } else { given.join(", ") })); }
    let has_header = a.get("has_header").and_then(|v| v.as_bool()).unwrap_or(true);
    let from_csv_text = |text: &str| -> Result<Vec<serde_json::Map<String, Value>>, String> {
        let table = parse_csv(text);
        if table.is_empty() { return Ok(vec![]); }
        let (header, body): (Vec<String>, &[Vec<String>]) = if has_header { (table[0].clone(), &table[1..]) }
            else { ((0..table[0].len()).map(|i| i.to_string()).collect(), &table[..]) };
        Ok(body.iter().map(|r| { let mut m = serde_json::Map::new();
            for (i, h) in header.iter().enumerate() { let raw = r.get(i).cloned().unwrap_or_default();
                let v = raw.trim().parse::<f64>().map(Value::from).unwrap_or(Value::String(raw));
                m.insert(h.clone(), v); } m }).collect())
    };
    let from_json_value = |v: &Value| -> Result<Vec<serde_json::Map<String, Value>>, String> {
        let arr = v.as_array().ok_or("data/json_path content must be a JSON array")?;
        let fields: Option<Vec<String>> = a.get("fields").and_then(|f| f.as_array()).map(|f| f.iter().filter_map(|x| x.as_str().map(String::from)).collect());
        arr.iter().map(|row| match row {
            Value::Object(m) => Ok(m.clone()),
            Value::Array(cells) => { let fs = fields.clone().ok_or("array-of-arrays data needs a `fields` list naming each column")?;
                Ok(fs.iter().cloned().zip(cells.iter().cloned()).collect()) }
            _ => Err("each data row must be an object or an array (with `fields`)".to_string()),
        }).collect()
    };
    match given[0] {
        "data" => from_json_value(a.get("data").unwrap()),
        "csv" => from_csv_text(str_arg(a, "csv")?),
        "csv_path" => { let p = str_arg(a, "csv_path")?; let text = std::fs::read_to_string(p).map_err(|e| format!("csv_path '{p}': {e}"))?; from_csv_text(&text) }
        "json_path" => { let p = str_arg(a, "json_path")?; let text = std::fs::read_to_string(p).map_err(|e| format!("json_path '{p}': {e}"))?;
            let v: Value = serde_json::from_str(&text).map_err(|e| format!("json_path '{p}': invalid JSON: {e}"))?; from_json_value(&v) }
        _ => unreachable!(),
    }
}

const CLIP_PROP_NAMES: &[&str] = &["opacity", "x", "y", "scale", "rotation", "rot_x", "rot_y", "pos_z", "backface", "fill_alpha", "stroke_alpha", "gradient_angle", "flow_blur", "flow_quality", "pin_gain", "reveal", "path"];

fn animate_from_data(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let cid = resolve_clip_id(ss, a)?;
    let prop_name = str_arg(a, "property")?.to_string();
    let effect_id = a.get("effect_id").and_then(|v| v.as_str()).map(String::from);
    if effect_id.is_none() && !CLIP_PROP_NAMES.contains(&prop_name.as_str()) {
        return Err(format!("unknown clip property '{prop_name}': nothing reads it. Valid: {}. For an effect param pass effect_id.", CLIP_PROP_NAMES.join(", ")));
    }
    let rows = load_rows(a)?;
    if rows.is_empty() { return Err("no rows in the given data source".into()); }
    let time_field_owned: Option<String> = match a.get("time_field").and_then(|v| v.as_str()) {
        Some(s) => Some(s.to_string()),
        None => ["t", "time"].iter().find(|k| rows[0].contains_key(**k)).map(|s| s.to_string()),
    };
    let time_field = time_field_owned.as_deref();
    let value_field = a.get("value_field").and_then(|v| v.as_str()).unwrap_or("v");
    let scale = a.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let offset = a.get("offset").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let ease: Ease = serde_json::from_value(a.get("ease").cloned().unwrap_or(json!("linear"))).map_err(|e| e.to_string())?;
    let clear_existing = a.get("clear_existing").and_then(|v| v.as_bool()).unwrap_or(true);

    let clip_duration = ss.project.find_clip(&cid).ok_or("no such clip")?.duration;
    let start = a.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let span = a.get("duration").and_then(|v| v.as_f64()).unwrap_or((clip_duration - start).max(0.0));

    // Resolve (time, value) per row before touching the project, so a bad row fails loud with nothing written.
    let mut points: Vec<(f64, f64)> = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let raw_v = row.get(value_field).ok_or_else(|| format!("row {i} has no '{value_field}' field"))?;
        let v = raw_v.as_f64().ok_or_else(|| format!("row {i}'s '{value_field}' is not a number: {raw_v}"))?;
        let t = match time_field.and_then(|tf| row.get(tf)) {
            Some(tv) => tv.as_f64().ok_or_else(|| format!("row {i}'s '{}' is not a number: {tv}", time_field.unwrap()))?,
            None => if rows.len() == 1 { start } else { start + span * (i as f64) / (rows.len() - 1) as f64 },
        };
        if !t.is_finite() || !v.is_finite() { return Err(format!("row {i} has a non-finite time or value")); }
        points.push((t, v * scale + offset));
    }
    let default_v = points[0].1;

    let c = ss.project.find_clip_mut(&cid).ok_or("no such clip")?;
    let target: &mut Prop = match &effect_id {
        Some(id) => { let e = c.effects.iter_mut().find(|e| e.id == *id).ok_or("no such effect")?;
            if clear_existing { e.params.insert(prop_name.clone(), Prop::new(default_v)); }
            e.params.entry(prop_name.clone()).or_insert_with(|| Prop::new(default_v)) }
        None => { if clear_existing { c.props.insert(prop_name.clone(), Prop::new(default_v)); }
            c.props.entry(prop_name.clone()).or_insert_with(|| Prop::new(default_v)) }
    };
    for (t, v) in &points { target.set_key(*t, *v, ease); }
    Ok(json!({"keys": target.keys.len(), "rows_used": points.len(), "time_range": [points.first().map(|p| p.0), points.last().map(|p| p.0)]}))
}

// ---------------------------------------------------------------------------------------------
// Essential properties (which knobs a MOGRT-style template consumer should expose)
// ---------------------------------------------------------------------------------------------

/// An essential-property record encoded as one reserved tag: `essential:<effect_id.>property[=label]`.
fn essential_tag(property: &str, effect_id: Option<&str>, label: Option<&str>) -> String {
    let target = match effect_id { Some(e) => format!("{e}.{property}"), None => property.to_string() };
    match label { Some(l) if !l.is_empty() => format!("essential:{target}={l}"), _ => format!("essential:{target}") }
}
fn parse_essential_tag(tag: &str) -> Option<Value> {
    let rest = tag.strip_prefix("essential:")?;
    let (target, label) = match rest.split_once('=') { Some((t, l)) => (t, Some(l)), None => (rest, None) };
    let (effect_id, property) = match target.split_once('.') { Some((e, p)) => (Some(e), p), None => (None, target) };
    Some(json!({"property": property, "effect_id": effect_id, "label": label}))
}

fn set_essential_properties(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let cid = resolve_clip_id(ss, a)?;
    let props = a.get("properties").and_then(|v| v.as_array()).ok_or("properties must be an array of {property, effect_id?, label?}")?;
    let replace = a.get("replace").and_then(|v| v.as_bool()).unwrap_or(true);
    let mut new_tags = vec![];
    for p in props {
        let property = p.get("property").and_then(|v| v.as_str()).ok_or("each entry needs a property name")?;
        let effect_id = p.get("effect_id").and_then(|v| v.as_str());
        let label = p.get("label").and_then(|v| v.as_str());
        new_tags.push(essential_tag(property, effect_id, label));
    }
    let c = ss.project.find_clip_mut(&cid).ok_or("no such clip")?;
    if replace { c.tags.retain(|t| !t.starts_with("essential:")); }
    for t in new_tags { if !c.tags.contains(&t) { c.tags.push(t); } }
    let listed: Vec<Value> = c.tags.iter().filter_map(|t| parse_essential_tag(t)).collect();
    Ok(json!({"essential_properties": listed}))
}
fn list_essential_properties(ss: &mut Session, a: &Value) -> Result<Value, String> {
    if let Some(cid_arg) = a.get("clip_id").and_then(|v| v.as_str()) {
        let cid = crate::ext::names::resolve_ref(&ss.project, "clip", cid_arg)?;
        let c = ss.project.find_clip(&cid).ok_or("no such clip")?;
        return Ok(json!({"clip_id": cid, "essential_properties": c.tags.iter().filter_map(|t| parse_essential_tag(t)).collect::<Vec<_>>()}));
    }
    let mut out = vec![];
    for t in &ss.project.tracks { for c in &t.clips { let es: Vec<Value> = c.tags.iter().filter_map(|t| parse_essential_tag(t)).collect();
        if !es.is_empty() { out.push(json!({"clip_id": c.id, "essential_properties": es})); } } }
    Ok(json!({"clips": out}))
}
fn clear_essential_properties(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let cid = resolve_clip_id(ss, a)?;
    let c = ss.project.find_clip_mut(&cid).ok_or("no such clip")?;
    let before = c.tags.len(); c.tags.retain(|t| !t.starts_with("essential:"));
    Ok(json!({"removed": before - c.tags.len()}))
}

// ---------------------------------------------------------------------------------------------
// Responsive time
// ---------------------------------------------------------------------------------------------

/// Map an old-duration time to its new-duration time under a 3-zone protect_in/protect_out retime.
fn remap_time(t: f64, old_dur: f64, new_dur: f64, protect_in: f64, protect_out: f64) -> f64 {
    let old_mid_end = (old_dur - protect_out).max(protect_in);
    let new_mid_end = (new_dur - protect_out).max(protect_in);
    if t <= protect_in { t }
    else if t >= old_mid_end { new_dur - (old_dur - t) }
    else { let old_mid_len = (old_mid_end - protect_in).max(1e-9); let new_mid_len = (new_mid_end - protect_in).max(0.0);
        protect_in + (t - protect_in) / old_mid_len * new_mid_len }
}
fn retime_prop(p: &mut Prop, old_dur: f64, new_dur: f64, protect_in: f64, protect_out: f64) {
    for k in &mut p.keys { k.t = remap_time(k.t, old_dur, new_dur, protect_in, protect_out); }
    p.keys.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
}
fn apply_responsive_time(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let cid = resolve_clip_id(ss, a)?;
    let new_duration = num_arg(a, "new_duration")?;
    if new_duration <= 0.0 { return Err("new_duration must be > 0".into()); }
    let protect_in = opt_num(a, "protect_in", 0.0).max(0.0);
    let protect_out = opt_num(a, "protect_out", 0.0).max(0.0);
    let c = ss.project.find_clip_mut(&cid).ok_or("no such clip")?;
    let old_duration = c.duration;
    if protect_in + protect_out > old_duration.min(new_duration) + 1e-9 {
        return Err(format!("protect_in + protect_out ({}) exceeds the shorter of the old ({old_duration}) and new ({new_duration}) duration", protect_in + protect_out));
    }
    let mut keys_touched = 0usize;
    for p in c.props.values_mut() { keys_touched += p.keys.len(); retime_prop(p, old_duration, new_duration, protect_in, protect_out); }
    for e in c.effects.iter_mut() { for p in e.params.values_mut() { keys_touched += p.keys.len(); retime_prop(p, old_duration, new_duration, protect_in, protect_out); } }
    c.duration = new_duration;
    Ok(json!({"old_duration": old_duration, "new_duration": new_duration, "protect_in": protect_in, "protect_out": protect_out, "keyframes_remapped": keys_touched}))
}

// ---------------------------------------------------------------------------------------------
// Scripting hooks
// ---------------------------------------------------------------------------------------------

fn register_hook(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let event = str_arg(a, "event")?.to_string();
    let template = str_arg(a, "template")?.to_string();
    let args = a.get("args").cloned().unwrap_or(json!({}));
    ss.project.hooks.entry(event.clone()).or_default().push(crate::ext::data_driven_v7::HookEntry { template, args });
    Ok(json!({"event": event, "count": ss.project.hooks.get(&event).map(|v| v.len()).unwrap_or(0)}))
}
fn list_hooks(ss: &mut Session, a: &Value) -> Result<Value, String> {
    match a.get("event").and_then(|v| v.as_str()) {
        Some(ev) => Ok(json!({"event": ev, "hooks": ss.project.hooks.get(ev).cloned().unwrap_or_default()})),
        None => Ok(json!({"hooks": ss.project.hooks})),
    }
}
fn remove_hook(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let event = str_arg(a, "event")?; let index = num_arg(a, "index")? as usize;
    let list = ss.project.hooks.get_mut(event).ok_or_else(|| format!("no hooks registered for event '{event}'"))?;
    if index >= list.len() { return Err(format!("index {index} out of range (event '{event}' has {} hooks)", list.len())); }
    list.remove(index);
    Ok(json!({"event": event, "remaining": list.len()}))
}
fn run_hooks(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let event = str_arg(a, "event")?.to_string();
    let hooks = ss.project.hooks.get(&event).cloned().unwrap_or_default();
    let mut results = vec![];
    for h in &hooks {
        let r = crate::commands::dispatch_nested(ss, "template_apply", &json!({"name": h.template, "args": h.args}));
        results.push(match r { Ok(v) => json!({"template": h.template, "ok": true, "result": v}), Err(e) => json!({"template": h.template, "ok": false, "error": e}) });
    }
    Ok(json!({"event": event, "ran": results.len(), "results": results}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "animate_from_data", description: "Data-driven animation (FEATURES.md L239): keyframe a clip's transform property or effect param from a JSON array or CSV table, one key per row. Exactly one source: data (array of row objects or row arrays + `fields`), csv (inline text), csv_path or json_path (a file on disk). value_field names the value column (default 'v'); time_field names an explicit per-row time column, else rows are spaced evenly across [start, start+duration]. scale/offset apply value*scale+offset. clear_existing (default true) replaces the property's prior keys.", mutates: true,
        schema: obj(vec![("clip_id", s("clip")), ("property", s("transform property, or an effect param name with effect_id")), ("effect_id", s("optional: target an effect's param instead of a transform property")),
            ("data", json!({"description":"inline JSON array: row objects, or row arrays paired with `fields`"})), ("csv", s("inline CSV text")), ("csv_path", s("path to a CSV file")), ("json_path", s("path to a JSON file (same shape as data)")),
            ("fields", json!({"type":"array","items":{"type":"string"},"description":"column names, required when a data/json_path row is an array"})), ("has_header", json!({"type":"boolean","description":"CSV only; default true"})),
            ("time_field", s("optional: row field holding clip-local seconds; auto-detected from a 't' or 'time' column when omitted")), ("value_field", s("row field holding the value; default 'v'")),
            ("start", n("clip-local start when time_field is absent; default 0")), ("duration", n("span to spread rows across when time_field is absent; default the clip's own duration")),
            ("scale", n("value*scale+offset; default 1")), ("offset", n("default 0")), ("ease", json!({"type":"string","enum":crate::keyframe::EASE_NAMES})), ("clear_existing", json!({"type":"boolean","description":"default true: replace the property's existing keys"}))],
            &["clip_id","property"]),
        run: animate_from_data },
    Command { name: "set_essential_properties", description: "Essential properties (FEATURES.md L239): mark which of a clip's properties (transform or effect params) are the knobs an external caller/MOGRT-style template consumer should expose. Stored as reserved `essential:` tags (set_meta's tags also see these). replace (default true) clears the clip's prior essential marks first.", mutates: true,
        schema: obj(vec![("clip_id", s("clip")), ("properties", json!({"type":"array","items":{"type":"object","properties":{"property":{"type":"string"},"effect_id":{"type":"string"},"label":{"type":"string"}},"required":["property"]}})), ("replace", json!({"type":"boolean"}))], &["clip_id","properties"]),
        run: set_essential_properties },
    Command { name: "list_essential_properties", description: "List a clip's essential properties, or every clip in the project that has any when clip_id is omitted.", mutates: false,
        schema: obj(vec![("clip_id", s("optional: one clip; omit for the whole project"))], &[]), run: list_essential_properties },
    Command { name: "clear_essential_properties", description: "Remove all essential-property marks from a clip.", mutates: true,
        schema: obj(vec![("clip_id", s("clip"))], &["clip_id"]), run: clear_essential_properties },
    Command { name: "apply_responsive_time", description: "Responsive time (FEATURES.md L239, After Effects 'Responsive Design - Time'): retime a clip to new_duration while protecting protect_in seconds at the start and protect_out seconds at the end from the stretch -- keyframes in a protected zone keep their time (the end zone slides to stay flush with the new end), the zone between is scaled proportionally. protect_in = protect_out = 0 (the default) is a plain uniform retime of every property and effect-param keyframe on the clip; the clip's own duration is updated to match. Explicit call, not auto-triggered by trim_clip/set_speed.", mutates: true,
        schema: obj(vec![("clip_id", s("clip")), ("new_duration", n("seconds, > 0")), ("protect_in", n("seconds protected at the start; default 0")), ("protect_out", n("seconds protected at the end; default 0"))], &["clip_id","new_duration"]),
        run: apply_responsive_time },
    Command { name: "register_hook", description: "Scripting hooks (FEATURES.md L239): register a template_apply call to run under a named lifecycle event when run_hooks(event) is invoked. Explicit trigger only -- not auto-fired by render or export (see module docs for why).", mutates: true,
        schema: obj(vec![("event", s("event name, e.g. before_render, after_export -- any string")), ("template", s("a template_define'd template name")), ("args", json!({"description":"args object passed to template_apply"}))], &["event","template"]),
        run: register_hook },
    Command { name: "list_hooks", description: "List registered scripting hooks, for one event or all of them.", mutates: false,
        schema: obj(vec![("event", s("optional: one event name"))], &[]), run: list_hooks },
    Command { name: "remove_hook", description: "Remove one registered hook by its index within its event's list (see list_hooks).", mutates: true,
        schema: obj(vec![("event", s("event name")), ("index", n("0-based index within that event's hook list"))], &["event","index"]), run: remove_hook },
    Command { name: "run_hooks", description: "Run every hook registered under an event, in registration order, each as a template_apply call. Returns each hook's own ok/error; one hook failing does not stop the rest.", mutates: true,
        schema: obj(vec![("event", s("event name"))], &["event"]), run: run_hooks },
] }
