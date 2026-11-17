//! The keyword search kimo used before tool_index (toolgroups.rs at 8126ebd), kept verbatim so the discovery benchmark
//! can score old and new on the same requests. Test-only; nothing in the engine calls it.
#![allow(dead_code)]
use kimo_core::commands::{registry_ref, Command};
use kimo_core::toolgroups::{group_of, search_catalog};
use serde_json::{json, Value};
fn reg() -> &'static [Command] { registry_ref() }
fn first_sentence(d: &str) -> String { let e = d.find(". ").map(|i| i + 1).unwrap_or(d.len()); let s = &d[..e]; if s.len() > 150 { format!("{}...", &s[..s.char_indices().map(|c| c.0).take_while(|&i| i <= 147).last().unwrap_or(0)]) } else { s.to_string() } }
fn required_args(c: &Command) -> Vec<String> { c.schema.get("required").and_then(|r| r.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default() }
pub fn search_tools(q: &str, limit: usize, group: Option<&str>, effects: &Value, transitions: &Value) -> Value {
    let limit = limit.clamp(1, 25); let r = search(q, 50); let total = r["total"].as_u64().unwrap_or(0);
    let mut tools: Vec<Value> = vec![];
    for m in r["matches"].as_array().into_iter().flatten() { let name = m["action"].as_str().unwrap_or(""); let Some(c) = reg().iter().find(|c| c.name == name) else { continue }; if let Some(g) = group { if group_of(name) != g { continue; } }
        let req = required_args(c); tools.push(json!({"type":"tool","name":name,"group":group_of(name),"summary":m["summary"],"required":req,"call_as":format!("{}({})", name, req.join(", "))})); }
    let words: Vec<String> = q.split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2).map(|w| w.to_lowercase()).collect();
    let mut cat: Vec<Value> = vec![]; if group.map_or(true, |g| g == "effects") { for h in search_catalog(q, effects, transitions, 8) { let ty = h["type"].as_str().unwrap_or(""); let kind = h["kind"].as_str().unwrap_or("");
        let (tool, extra) = if ty == "transition" { ("add_transition", ", duration") } else { ("add_effect", "") };
        cat.push(json!({"type":ty,"name":kind,"group":"effects","summary":h["summary"],"call_as":format!("{tool}(clip_id{extra}, kind=\"{kind}\")")})); } }
    let matches_all = |h: &Value| { let t = format!("{} {}", h["name"].as_str().unwrap_or(""), h["summary"].as_str().unwrap_or("")).to_lowercase(); !words.is_empty() && words.iter().all(|w| t.contains(w.as_str())) };
    let mut out: Vec<Value> = vec![]; let mut rest: Vec<Value> = vec![];
    for (i, h) in cat.into_iter().enumerate() { if i == 0 && matches_all(&h) { out.push(h); } else { rest.push(h); } }
    out.extend(tools); out.extend(rest); out.truncate(limit);
    json!({"matches": out, "total": total, "note": "call any listed name directly as a tool; effects and transitions are used through add_effect / add_transition"})
}
pub fn search(q: &str, limit: usize) -> Value {
    // OR search ranked by how many query words match: name (3), argument names (2), description (1). Words are lowercased and lightly stemmed.
    const STOP: &[&str] = &["a", "an", "the", "to", "of", "for", "and", "or", "in", "on", "with", "add", "set", "make", "create"];
    let stem = |w: &str| -> String { let w = w.to_lowercase(); for suf in ["ing", "es", "s"] { if w.len() > suf.len() + 3 && w.ends_with(suf) { return w[..w.len() - suf.len()].to_string(); } } w };
    let mut words: Vec<String> = q.split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| !w.is_empty()).map(|w| w.to_string()).collect();
    let kept: Vec<String> = words.iter().filter(|w| !STOP.contains(&w.to_lowercase().as_str())).cloned().collect(); if !kept.is_empty() { words = kept; }
    let words: Vec<String> = words.iter().map(|w| stem(w)).collect();
    // two-word phrases: "slow motion" is about speed, "green screen key" is about chroma keying; the second word would otherwise outrank the intent
    let has = |x: &[&str]| words.iter().any(|w| x.contains(&w.to_lowercase().as_str()));
    let words: Vec<String> = { let sp = has(&["slow", "slowmo", "fast"]); let gs = has(&["green", "greenscreen", "screen", "chroma"]); words.iter().filter(|w| !(sp && w.as_str() == "motion") && !(gs && w.as_str() == "key")).cloned().collect() };
    fn alts<'a>(w: &'a str) -> Vec<&'a str> { let mut v = vec![w]; match w { "slow" | "slowmo" | "fast" => v.push("speed"), "shatter" | "break" | "smash" => { v.push("fracture"); v.push("shard"); } "speech" | "spoken" | "voice" => { v.push("transcribe"); v.push("caption"); } "green" | "greenscreen" | "screen" => { v.push("chroma"); } "shake" | "shaky" => { v.push("stabilize"); v.push("shake"); } "fade" => { v.push("volume"); v.push("opacity"); } "subtitle" => v.push("caption"), "cover" => v.push("thumbnail"), "cut" => { v.push("split"); v.push("trim"); } "um" | "uh" | "ums" | "uhs" | "stutter" | "disfluency" => v.push("filler"), "tiny" | "stray" | "false" => v.push("short"), "point" | "pointing" | "callout" | "annotate" | "annotation" => v.push("arrow"), _ => {} } v }
    let mut hits: Vec<(usize, usize, &Command)> = reg().iter().filter_map(|c| {
        let nm = c.name.to_lowercase(); let ds = c.description.to_lowercase();
        let args: String = c.schema.get("properties").and_then(|p| p.as_object()).map(|o| o.iter().map(|(k, v)| format!("{} {}", k, v.get("description").and_then(|d| d.as_str()).unwrap_or(""))).collect::<Vec<_>>().join(" ").to_lowercase()).unwrap_or_default();
        let arg_names: String = c.schema.get("properties").and_then(|p| p.as_object()).map(|o| o.keys().cloned().collect::<Vec<_>>().join(" ").to_lowercase()).unwrap_or_default();
        let (mut sc, mut matched) = (0usize, 0usize);
        let toks: Vec<&str> = nm.split('_').collect();
        for w in &words { let mut m = 0; for a in alts(w.as_str()) { let v = if toks.iter().any(|t| *t == a || stem(t) == a) { 4 } else if nm.contains(a) { 3 } else if arg_names.contains(a) { 2 } else if ds.contains(a) || args.contains(a) { 1 } else { 0 }; m = m.max(v); } if m > 0 { sc += m; matched += 1; } }
        if matched > 0 { Some((matched, sc, c)) } else { None } }).collect();
    hits.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)).then(a.2.name.cmp(b.2.name)));
    json!({"matches": hits.iter().take(limit.clamp(1, 50)).map(|(_, _, c)| json!({"group": group_of(c.name), "action": c.name, "summary": first_sentence(c.description)})).collect::<Vec<_>>(), "total": hits.len()})
}
