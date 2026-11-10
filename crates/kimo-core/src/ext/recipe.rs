//! RV-11 recipe tools: `recipe_list`, `recipe_describe`, `recipe_apply`. A recipe expands to plain tool calls and runs through `batch` (atomic, one undo step).
use super::batch::Op;
use super::kit::*;
use crate::recipes;

struct Entry { name: String, version: u32, summary: String, keywords: Vec<String>, schema: Value, roles: Vec<String> }
fn entries() -> Vec<Entry> {
    let mut v: Vec<Entry> = recipes::all().into_iter().map(|r| Entry { name: r.name.into(), version: r.version, summary: r.summary.into(), keywords: r.keywords.iter().map(|s| s.to_string()).collect(), schema: recipes::schema(r), roles: r.roles.iter().map(|s| s.to_string()).collect() }).collect();
    v.push(Entry { name: "lower_third".into(), version: super::lower_third::VERSION, summary: super::lower_third::SUMMARY.into(), keywords: super::lower_third::KEYWORDS.iter().map(|s| s.to_string()).collect(), schema: super::lower_third::schema(), roles: ["bar", "accent", "title", "subtitle"].iter().map(|s| s.to_string()).collect() });
    v.push(Entry { name: "narration_section".into(), version: super::narration_section::VERSION, summary: super::narration_section::SUMMARY.into(), keywords: super::narration_section::KEYWORDS.iter().map(|s| s.to_string()).collect(), schema: super::narration_section::schema(), roles: ["captions", "ducking"].iter().map(|s| s.to_string()).collect() });
    v.sort_by(|a, b| a.name.cmp(&b.name)); v
}
fn clean(s: &str) -> String { s.chars().filter(|c| !c.is_control()).take(40).collect() }
fn entry(name: &str) -> Result<Entry, String> { let all = entries(); let valid = all.iter().map(|e| e.name.clone()).collect::<Vec<_>>().join(", "); all.into_iter().find(|e| e.name == name).ok_or_else(|| format!("invalid_value: unknown recipe '{}'. Valid: {valid}", clean(name))) }
fn ops_json(ops: &[Op]) -> Value { Value::Array(ops.iter().map(|o| json!({"id": o.id, "tool": o.tool, "args": o.args})).collect()) }

