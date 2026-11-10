//! RV-3 / RV-22: registry lint. (1) A description that tells the caller to use some snake_case name ("via set_text_style") must name a real tool, or a
//! property / enum value of that same tool. Catches stale advice such as list_fonts pointing at set_text_style instead of set_text_weight.
//! (2) Measured report (ignored test, run with --ignored --nocapture): for every enum property, does a bogus value produce an error that lists the allowed values?
use kimo_core::commands::{dispatch, dispatch_ex, registry_ref, Session};
use serde_json::{json, Value};
use std::collections::HashSet;
const LEAD: [&str; 8] = ["via", "use", "using", "see", "call", "with", "run", "then"];
fn words(s: &str) -> Vec<(String, String)> { let mut out = vec![]; let mut prev = String::new(); let mut cur = String::new();
    for ch in s.chars().chain(std::iter::once(' ')) { if ch.is_ascii_alphanumeric() || ch == '_' { cur.push(ch); } else if !cur.is_empty() { out.push((prev.clone(), cur.clone())); prev = cur.to_lowercase(); cur.clear(); } else { } } out }
fn schema_names(v: &Value, out: &mut HashSet<String>) { match v { Value::Object(m) => { for (k, x) in m { out.insert(k.clone()); schema_names(x, out); } } Value::Array(a) => { for x in a { schema_names(x, out); } } Value::String(s) => { out.insert(s.clone()); } _ => {} } }
#[test] fn descriptions_only_point_at_real_tools() {
    let tools: HashSet<&str> = registry_ref().iter().map(|c| c.name).collect(); let mut bad = vec![]; let mut all = HashSet::new(); for c in registry_ref() { schema_names(&c.schema, &mut all); } { let mut ss = Session::new(); if let Ok(v) = dispatch(&mut ss, "list_effects", &json!({})) { for e in v.as_array().cloned().unwrap_or_default() { if let Some(k) = e.get("kind").and_then(|k| k.as_str()) { all.insert(k.to_string()); } } } }
    for c in registry_ref() { let mut own = HashSet::new(); schema_names(&c.schema, &mut own);
        for (prev, w) in words(&c.description) { if !w.contains('_') || !LEAD.contains(&prev.as_str()) { continue; } let lw = w.to_lowercase();
            if tools.contains(w.as_str()) || all.contains(&w) || own.contains(&w) || own.contains(&lw) || w.starts_with("KIMO_") { continue; } bad.push(format!("{}: description says \"{} {}\" but {} is not a tool or a property/value of any tool", c.name, prev, w, w)); } }
    assert!(bad.is_empty(), "stale tool references in descriptions:\n{}", bad.join("\n"));
}
fn prop_names(v: &Value, out: &mut HashSet<String>) { if let Some(p) = v.get("properties").and_then(|p| p.as_object()) { for (k, x) in p { out.insert(k.to_lowercase()); prop_names(x, out); } } if let Some(i) = v.get("items") { prop_names(i, out); } }
/// Reviewed by hand: the advice is right but the target takes an opaque JSON object (node) or is the reading/path counterpart, so no property name carries the noun.
const REVIEWED: [(&str, &str, &str); 4] = [("bind_shared_grade_graphs", "binding", "set_grade_scopes"), ("set_grade_node", "read", "inspect_grade_graph"), ("set_node_param", "kind", "set_grade_node"), ("grade_node_tracking", "keys", "set_node_mask")];
fn via_problems(owner: &str, desc: &str) -> Vec<String> {
    let by: std::collections::HashMap<&str, &kimo_core::commands::Command> = registry_ref().iter().map(|c| (c.name, c)).collect(); let mut bad = vec![]; let ws = words(desc);
    for i in 1..ws.len() { let (prev, w) = &ws[i]; if prev != "via" || i < 2 { continue; } let Some(t) = by.get(w.as_str()) else { continue }; let noun = ws[i - 2].1.to_lowercase(); if noun.len() < 4 || noun.ends_with("able") || REVIEWED.iter().any(|(o, n, t)| *o == owner && *n == noun && *t == w.as_str()) { continue; }
        let mut names = HashSet::new(); prop_names(&t.schema, &mut names); let hay = format!("{} {}", t.name, names.iter().cloned().collect::<Vec<_>>().join(" ")); if !hay.contains(&noun) { bad.push(format!("{owner}: \"{noun} via {w}\" but {w} never mentions {noun}")); } }
    bad
}
#[test] fn x_via_tool_means_the_tool_handles_x() {
    // "weight via set_text_style": the named tool's name or one of its property names must contain the thing (a mention inside its description does not count), otherwise the advice is stale.
    let bad: Vec<String> = registry_ref().iter().flat_map(|c| via_problems(c.name, c.description)).collect(); assert!(bad.is_empty(), "stale 'via' advice:\n{}", bad.join("\n"));
}
#[test] fn via_lint_still_catches_the_original_set_text_style_bug() {
    // mutation check: the pre-fix list_fonts wording must fail the lint, and the fixed wording must pass
    let old = "List fonts. Variable fonts accept weight via set_text_style."; assert!(!via_problems("list_fonts", old).is_empty(), "lint is too weak: the original bug passes");
    let new = registry_ref().iter().find(|c| c.name == "list_fonts").unwrap().description; assert!(via_problems("list_fonts", new).is_empty());
}
#[test] fn list_fonts_points_at_set_text_weight() {
    let d = registry_ref().iter().find(|c| c.name == "list_fonts").unwrap().description; assert!(d.contains("set_text_weight") && !d.contains("set_text_style"), "{d}");
}
/// Every string enum in every schema: a bogus value must come back with the allowed list (hint + allowed + allowed_total) when the call reaches the enum check.
/// Calls that fail earlier for another reason (needs a real asset, pipeline, etc.) are counted as "unverified", never as passing.
#[test] fn every_schema_enum_rejection_lists_allowed_values() {
    let (mut total, mut listed) = (0, 0); let (mut unverified, mut missing) = (vec![], vec![]);
    for c in registry_ref() { let props = match c.schema.get("properties").and_then(|p| p.as_object()) { Some(p) => p.clone(), None => continue };
        for (pname, p) in &props { let Some(en) = p.get("enum").and_then(|e| e.as_array()) else { continue }; let vals: Vec<String> = en.iter().filter_map(|v| v.as_str().map(String::from)).collect(); if vals.is_empty() { continue; }
            let mut s = Session::new(); let tr = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
            let cl = dispatch(&mut s, "add_shape", &json!({"track_id": tr, "shape": "rect", "width": 50, "height": 50, "start": 0, "duration": 2})).unwrap()["clip_id"].as_str().unwrap().to_string();
            let mut a = serde_json::Map::new(); for r in c.schema.get("required").and_then(|r| r.as_array()).cloned().unwrap_or_default() { let k = r.as_str().unwrap().to_string(); let t = props.get(&k).and_then(|x| x.get("type")).and_then(|t| t.as_str()).unwrap_or("string");
                a.insert(k.clone(), match t { "number" | "integer" => json!(1), "boolean" => json!(true), "array" => json!([]), "object" => json!({}), _ => if k == "clip_id" { json!(cl) } else if k == "track_id" { json!(tr) } else { json!(vals[0]) } }); }
            for (k, v) in a.clone() { if v == json!("x") { a.insert(k, json!(cl.clone())); } }
            if props.contains_key("clip_id") { a.entry("clip_id".to_string()).or_insert(json!(cl)); } if props.contains_key("track_id") { a.entry("track_id".to_string()).or_insert(json!(tr)); }
            a.insert(pname.clone(), json!("zz_bogus")); total += 1;
            match dispatch_ex(&mut s, c.name, &Value::Object(a)) { Err(e) => { if e.allowed_total == Some(vals.len()) && e.param.as_deref() == Some(pname.as_str()) { listed += 1; } else if e.message.contains("zz_bogus") || vals.iter().take(2).all(|v| e.message.contains(v.as_str())) { missing.push(format!("{}.{}: {}", c.name, pname, e.message.chars().take(90).collect::<String>())); } else { unverified.push(format!("{}.{}: {}", c.name, pname, e.message.chars().take(70).collect::<String>())); } }
                Ok(_) => missing.push(format!("{}.{}: ACCEPTED bogus value", c.name, pname)) } } }
    eprintln!("enum properties probed: {total}; rejection lists allowed values: {listed}; reached the enum but no list: {}; failed earlier (unverified): {}", missing.len(), unverified.len());
    for g in &missing { eprintln!("NOLIST {g}"); } for g in &unverified { eprintln!("UNVERIFIED {g}"); }
    assert!(missing.is_empty(), "enum rejections without an allowed list:\n{}", missing.join("\n"));
}
