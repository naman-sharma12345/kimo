//! U2.4: user-authored batch templates. Kimo ships none. A template is a named, parameterised op list stored in the project; `template_apply`
//! substitutes the parameters and runs the result as ONE atomic batch (so foreach / $calc / names all work inside, and it is one undo entry).
use super::kit::*;
use crate::commands::Session;
use serde::{Deserialize, Serialize};

pub const MAX_TEMPLATES: usize = 100;
pub const MAX_BYTES: usize = 65_536;
const TYPES: [&str; 7] = ["number", "string", "boolean", "id", "array", "object", "any"];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Param { pub name: String, #[serde(rename = "type")] pub ty: String, #[serde(default, skip_serializing_if = "Option::is_none")] pub default: Option<Value> }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Template { pub name: String, pub params: Vec<Param>, pub ops: Vec<Value> }
fn ident(s: &str, extra: &str) -> bool { !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || extra.contains(c)) }
fn perr(m: String) -> String { format!("template_param: {m}") }
fn type_ok(ty: &str, v: &Value) -> bool { match ty { "number" => v.is_number(), "string" | "id" => v.is_string(), "boolean" => v.is_boolean(), "array" => v.is_array(), "object" => v.is_object(), _ => true } }
fn build(a: &Value) -> Result<Template, String> {
    let name = str_arg(a, "name")?; if !ident(name, ".-") { return Err("out_of_range: template name must be 1..64 characters of A-Z a-z 0-9 _ . -".into()); }
    let mut params: Vec<Param> = vec![];
    for (i, p) in a.get("params").and_then(|v| v.as_array()).cloned().unwrap_or_default().iter().enumerate() {
        let pn = p.get("name").and_then(|v| v.as_str()).filter(|n| ident(n, "")).ok_or(perr(format!("params[{i}].name must be 1..64 characters of letters, digits and _")))?; if params.iter().any(|q| q.name == pn) { return Err(perr(format!("parameter '{pn}' is defined twice"))); }
        let ty = p.get("type").and_then(|v| v.as_str()).unwrap_or("any"); if !TYPES.contains(&ty) { return Err(perr(format!("params[{i}].type '{ty}' is not allowed. Valid: {}", TYPES.join(", ")))); }
        let default = p.get("default").filter(|d| !d.is_null()).cloned(); if let Some(d) = &default { if !type_ok(ty, d) { return Err(perr(format!("default of '{pn}' is not a {ty}"))); } }
        params.push(Param { name: pn.to_string(), ty: ty.to_string(), default });
    }
    let ops = a.get("ops").and_then(|v| v.as_array()).filter(|o| !o.is_empty() && o.len() <= super::batch::MAX_OPS).ok_or(format!("out_of_range: ops must be 1..{} entries", super::batch::MAX_OPS))?.clone();
    for (i, o) in ops.iter().enumerate() { if !o.is_object() || !(o.get("tool").map_or(false, |t| t.is_string()) || o.get("foreach").is_some()) { return Err(format!("out_of_range: ops[{i}] needs a string tool (or a foreach)")); } }
    let t = Template { name: name.to_string(), params, ops }; if serde_json::to_vec(&t).map(|b| b.len()).unwrap_or(usize::MAX) > MAX_BYTES { return Err(format!("out_of_range: a template may be at most {MAX_BYTES} bytes")); } Ok(t)
}
fn put(ss: &mut Session, t: Template) -> Result<bool, String> {
    if let Some(x) = ss.project.templates.iter_mut().find(|x| x.name == t.name) { *x = t; return Ok(true); }
    if ss.project.templates.len() >= MAX_TEMPLATES { return Err(format!("out_of_range: at most {MAX_TEMPLATES} templates per project")); } ss.project.templates.push(t); Ok(false)
}
fn template_define(ss: &mut Session, a: &Value) -> Result<Value, String> {
    if let Some(p) = a.get("library_path").and_then(|v| v.as_str()) {
        let txt = std::fs::read_to_string(p).map_err(|e| format!("cannot read library_path: {e}"))?; let v: Value = serde_json::from_str(&txt).map_err(|e| format!("library_path is not valid JSON: {e}"))?;
        let list = v.get("templates").and_then(|t| t.as_array()).ok_or("library_path must hold {\"templates\":[...]}")?; let built = list.iter().map(build).collect::<Result<Vec<_>, _>>()?;
        let names: Vec<String> = built.iter().map(|t| t.name.clone()).collect(); for t in built { put(ss, t)?; } return Ok(json!({"ok": true, "imported": names}));
    }
    let t = build(a)?; let (n, k) = (t.name.clone(), t.ops.len()); let params: Vec<String> = t.params.iter().map(|p| p.name.clone()).collect(); let replaced = put(ss, t)?;
    Ok(json!({"ok": true, "name": n, "params": params, "op_count": k, "replaced": replaced}))
}
fn find<'a>(ss: &'a Session, name: &str) -> Result<&'a Template, String> { ss.project.templates.iter().find(|t| t.name == name).ok_or_else(|| { let names: Vec<String> = ss.project.templates.iter().map(|t| t.name.clone()).collect(); format!("unknown_template: no template '{name}' (defined: {})", if names.is_empty() { "none".into() } else { names.join(", ") }) }) }
fn template_list(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let rows: Vec<Value> = ss.project.templates.iter().map(|t| json!({"name": t.name, "params": t.params.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), "op_count": t.ops.len()})).collect();
    let mut out = json!({"templates": rows, "count": rows.len()});
    if let Some(p) = a.get("export_path").and_then(|v| v.as_str()) { std::fs::write(p, serde_json::to_vec_pretty(&json!({"templates": ss.project.templates})).map_err(|e| e.to_string())?).map_err(|e| format!("cannot write export_path: {e}"))?; out["exported_to"] = json!(p); }
    Ok(out)
}
fn template_describe(ss: &mut Session, a: &Value) -> Result<Value, String> { let t = find(ss, str_arg(a, "name")?)?; Ok(json!({"name": t.name, "params": t.params.iter().map(|p| json!({"name": p.name, "type": p.ty, "default": p.default, "required": p.default.is_none()})).collect::<Vec<_>>(), "ops": t.ops, "op_count": t.ops.len()})) }
fn template_delete(ss: &mut Session, a: &Value) -> Result<Value, String> { let n = str_arg(a, "name")?.to_string(); find(ss, &n)?; ss.project.templates.retain(|t| t.name != n); Ok(json!({"ok": true, "deleted": n})) }
fn sub(v: &Value, env: &[(String, Value)]) -> Value {
    if super::batch_calc::is_calc(v) { let e = v["$calc"].as_str().unwrap(); let t = super::batch_calc::rewrite(e, &|n| env.iter().find(|(k, _)| k == n).and_then(|(_, x)| x.as_f64()), &|r| r.to_string()).unwrap_or_else(|_| e.to_string()); return json!({"$calc": t}); }
    match v {
        Value::String(s) => { if let Some(n) = s.strip_prefix("{{").and_then(|r| r.strip_suffix("}}")).filter(|n| !n.contains("{{") && !n.contains("}}")) { if let Some((_, x)) = env.iter().find(|(k, _)| k == n.trim()) { return x.clone(); } return v.clone(); }
            let mut out = s.clone(); for (k, x) in env { let pat = format!("{{{{{k}}}}}"); if out.contains(&pat) { out = out.replace(&pat, &match x { Value::String(t) => t.clone(), o => o.to_string() }); } } Value::String(out) }
        Value::Array(a) => Value::Array(a.iter().map(|x| sub(x, env)).collect()), Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), sub(x, env))).collect()), _ => v.clone(),
    }
}
fn template_apply(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let t = find(ss, str_arg(a, "name")?)?.clone(); let args = match a.get("args") { None | Some(Value::Null) => serde_json::Map::new(), Some(v) => v.as_object().ok_or(perr("args must be an object of parameter values".into()))?.clone() };
    for k in args.keys() { if !t.params.iter().any(|p| &p.name == k) { return Err(perr(format!("unknown parameter '{k}' for template '{}' (parameters: {})", t.name, t.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")))); } }
    let mut env: Vec<(String, Value)> = vec![];
    for p in &t.params { let v = match args.get(&p.name) { Some(v) if !v.is_null() => v.clone(), _ => p.default.clone().ok_or(perr(format!("missing parameter '{}' ({}) for template '{}'", p.name, p.ty, t.name)))? }; if !type_ok(&p.ty, &v) { return Err(perr(format!("parameter '{}' must be a {}", p.name, p.ty))); } env.push((p.name.clone(), v)); }
    let ops: Vec<Value> = t.ops.iter().map(|o| sub(o, &env)).collect();
    let mut b = json!({"ops": ops, "atomic": true, "label": format!("template {}", t.name), "return": "minimal"});
    for k in ["dry_run", "expand_only", "return", "label", "expect_revision", "min_clip_frames"] { if let Some(v) = a.get(k) { b[k] = v.clone(); } }
    let flag = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false); let (dry, only) = (flag("dry_run"), flag("expand_only"));
    let expansion = match super::batch_expand::expand_args(ss, &b)? { Some(x) => x["ops"].clone(), None => b["ops"].clone() };
    let hash = crate::deps::sha256_hex(serde_json::to_string(&expansion).unwrap_or_default().as_bytes()); let count = expansion.as_array().map_or(0, |o| o.len());
    let mut out = super::batch::batch(ss, &b)?; out["template"] = json!(t.name); out["applied_ops"] = json!(count); out["expansion_hash"] = json!(hash);
    if only { out["no_history"] = json!(true); }
    out["args"] = Value::Object(env.into_iter().collect()); if (dry || flag("expansion")) && !only { out["expansion"] = expansion; }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// v7: MOGRT substitute. Adobe's .mogrt format is closed (a proprietary zip container); this is Kimo's
// own open substitute -- a single JSON file bundling a template_define'd template body with the
// essential-property marks (set_essential_properties, FEATURES.md L239 -- already described in this
// codebase as "the knobs an external caller/MOGRT-style template consumer should expose") captured off
// whichever clips the template's last application (or any manual clips) carry them on. Essential marks
// are plain `essential:` tags on Clip.tags (see ext/data_driven_v7.rs), so round-tripping them is just
// carrying the tag strings verbatim -- no separate parser needed here.
const MAX_PACKAGE_BYTES: usize = 1_048_576;

fn export_template_package(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let t = find(ss, str_arg(a, "name")?)?.clone();
    let path = str_arg(a, "path")?;
    let explicit: Option<Vec<String>> = a.get("clip_ids").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());
    let mut essential = vec![];
    for tr in &ss.project.tracks { for c in &tr.clips {
        let tags: Vec<String> = c.tags.iter().filter(|t| t.starts_with("essential:")).cloned().collect();
        if tags.is_empty() { continue; }
        if let Some(ids) = &explicit { if !ids.contains(&c.id) { continue; } }
        essential.push(json!({"clip_id": c.id, "role": c.role, "essential_tags": tags}));
    } }
    let package = json!({"kimo_template_package_version": 1, "template": t, "essential_properties": essential});
    let bytes = serde_json::to_vec_pretty(&package).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_PACKAGE_BYTES { return Err(format!("out_of_range: a template package may be at most {MAX_PACKAGE_BYTES} bytes, this one is {}", bytes.len())); }
    std::fs::write(path, &bytes).map_err(|e| format!("cannot write path: {e}"))?;
    Ok(json!({"path": path, "template_name": t.name, "params": t.params.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), "op_count": t.ops.len(), "essential_clip_count": essential.len()}))
}

fn import_template_package(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let path = str_arg(a, "path")?;
    let txt = std::fs::read_to_string(path).map_err(|e| format!("cannot read path: {e}"))?;
    let v: Value = serde_json::from_str(&txt).map_err(|e| format!("path is not valid JSON: {e}"))?;
    if v.get("kimo_template_package_version").is_none() { return Err("not a kimo template package: missing kimo_template_package_version".into()); }
    let t: Template = serde_json::from_value(v.get("template").cloned().ok_or("package has no \"template\"")?).map_err(|e| format!("package's template is malformed: {e}"))?;
    let name = t.name.clone(); let params: Vec<String> = t.params.iter().map(|p| p.name.clone()).collect(); let op_count = t.ops.len();
    let replaced = put(ss, t)?;
    let reapply = a.get("reapply_essential").and_then(|v| v.as_bool()).unwrap_or(true);
    let (mut reapplied, mut unmatched) = (0usize, vec![]);
    if reapply {
        for entry in v.get("essential_properties").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
            let role = entry.get("role").and_then(|v| v.as_str());
            let orig_id = entry.get("clip_id").and_then(|v| v.as_str());
            let tags: Vec<String> = entry.get("essential_tags").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
            // Prefer matching by role (survives a fresh template_apply, since ops usually carry the role forward);
            // fall back to the original clip_id (useful when importing back into the same project the package came from).
            let target = role.and_then(|r| ss.project.tracks.iter().flat_map(|tr| tr.clips.iter()).find(|c| c.role.as_deref() == Some(r)).map(|c| c.id.clone()))
                .or_else(|| orig_id.filter(|id| ss.project.find_clip(id).is_some()).map(|s| s.to_string()));
            match target.and_then(|id| ss.project.find_clip_mut(&id)) {
                Some(c) => { for tg in tags { if !c.tags.contains(&tg) { c.tags.push(tg); } } reapplied += 1; }
                None => unmatched.push(json!({"clip_id": orig_id, "role": role})),
            }
        }
    }
    Ok(json!({"path": path, "template_name": name, "params": params, "op_count": op_count, "replaced": replaced, "essential_reapplied": reapplied, "essential_unmatched": unmatched}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "template_define", mutates: true, description: "Store a named template in the project (Kimo ships none; you author them). params: [{name, type number|string|boolean|id|array|object|any, default?}]; ops: the batch ops, where {{param}} is replaced by the argument (a whole-string slot keeps its type) and foreach, {\"$calc\"}, @names and $refs work as in batch. Redefining a name replaces it. library_path instead imports every template from a JSON file {\"templates\":[...]}.",
        schema: obj(vec![("name", s("template name: A-Z a-z 0-9 _ . -")), ("params", json!({"type":"array","items":{"type":"object","properties":{"name":{"type":"string"},"type":{"type":"string","enum":TYPES},"default":{}},"required":["name"]}})), ("ops", json!({"type":"array","items":{"type":"object"}})), ("library_path", s("import templates from this JSON file instead of defining one"))], &[]), run: template_define },
    Command { name: "template_apply", mutates: true, description: "Expand a template with args and run it as ONE atomic, undoable batch. Missing, unknown or wrongly typed parameters are rejected by name. dry_run runs and discards; expand_only returns the exact ops without running. The default result is compact: ok, template, applied_ops, created ids, the resolved args and expansion_hash (sha256 of the expanded op list). The exact flat ops come back in expansion only for dry_run, expand_only (as ops) or expansion:true. return (full, ids, minimal, summary; default minimal), label and expect_revision are accepted as in batch.",
        schema: obj(vec![("name", s("template name")), ("args", json!({"type":"object"})), ("dry_run", json!({"type":"boolean"})), ("expand_only", json!({"type":"boolean"})), ("expansion", json!({"type":"boolean","description":"include the exact expanded ops in the result (default false; always included for dry_run)"})), ("return", json!({"type":"string","enum":["summary","full","ids","minimal"]})), ("label", s("history label")), ("expect_revision", s("revision from a dry run"))], &["name"]), run: template_apply },
    Command { name: "template_list", mutates: false, description: "Read-only: list the templates stored in this project (name, parameter names, op count). export_path also writes them to a JSON library file that template_define library_path can import.",
        schema: obj(vec![("export_path", s("write {\"templates\":[...]} to this file"))], &[]), run: template_list },
    Command { name: "template_describe", mutates: false, description: "Read-only: one template in full - parameters with type, default and required, and its ops.", schema: obj(vec![("name", s("template name"))], &["name"]), run: template_describe },
    Command { name: "template_delete", mutates: true, description: "Delete a stored template by name.", schema: obj(vec![("name", s("template name"))], &["name"]), run: template_delete },
    Command { name: "export_template_package", mutates: false, description: "v7 MOGRT substitute: bundle an already-defined template with its clips' essential-property marks into one JSON file. template_define must already have the template; clip_ids defaults to every clip carrying an essential mark. Adobe's .mogrt is a closed/proprietary format; this is Kimo's own open one-file substitute, importable with import_template_package (in this project or another).",
        schema: obj(vec![("name", s("template name, already stored via template_define")), ("path", s("output .json path")), ("clip_ids", json!({"type":"array","items":{"type":"string"},"description":"optional: only these clips' essential marks (default: every clip with any)"}))], &["name","path"]), run: export_template_package },
    Command { name: "import_template_package", mutates: true, description: "v7 MOGRT substitute: load a package written by export_template_package. Registers its template (as template_define would, replacing a same-named one). By default (reapply_essential) also reapplies its essential-property marks onto clips in the CURRENT project: matched by role first (so a fresh template_apply instance picks them up), falling back to the original clip_id (round-tripping within the same project). Unmatched entries are reported, not silently dropped.",
        schema: obj(vec![("path", s("input .json path, written by export_template_package")), ("reapply_essential", json!({"type":"boolean","description":"default true"}))], &["path"]), run: import_template_package },
] }
