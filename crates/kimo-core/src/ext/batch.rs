//! RV-11 `batch`: many tool calls as ONE atomic, undoable step. Thin over `commands::dispatch_nested` (same validation, locks and id allocation as single
//! calls, no per-op history). Phase 1 validates every op without touching the project; phase 2 applies on the live session with a snapshot, so a failure
//! restores project bytes and id counters exactly. Recipes (`recipes`) expand to the same op list and run through `run_ops`.
use super::kit::*;
use serde_json::Map;
use std::collections::{BTreeMap, HashMap};

pub const MAX_OPS: usize = 500;
pub const MAX_OUT_BYTES: usize = 60_000;
const FORBIDDEN: &[&str] = &["batch", "undo", "redo", "restore", "load_project", "save_project", "checkpoint", "checkpoint_delete", "collect_project", "relink_media", "recipe_apply", "template_apply", "proxy_generate"];
const FORBIDDEN_PREFIX: &[&str] = &["export", "render_job", "history_"];
pub fn forbidden(tool: &str) -> bool { FORBIDDEN.contains(&tool) || FORBIDDEN_PREFIX.iter().any(|p| tool.starts_with(p)) }

#[derive(Clone, Debug)]
pub struct Op { pub id: String, pub tool: String, pub args: Value }
#[derive(Clone, Debug)]
pub struct VErr { pub index: usize, pub id: String, pub tool: String, pub code: &'static str, pub message: String, pub allowed: Option<Vec<String>> }
impl VErr { fn json(&self) -> Value { let mut v = json!({"index": self.index, "id": self.id, "tool": self.tool, "code": self.code, "message": self.message}); if let Some(a) = &self.allowed { v["allowed"] = json!(a); } v } }

#[derive(Debug, Clone, PartialEq)]
enum Seg { Key(String), Idx(usize) }
/// `$<op>.<path>` with path = `field`, `field.sub`, `field[0]`, `[0]`. Returns None when the string is not a reference ("$$..." is an escape, handled by the caller).
fn parse_ref(s: &str) -> Result<Option<(String, Vec<Seg>)>, String> {
    if !s.starts_with('$') || s.starts_with("$$") { return Ok(None); }
    let rest = &s[1..]; let dot = rest.find('.').ok_or_else(|| format!("malformed reference '{s}': expected $<op_id>.<field> (write $$ for a literal $)"))?;
    let (id, path) = (&rest[..dot], &rest[dot + 1..]);
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '#') || path.is_empty() { return Err(format!("malformed reference '{s}': expected $<op_id>.<field> (write $$ for a literal $)")); }
    let mut segs = vec![]; for part in path.split('.') {
        let (name, mut tail) = match part.find('[') { Some(i) => (&part[..i], &part[i..]), None => (part, "") };
        if !name.is_empty() { segs.push(Seg::Key(name.to_string())); } else if tail.is_empty() { return Err(format!("malformed reference '{s}': empty path segment")); }
        while !tail.is_empty() { let close = tail.find(']').ok_or_else(|| format!("malformed reference '{s}': unterminated ["))?; let n: usize = tail[1..close].parse().map_err(|_| format!("malformed reference '{s}': bad index"))?; segs.push(Seg::Idx(n)); tail = &tail[close + 1..]; if !tail.is_empty() && !tail.starts_with('[') { return Err(format!("malformed reference '{s}': text after ]")); } }
    }
    Ok(Some((id.to_string(), segs)))
}
fn walk_refs(v: &Value, f: &mut dyn FnMut(&str)) { if super::batch_calc::is_calc(v) { for r in super::batch_calc::refs(v["$calc"].as_str().unwrap_or("")) { f(&r); } return; } match v { Value::String(s) => f(s), Value::Array(a) => a.iter().for_each(|x| walk_refs(x, f)), Value::Object(o) => o.values().for_each(|x| walk_refs(x, f)), _ => {} } }
/// Type-preserving substitution: a whole-string reference becomes the referenced value; "$$x" becomes "$x".
fn resolve(v: &Value, results: &HashMap<String, Value>) -> Result<Value, String> {
    if super::batch_calc::is_calc(v) { let e = v["$calc"].as_str().unwrap_or("");
        let r = super::batch_calc::eval(e, &|n| if n.starts_with('$') { resolve(&Value::String(n.to_string()), results).map(|x| x.as_f64()) } else { Ok(None) })?; return Ok(super::batch_calc::num_json(r)); }
    match v {
        Value::String(s) => match parse_ref(s)? { None => Ok(if s.starts_with("$$") { Value::String(s[1..].to_string()) } else { v.clone() }),
            Some((id, segs)) => { let mut cur = results.get(&id).ok_or_else(|| format!("reference '{s}': op '{id}' has not produced a result"))?;
                for sg in &segs { cur = match sg { Seg::Key(k) => cur.get(k.as_str()), Seg::Idx(i) => cur.get(*i) }.ok_or_else(|| format!("reference '{s}': no such field in the result of op '{id}'"))?; } Ok(cur.clone()) } },
        Value::Array(a) => a.iter().map(|x| resolve(x, results)).collect::<Result<Vec<_>, _>>().map(Value::Array),
        Value::Object(o) => { let mut m = Map::new(); for (k, x) in o { m.insert(k.clone(), resolve(x, results)?); } Ok(Value::Object(m)) }
        _ => Ok(v.clone()),
    }
}
fn is_ref(v: &Value) -> bool { v.as_str().map_or(false, |s| s.starts_with('$') && !s.starts_with("$$")) }