fn list(_ss: &mut Session, a: &Value) -> Result<Value, String> {
    let q = a.get("query").and_then(|v| v.as_str()).unwrap_or("").to_lowercase(); let terms: Vec<&str> = q.split_whitespace().collect();
    let rows: Vec<Value> = entries().into_iter().filter(|e| terms.is_empty() || terms.iter().any(|t| e.name.contains(t) || e.summary.to_lowercase().contains(t) || e.keywords.iter().any(|k| k.contains(t)))).map(|e| json!({"name": e.name, "recipe_version": e.version, "summary": e.summary, "keywords": e.keywords})).collect();
    Ok(json!({"count": rows.len(), "recipes": rows}))
}
fn describe(_ss: &mut Session, a: &Value) -> Result<Value, String> {
    let e = entry(str_arg(a, "name")?)?;
    Ok(json!({"name": e.name, "recipe_version": e.version, "summary": e.summary, "keywords": e.keywords, "params": e.schema, "roles": e.roles, "apply_extra": {"start": "timeline seconds (required)", "track_id|track_name": "shared track for all layers; omitted creates one", "name_prefix": "role label prefix in the result: <prefix>/<role>", "dry_run": "returns the expansion ops and a revision for expect_revision"}}))
}
fn apply(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let e = entry(str_arg(a, "name")?)?; let narr = e.name == "narration_section"; let start = if narr { a.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0) } else { num_arg(a, "start")? }; if !start.is_finite() || start < 0.0 { return Err("invalid_value: start must be >= 0 (timeline seconds)".into()); }
    let params = a.get("params").cloned().unwrap_or(json!({})); let prefix = a.get("name_prefix").and_then(|v| v.as_str()).map(clean).unwrap_or_else(|| e.name.clone());
    let track_id = match (a.get("track_id").and_then(|v| v.as_str()), a.get("track_name").and_then(|v| v.as_str())) {
        (Some(_), Some(_)) => return Err("invalid_value: pass track_id or track_name, not both".into()),
        (Some(t), None) => Some(t.to_string()),
        (None, Some(n)) => Some(ss.project.tracks.iter().find(|t| t.name == n).map(|t| t.id.clone()).ok_or_else(|| format!("invalid_value: no track named '{}'; pass track_id or omit it to create a track", clean(n)))?),
        _ => None };
    let (mut ops, dur): (Vec<Op>, f64) = if e.name == "lower_third" {
        let mut p = params.as_object().cloned().ok_or("params must be an object")?; p.insert("start".into(), json!(start)); if let Some(t) = &track_id { p.insert("track_id".into(), json!(t)); } let d = p.get("duration").and_then(|v| v.as_f64()).unwrap_or(5.0);
        (super::lower_third::expand(ss, &Value::Object(p))?, d)
    } else if narr {
        let mut p = params.as_object().cloned().ok_or("params must be an object")?; if a.get("start").is_some() { p.insert("start".into(), json!(start)); } if let Some(t) = &track_id { p.insert("track_id".into(), json!(t)); }
        let pv = Value::Object(p); let ws = super::narration_section::speech_windows(ss, &pv).unwrap_or_default(); let d = ws.last().map(|w| w[1]).unwrap_or(start) - ws.first().map(|w| w[0]).unwrap_or(start);
        (super::narration_section::expand(ss, &pv)?, d)
    } else {
        let r = recipes::find(&e.name).ok_or("recipe table out of sync")?; let p = recipes::check_params(r, &params)?; let d = p.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let ctx = recipes::Ctx { p, w: ss.project.width as f64, h: ss.project.height as f64, start, track: track_id.clone().unwrap_or_else(|| "$trk.track_id".into()) };
        let mut o = (r.expand)(&ctx)?; if track_id.is_none() { o.insert(0, recipes::op("trk", "add_track", json!({}))); } (o, d)
    };
    if ops.is_empty() { return Err("recipe produced no ops".into()); }
    let dry = a.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false); let label = a.get("label").and_then(|v| v.as_str()).map(clean).unwrap_or_else(|| format!("recipe {}", e.name));
    let mut b = json!({"ops": ops_json(&ops), "atomic": true, "dry_run": dry, "label": label, "return": "full"}); if let Some(r) = a.get("expect_revision") { b["expect_revision"] = r.clone(); }
    let ex_ops = ops_json(&ops); let hash = crate::deps::sha256_hex(serde_json::to_string(&ex_ops).unwrap_or_default().as_bytes());
    let flag = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    if flag("expand_only") { return Ok(json!({"ok": true, "expand_only": true, "count": ex_ops.as_array().map_or(0, |o| o.len()), "ops": ex_ops, "expansion_hash": hash, "recipe": {"name": e.name, "recipe_version": e.version}, "no_history": true, "note": "nothing was run; these are the exact ops a real run would execute"})); }
    let mut out = super::batch::batch(ss, &b)?;
    let mut roles = Map::new(); let mut tracks = vec![];
    if let Some(rs) = out["results"].as_array() { for x in rs { let id = x["id"].as_str().unwrap_or(""); if let Some(c) = x["result"]["clip_id"].as_str() { roles.insert(format!("{prefix}/{id}"), json!(c)); } if let Some(t) = x["result"]["track_id"].as_str() { tracks.push(t.to_string()); } } }
    out["results"] = Value::Array(out["results"].as_array().cloned().unwrap_or_default().iter().map(|x| json!({"index": x["index"], "id": x["id"], "tool": x["tool"]})).collect());
    out["recipe"] = json!({"name": e.name, "recipe_version": e.version}); out["roles"] = Value::Object(roles); out["anchors"] = if narr { let pv = params.clone(); let ws = super::narration_section::speech_windows(ss, &pv).unwrap_or_default(); json!({"first_word_start": ws.first().map(|w| w[0]), "last_word_end": ws.last().map(|w| w[1]), "speech_windows": ws, "unit": "timeline seconds"}) } else { json!({"start": start, "end": start + dur, "unit": "timeline seconds"}) };
    if let Some(t) = &track_id { out["track_id"] = json!(t); } else if let Some(t) = tracks.first() { out["track_id"] = json!(t); }
    out["expansion_hash"] = json!(hash); if dry || flag("expansion") { out["expansion"] = ex_ops; }
    Ok(out)
}
use serde_json::Map;
pub fn commands() -> Vec<Command> { vec![
    Command { name: "recipe_list", description: "Convenience: list scene recipes (lower_third, narration_section, title_card, stat_counter, product_reveal, explainer_diagram). query filters by name or keyword.", mutates: false, schema: obj(vec![("query", s("optional keyword filter"))], &[]), run: list },
    Command { name: "recipe_describe", description: "Convenience: describe one recipe: its params as a JSON schema (types, ranges, allowed values, defaults, px-from-centre spaces), roles and version.", mutates: false, schema: obj(vec![("name", s("recipe name from recipe_list"))], &["name"]), run: describe },
    Command { name: "recipe_apply", description: "Apply a recipe as one atomic undoable step (deprecated: use template_define and template_apply). Convenience (deprecated in favour of template_define and template_apply): apply a recipe as ONE atomic undoable step: expands to ordinary tool calls and runs them through batch. Layers share one track. dry_run returns the expansion and a revision. Returns created ids by role and anchors in timeline seconds. Each apply is a new independent instance. The result carries expansion_hash (sha256 of the expanded op list); the ops themselves are returned only for dry_run, expand_only or expansion:true.", mutates: true,
        schema: obj(vec![("name", s("recipe name")), ("params", json!({"type":"object","description":"recipe params, see recipe_describe"})), ("start", n("timeline start, seconds")), ("track_id", s("existing track for every layer")), ("track_name", s("existing track name for every layer")), ("name_prefix", s("prefix of role labels in the result")), ("dry_run", json!({"type":"boolean"})), ("expand_only", json!({"type":"boolean","description":"return the exact ops without running anything"})), ("expansion", json!({"type":"boolean","description":"include the exact expanded ops in the result (default false; always included for dry_run and expand_only)"})), ("label", s("history label")), ("expect_revision", s("revision from a dry run"))], &["name"]), run: apply },
] }
