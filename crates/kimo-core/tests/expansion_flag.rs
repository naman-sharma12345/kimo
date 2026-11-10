//! template_apply / recipe_apply: the exact ops come back only on request (dry_run, expand_only, expansion:true); expansion_hash always.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn fresh() -> (Session, String) { let mut ss = Session::new(); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); (ss, t) }
fn define(ss: &mut Session) { c(ss, "template_define", json!({"name": "seg", "params": [{"name": "track", "type": "id"}], "ops": [
    {"foreach": {"var": "k", "in": {"range": [0, 40]}}, "ops": [{"tool": "add_text", "args": {"track_id": "{{track}}", "text": "Caption {{k}}", "start": {"$calc": "k*1.5"}, "duration": 1.25}}]},
    {"foreach": {"var": "k", "in": {"range": [0, 12]}}, "ops": [{"id": "t", "tool": "add_text", "args": {"track_id": "{{track}}", "text": "Title {{k}}", "start": {"$calc": "60+k*4"}, "duration": 3.5}},
      {"tool": "set_keyframes", "args": {"clip_id": "$t.clip_id", "property": "opacity", "keys": [{"t": 0, "v": 0}, {"t": 0.5, "v": 1}]}}]}]})); }
fn hash(v: &Value) -> String { kimo_core::deps::sha256_hex(serde_json::to_string(v).unwrap().as_bytes()) }
fn track(ss: &mut Session) -> String { c(ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string() }
#[test] fn default_result_is_compact_and_has_no_expansion_array() {
    let mut ss = Session::new(); define(&mut ss); let t = track(&mut ss);
    let r = c(&mut ss, "template_apply", json!({"name": "seg", "args": {"track": t}})); let n = serde_json::to_string(&r).unwrap().len();
    println!("TEMPLATE_DEFAULT_BYTES {n}"); assert!(r.get("expansion").is_none() && r.get("ops").is_none(), "{r}");
    assert_eq!(r["ok"], true); assert_eq!(r["template"], "seg"); assert_eq!(r["applied_ops"], 64); assert!(r["expansion_hash"].as_str().unwrap().len() == 64);
    assert!(n < 1024, "default result {n} bytes");
}
#[test] fn expansion_hash_equals_hash_of_expand_only_ops_and_flag_returns_the_same_ops() {
    let mut ss = Session::new(); define(&mut ss); let t = track(&mut ss); let a = json!({"track": t});
    let eo = c(&mut ss, "template_apply", json!({"name": "seg", "args": a, "expand_only": true})); let ops = eo["ops"].clone(); assert_eq!(ops.as_array().unwrap().len(), 64);
    assert_eq!(eo["expansion_hash"], hash(&ops)); assert!(eo.get("expansion").is_none());
    let before = serde_json::to_string(&ss.project).unwrap();
    let dry = c(&mut ss, "template_apply", json!({"name": "seg", "args": a, "dry_run": true})); assert_eq!(dry["expansion"], ops); assert_eq!(dry["expansion_hash"], eo["expansion_hash"]); assert_eq!(before, serde_json::to_string(&ss.project).unwrap());
    let r = c(&mut ss, "template_apply", json!({"name": "seg", "args": a, "expansion": true})); assert_eq!(r["expansion"], ops); assert_eq!(r["expansion_hash"], hash(&ops));
    let r2 = c(&mut ss, "template_apply", json!({"name": "seg", "args": a, "expansion": false, "return": "minimal"})); assert!(r2.get("expansion").is_none()); assert_eq!(r2["expansion_hash"], hash(&ops));
}
#[test] fn recipe_apply_default_shape_is_the_pre_u24_shape_and_flag_matches_expand_only() {
    for n in ["lower_third", "title_card", "stat_counter"] {
        let p = match n { "lower_third" => json!({"title": "A"}), "title_card" => json!({"title": "A"}), _ => json!({"value": "10"}) };
        let (mut ss, t) = fresh();
        let eo = c(&mut ss, "recipe_apply", json!({"name": n, "start": 0.0, "params": p, "track_id": t, "expand_only": true})); let ops = eo["ops"].clone(); assert_eq!(eo["expansion_hash"], hash(&ops), "{n}");
        let dry = c(&mut ss, "recipe_apply", json!({"name": n, "start": 0.0, "params": p, "track_id": t, "dry_run": true})); assert_eq!(dry["expansion"], ops, "{n}"); assert_eq!(dry["expansion_hash"], eo["expansion_hash"]);
        let r = c(&mut ss, "recipe_apply", json!({"name": n, "start": 0.0, "params": p, "track_id": t})); assert!(r.get("expansion").is_none(), "{n}"); assert_eq!(r["expansion_hash"], eo["expansion_hash"], "{n}");
        let mut keys: Vec<&str> = r.as_object().unwrap().keys().map(|s| s.as_str()).filter(|k| *k != "expansion_hash" && *k != "history_label").collect(); keys.sort();
        let (mut ss2, t2) = fresh(); let r2 = c(&mut ss2, "recipe_apply", json!({"name": n, "start": 0.0, "params": p, "track_id": t2, "expansion": true})); assert_eq!(r2["expansion"], ops, "{n}");
        assert_eq!(keys, vec!["anchors", "applied", "created", "dry_run", "elapsed_ms", "ok", "recipe", "results", "revision", "roles", "track_id", "warnings"], "{n}");
    }
}