fn check_args(tool: &str, args: &Value) -> Vec<(&'static str, String, Option<Vec<String>>)> {
    let mut out = vec![]; let Some(c) = crate::commands::registry_ref().iter().find(|c| c.name == tool) else { return out };
    let obj = match args.as_object() { Some(o) => o, None => { out.push(("bad_arg_type", "args must be an object".to_string(), None)); return out } };
    if let Some(req) = c.schema.get("required").and_then(|r| r.as_array()) { for k in req.iter().filter_map(|x| x.as_str()) { if !obj.contains_key(k) || obj[k].is_null() { let ty = c.schema.pointer(&format!("/properties/{k}/type")).and_then(|t| t.as_str()).unwrap_or("value"); out.push(("missing_arg", format!("missing {ty} '{k}'"), None)); } } }
    if let Some(props) = c.schema.get("properties").and_then(|p| p.as_object()) { for (k, v) in obj { let Some(p) = props.get(k) else { continue }; if is_ref(v) || super::batch_calc::is_calc(v) || v.is_null() { continue; }
        if let Some(t) = p.get("type").and_then(|t| t.as_str()) { let ok = match t { "string" => v.is_string(), "number" => v.is_number(), "integer" => v.is_i64() || v.is_u64(), "boolean" => v.is_boolean(), "array" => v.is_array(), "object" => v.is_object(), _ => true }; if !ok { out.push(("bad_arg_type", format!("argument '{k}' must be a {t}"), None)); continue; } }
        if let (Some(en), Some(s)) = (p.get("enum").and_then(|e| e.as_array()), v.as_str()) { let vals: Vec<String> = en.iter().filter_map(|x| x.as_str().map(String::from)).collect(); if !vals.is_empty() && !vals.iter().any(|x| x == s) { out.push(("invalid_value", format!("unknown {k} '{}'. Valid: {}", sanitize(s), vals.join(", ")), Some(vals))); } } } }
    out
}

/// Parses the `ops` argument into `Op`s with positional ids ("1", "2", ...) where none is given.
pub fn parse_ops(a: &Value) -> Result<Vec<Op>, String> {
    let arr = a.get("ops").and_then(|v| v.as_array()).ok_or_else(|| crate::errors::arg_err(a, "ops", "array"))?;
    if arr.is_empty() { return Err("ops must hold at least one op".into()); } if arr.len() > MAX_OPS { return Err(format!("ops holds {} entries; the limit is {MAX_OPS}", arr.len())); }
    arr.iter().enumerate().map(|(i, o)| { let tool = o.get("tool").and_then(|v| v.as_str()).ok_or_else(|| format!("ops[{i}]: missing string 'tool'"))?.to_string();
        let id = match o.get("id") { Some(Value::String(s)) => s.clone(), None | Some(Value::Null) => (i + 1).to_string(), Some(_) => return Err(format!("ops[{i}]: 'id' must be a string")) };
        Ok(Op { id, tool, args: o.get("args").cloned().unwrap_or_else(|| json!({})) }) }).collect()
}

/// Phase 1. Returns every problem found (empty = valid) plus per-op warnings (ignored unknown args). Never touches the project.
pub fn validate(ss: &Session, ops: &[Op]) -> (Vec<VErr>, Vec<Vec<String>>) {
    let mut errs = vec![]; let mut warns: Vec<Vec<String>> = vec![vec![]; ops.len()]; let mut seen: HashMap<&str, usize> = HashMap::new();
    let locked: Vec<&str> = ss.project.tracks.iter().filter(|t| t.locked).map(|t| t.id.as_str()).collect();
    let mut e = |i: usize, o: &Op, code: &'static str, message: String, allowed: Option<Vec<String>>| errs.push(VErr { index: i, id: o.id.clone(), tool: o.tool.clone(), code, message, allowed });
    for (i, o) in ops.iter().enumerate() {
        let idok = !o.id.is_empty() && o.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '#'); if !idok { e(i, o, "invalid_value", format!("op id '{}' must use letters, digits, _ and # only", sanitize(&o.id)), None); }
        if let Some(prev) = seen.insert(o.id.as_str(), i) { e(i, o, "invalid_value", format!("op id '{}' is already used by ops[{prev}]", sanitize(&o.id)), None); }
        let known = crate::commands::registry_ref().iter().any(|c| c.name == o.tool);
        if !known { let names: Vec<String> = crate::commands::registry_ref().iter().map(|c| c.name.to_string()).collect(); e(i, o, "unknown_tool", format!("unknown tool '{}' (nearest: {})", sanitize(&o.tool), crate::errors::nearest(&o.tool, &names, 3).join(", ")), None); continue; }
        if forbidden(&o.tool) { e(i, o, "forbidden_in_batch", format!("tool '{}' cannot run inside a batch (forbidden: {}, export*, render_job*, history_*)", o.tool, FORBIDDEN.join(", ")), None); continue; }
        if o.tool == "project_set" && i != 0 { e(i, o, "forbidden_in_batch", "project_set is only allowed as the first op of a batch".into(), None); continue; }
        for (code, m, allowed) in check_args(&o.tool, &o.args) { e(i, o, code, m, allowed); }
        let w = crate::commands::unknown_args(&o.tool, &o.args); warns[i] = w.iter().map(|k| format!("ignored unknown argument '{k}'")).collect();
        let mut refs = vec![]; walk_refs(&o.args, &mut |s| refs.push(s.to_string()));
        for s in refs { match parse_ref(&s) { Err(m) => e(i, o, "invalid_reference", m, None), Ok(None) => {}, Ok(Some((id, _))) => { match ops.iter().position(|x| x.id == id) { None => e(i, o, "invalid_reference", format!("reference '{s}' names an unknown op '{id}'"), None), Some(j) if j >= i => e(i, o, "invalid_reference", format!("reference '{s}' points forward or at itself (op '{id}' is ops[{j}])"), None), _ => {} } } } }
        if let Some(t) = o.args.get("track_id").and_then(|v| v.as_str()) { if locked.contains(&t) && !is_ref(&o.args["track_id"]) && crate::commands::registry_ref().iter().any(|c| c.name == o.tool && c.mutates) && o.tool != "set_track" { e(i, o, "track_locked", format!("track '{t}' is locked: tool '{}' would modify it; unlock with set_track locked=false", o.tool), None); } }
    }
    (errs, warns)
}
fn err_text(code: &str, head: &str, body: Value) -> String { format!("{code}: {head}\n{body}") }
fn validation_error(errs: &[VErr]) -> String {
    let first = &errs[0]; let all_same = errs.iter().all(|e| e.code == first.code && e.index == first.index);
    let code = if errs.len() == 1 || all_same { match first.code { "invalid_reference" => "invalid_reference", "forbidden_in_batch" => "forbidden_in_batch", _ => "batch_failed" } } else { "batch_failed" };
    let head = if errs.len() == 1 { format!("ops[{}] ({}): {}", first.index, first.tool, first.message) } else { format!("ops[{}] ({}): {} (+{} more problem(s); validation applied nothing)", first.index, first.tool, first.message, errs.len() - 1) };
    err_text(code, &head, json!({"validation_errors": errs.iter().map(|e| e.json()).collect::<Vec<_>>(), "applied": 0, "rolled_back": false}))
}
fn id_fields(v: &Value, out: &mut BTreeMap<&'static str, Vec<String>>) {
    for (single, plural, bucket) in [("clip_id", "clip_ids", "clip_ids"), ("track_id", "track_ids", "track_ids"), ("asset_id", "asset_ids", "asset_ids"), ("effect_id", "effect_ids", "effect_ids")] {
        if let Some(s) = v.get(single).and_then(|x| x.as_str()) { out.entry(bucket).or_default().push(s.to_string()); }
        if let Some(a) = v.get(plural).and_then(|x| x.as_array()) { for s in a.iter().filter_map(|x| x.as_str()) { out.entry(bucket).or_default().push(s.to_string()); } } }
}
fn brief(v: &Value) -> Value {
    match v { Value::Object(o) => { let mut m = Map::new(); for (k, x) in o.iter().take(8) { match x { Value::String(s) if s.chars().count() <= 80 => { m.insert(k.clone(), x.clone()); } Value::Number(_) | Value::Bool(_) => { m.insert(k.clone(), x.clone()); } Value::Array(a) => { m.insert(k.clone(), if a.len() <= 4 && a.iter().all(|y| y.is_string() || y.is_number()) { x.clone() } else { json!({"count": a.len()}) }); } _ => {} } } Value::Object(m) } other => other.clone() }
}
pub struct Run { pub calcs: Vec<Value>, pub results: Vec<Value>, pub ids: Vec<String>, pub failed: Vec<Value>, pub applied: usize, pub warnings: Vec<Value>, pub any_mutation: bool, pub tools: Vec<String> }
/// Phase 2 on the live session. `atomic` rolls the project back to the pre-batch snapshot on the first failure; otherwise ops done so far stay.
pub fn run_ops(ss: &mut Session, ops: &[Op], warns: &[Vec<String>], atomic: bool, stop_on_error: bool) -> Run {
    let mut results: HashMap<String, Value> = HashMap::new(); let mut r = Run { calcs: vec![], results: vec![], ids: vec![], failed: vec![], applied: 0, warnings: vec![], any_mutation: false, tools: vec![] };
    let mutates: HashMap<&str, bool> = crate::commands::registry_ref().iter().map(|c| (c.name, c.mutates)).collect();
    for (i, o) in ops.iter().enumerate() {
        crate::guard::check();
        let fail = |m: String| -> Value { let e = crate::errors::ToolError::from(m.clone()); json!({"index": i, "id": o.id, "tool": o.tool, "error": {"code": e.code, "message": m, "param": e.param}}) };
        let args = match resolve(&o.args, &results) { Ok(a) => a, Err(m) => { r.failed.push(fail(if m.starts_with("calc_error") { m } else { format!("invalid_reference: {m}") })); if stop_on_error { break } else { continue } } };
        { fn hc(v: &Value) -> bool { super::batch_calc::is_calc(v) || match v { Value::Array(a) => a.iter().any(hc), Value::Object(m) => m.values().any(hc), _ => false } } if hc(&o.args) { r.calcs.push(json!({"index": i, "id": o.id, "args": args.clone()})); } }
        match crate::commands::dispatch_nested(ss, &o.tool, &args) {
            Ok(mut v) => { if !warns[i].is_empty() || v.get("warnings").is_some() { r.warnings.push(json!({"index": i, "id": o.id, "warnings": v.get("warnings").cloned().unwrap_or_else(|| json!(warns[i]))})); } if let Some(ob) = v.as_object_mut() { ob.remove("no_history"); }
                if mutates.get(o.tool.as_str()).copied().unwrap_or(false) { r.any_mutation = true; } results.insert(o.id.clone(), v.clone()); r.results.push(json!({"index": i, "id": o.id, "tool": o.tool, "result": v})); r.applied += 1; r.tools.push(o.tool.clone()); }
            Err(m) => { r.failed.push(fail(m)); if stop_on_error || atomic { break } } }
    }
    let mut ids: BTreeMap<&'static str, Vec<String>> = BTreeMap::new(); for x in &r.results { id_fields(&x["result"], &mut ids); }
    r.ids = ids.into_iter().flat_map(|(_, v)| v).collect(); r
}
fn created(results: &[Value]) -> Value { let mut ids: BTreeMap<&'static str, Vec<String>> = BTreeMap::new(); for x in results { id_fields(&x["result"], &mut ids); } let mut m = Map::new(); for k in ["clip_ids", "track_ids", "asset_ids", "effect_ids"] { m.insert(k.into(), json!(ids.get(k).cloned().unwrap_or_default())); } Value::Object(m) }

/// Hash of the whole project JSON: the revision a dry run rehearsed against and a real run may demand (`expect_revision`).
pub fn revision(ss: &Session) -> String { use std::hash::{Hash, Hasher}; let mut h = std::collections::hash_map::DefaultHasher::new(); serde_json::to_vec(&ss.project).unwrap_or_default().hash(&mut h); format!("{:016x}", h.finish()) }
/// Clips the batch created or retimed that end up shorter than `floor` frames (negative drift corrections must never collapse a cut to 0 frames).
fn short_clips(before: &crate::model::Project, after: &crate::model::Project, floor: u32) -> Vec<(String, f64)> {
    let fps = after.fps.max(1.0); let mut v = vec![];
    for c in after.tracks.iter().flat_map(|t| t.clips.iter()) {
        let touched = match before.find_clip(&c.id) { None => true, Some(b) => (b.duration - c.duration).abs() > 1e-9 || (b.start - c.start).abs() > 1e-9 };
        if touched && c.duration * fps < floor as f64 - 1e-6 { v.push((c.id.clone(), c.duration * fps)); }
    }
    v
}
/// U2.1: `return:"minimal"` keeps only the verdict, the created ids and any failure detail; other modes are unchanged.
fn shape(out: Value, mode: &str, ids: &[String]) -> Value {
    if mode != "minimal" { return out; }
    let mut m = Map::new(); m.insert("count".into(), out["applied"].clone());
    for k in ["ok", "dry_run", "revision", "failed_op", "failed_ops", "rolled_back", "no_history", "history_label", "note"] { if let Some(v) = out.get(k) { m.insert(k.into(), v.clone()); } }
    if out["warnings"].as_array().map_or(false, |w| !w.is_empty()) { m.insert("warnings".into(), out["warnings"].clone()); }
    m.insert("created".into(), json!(ids)); Value::Object(m)
}
pub fn batch(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let expanded = super::batch_expand::expand_args(ss, a)?; let was_expanded = expanded.is_some(); let a = expanded.as_ref().unwrap_or(a);
    if a.get("expand_only").and_then(|v| v.as_bool()).unwrap_or(false) { let ops = parse_ops(a)?; return Ok(json!({"ok": true, "expand_only": true, "count": ops.len(), "ops": a["ops"].clone(), "note": "nothing was run; these are the exact ops a real run would execute"})); }
    let ops = parse_ops(a)?;
    let rev = revision(ss);
    if let Some(want) = a.get("expect_revision").and_then(|v| v.as_str()) { if want != rev { return Err(format!("project_drift: the project changed since the dry run (expected revision {}, live revision {rev}); run the dry_run again", sanitize(want))); } } let t0 = std::time::Instant::now();
    let flag = |k: &str, d: bool| a.get(k).and_then(|v| v.as_bool()).unwrap_or(d);
    let (atomic, dry, stop) = (flag("atomic", true), flag("dry_run", false), flag("stop_on_error", true)); let mode = match a.get("return").and_then(|v| v.as_str()) { None | Some("summary") => "summary", Some("full") => "full", Some("ids") => "ids", Some("minimal") => "minimal", Some(o) => return Err(format!("unknown return '{}'. Valid: summary, full, ids, minimal", sanitize(o))) }; let full = mode == "full";
    let quiet: std::collections::HashSet<usize> = a.get("ops").and_then(|v| v.as_array()).map(|l| l.iter().enumerate().filter(|(_, o)| o.get("quiet").and_then(|q| q.as_bool()).unwrap_or(false)).map(|(i, _)| i).collect()).unwrap_or_default();
    let label = a.get("label").and_then(|v| v.as_str()).map(|s| sanitize(s));
    let (errs, warns) = validate(ss, &ops); if !errs.is_empty() { return Err(validation_error(&errs)); }
    let before = ss.project.clone();
    let floor = match a.get("min_clip_frames") { None | Some(Value::Null) => 2u32, Some(v) => match v.as_u64() { Some(n) if (0..=4).contains(&n) => n as u32, _ => return Err("min_clip_frames must be a whole number from 0 to 4 (0 disables the check)".into()) } };
    let mut run = run_ops(ss, &ops, &warns, atomic, stop);
    if run.failed.is_empty() && floor > 0 { let short = short_clips(&before, &ss.project, floor);
        if !short.is_empty() { let list = short.iter().map(|(id, f)| format!("{id} ({:.2} frames)", f)).collect::<Vec<_>>().join(", "); let m = format!("clip_too_short: {list} would end up under {floor} frame(s); a trim or drift correction collapsed a clip (fix the ops or pass min_clip_frames lower; 0 disables)");
            if atomic || dry { let i = ops.len().saturating_sub(1); run.failed.push(json!({"index": i, "id": ops[i].id, "tool": ops[i].tool, "error": {"code": "clip_too_short", "message": m, "param": "min_clip_frames"}, "clips": short.iter().map(|x| x.0.clone()).collect::<Vec<_>>()})); }
            else { run.warnings.push(json!({"index": ops.len().saturating_sub(1), "code": "clip_too_short", "warnings": [m]})); } } }
    if !run.failed.is_empty() && (atomic || dry) {
        ss.project = before; let f = run.failed[0].clone(); let i = f["index"].as_u64().unwrap_or(0);
        return Err(err_text("batch_failed", &format!("ops[{i}] ({}) failed: {} (rolled back, nothing applied)", f["tool"].as_str().unwrap_or(""), f["error"]["message"].as_str().unwrap_or("")), json!({"failed_op": f, "applied": 0, "rolled_back": true})));
    }
    let n = run.results.len(); let created = created(&run.results);
    let mut out = json!({"ok": run.failed.is_empty(), "applied": run.applied, "dry_run": dry, "revision": rev, "created": created, "warnings": run.warnings, "elapsed_ms": t0.elapsed().as_millis() as u64});
    if !run.calcs.is_empty() { out["resolved_calcs"] = json!(run.calcs); }
    out["results"] = Value::Array(run.results.iter().map(|x| if quiet.contains(&(x["index"].as_u64().unwrap_or(u64::MAX) as usize)) { json!({"index": x["index"], "id": x["id"], "tool": x["tool"], "quiet": true}) } else if mode == "ids" { let mut m = BTreeMap::new(); id_fields(&x["result"], &mut m); json!({"index": x["index"], "id": x["id"], "tool": x["tool"], "ok": true, "created": m.into_values().flatten().collect::<Vec<_>>()}) } else if full { x.clone() } else { json!({"index": x["index"], "id": x["id"], "tool": x["tool"], "result_brief": brief(&x["result"])}) }).collect());
    if !run.failed.is_empty() { out["failed_op"] = run.failed[0].clone(); out["failed_ops"] = json!(run.failed); out["rolled_back"] = json!(false); }
    if serde_json::to_string(&out).map(|s| s.len()).unwrap_or(0) > MAX_OUT_BYTES { out["results"] = Value::Array(run.results.iter().map(|x| json!({"index": x["index"], "id": x["id"], "tool": x["tool"]})).collect()); out["note"] = json!("output over the size cap: per-op results reduced to index/id/tool; created ids are complete"); }
    if dry { ss.project = before; out["no_history"] = json!(true); out["note"] = json!("dry run: nothing was applied; ids are those a real run on the same project creates"); if was_expanded { out["expanded_ops"] = a["ops"].clone(); } return Ok(shape(out, mode, &run.ids)); }
    if !run.any_mutation || n == 0 { out["no_history"] = json!(true); } else { out["history_label"] = json!(format!("batch: {} x{}", label.unwrap_or_else(|| run.tools.first().cloned().unwrap_or_default()), n)); }
    Ok(shape(out, mode, &run.ids))
}
pub fn commands() -> Vec<Command> { vec![
    Command { name: "batch", description: "Run many tool calls as ONE atomic, undoable step. ops: [{id?, tool, args}] (max 500). atomic (default true): any failure restores the project exactly (ids included) and returns batch_failed and the failing op index. dry_run: validates and runs on the live project, then discards it; returns the ids a real run would create. stop_on_error (default true); with atomic:false the ops done so far are kept as one history entry. References: a string value \"$<op_id>.<field>\" (e.g. \"$a.clip_id\", \"$sc.clip_ids[0]\") is replaced by that field of an EARLIER op's result, keeping its type; ops without an id are \"$1\", \"$2\" by position; write \"$$\" for a literal $. Validation reports every problem (op index, allowed values) before anything runs. Forbidden inside a batch: batch, undo, redo, restore, load_project, save_project, checkpoint, checkpoint_delete, collect_project, relink_media, recipe_apply, export*, render_job*, history_*; project_set only as the first op. return: summary (default), full, ids (per-op id and created ids) or minimal ({ok,count,created} plus failure detail, smallest reply); an op with quiet:true leaves its result body out. A failed op always returns its full error detail. Any numeric arg may be {\"$calc\":\"expr\"}: numbers, + - * / % ^, parentheses, min max clamp abs floor ceil round sqrt sin cos mix(a,b,t) smoothstep(e0,e1,x), pi, foreach variables (i, n, numeric vars) and $op.field numeric references; no strings or loops (max 400 characters, 120 tokens). It is evaluated while expanding when its inputs are static and at execution time when it uses a $ref (resolved_calcs echoes those numbers); errors are calc_error with the position in the expression. An op may be {\"foreach\":{\"var\":\"c\",\"in\":SRC,\"max\":N},\"ops\":[...]} where SRC is an array, {\"range\":[a,b,step?]} (b excluded) or {\"find\":{find args}}; {{c}}, {{i}} (index) and {{n}} (count) substitute into args (a whole-string {{i}} becomes a number); ids inside get #<i> suffixes ($id#3.clip_id) and a bare $id inside the body means the same iteration; foreach nests 2 deep and the expanded list obeys the 500-op cap. expand_only:true returns the expanded ops without running (dry_run also echoes them as expanded_ops). dry_run returns `revision` (a hash of the project it rehearsed against); pass it back as expect_revision and the real run is rejected as a drift error if anything changed in between. min_clip_frames (0-4, default 2; 0 disables): any clip the batch creates or retimes that ends under this many frames fails the batch with code clip_too_short (atomic/dry_run roll back; atomic:false only warns). Result: ok, applied, revision, results, created {clip_ids, track_ids, asset_ids, effect_ids}, warnings, elapsed_ms.", mutates: true,
        schema: obj(vec![("ops", json!({"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"tool":{"type":"string"},"args":{"type":"object"},"quiet":{"type":"boolean","description":"omit this op's result body from the reply"}},"required":["tool"]},"description":"ordered tool calls, max 500"})), ("expand_only", json!({"type":"boolean","description":"expand foreach and return the exact flat ops without running anything"})), ("atomic", json!({"type":"boolean"})), ("min_clip_frames", json!({"type":"integer","minimum":0,"maximum":4,"description":"default 2: guards against accidental collapsed clips (clips the batch creates or retimes must be at least this many frames); 0 disables"})), ("dry_run", json!({"type":"boolean"})), ("stop_on_error", json!({"type":"boolean"})), ("label", s("history label")), ("expect_revision", s("revision returned by a dry run; the batch is rejected as a drift error when the live project no longer matches")), ("return", json!({"type":"string","enum":["summary","full","ids","minimal"]}))], &["ops"]), run: batch },
] }

/// Strip control chars and cap length for any untrusted text echoed in errors.
fn sanitize(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(80).collect()
}
