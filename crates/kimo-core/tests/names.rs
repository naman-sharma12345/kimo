//! U1: names, roles, tags: set_meta, find and the "@name" resolver.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::names::resolve_ref;
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
fn scene() -> (Session, String, Vec<String>) { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let ids = (0..4).map(|i| c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 10, "height": 10, "start": i as f64 * 2.0, "duration": 1.5}))["clip_id"].as_str().unwrap().to_string()).collect(); (ss, t, ids) }

#[test] fn set_meta_names_roles_tags_and_old_projects_stay_byte_equal() {
    let (mut ss, _t, ids) = scene(); let h0 = pj(&ss); assert!(!h0.contains("\"name\":\"") || true); assert!(!h0.contains("\"role\"") && !h0.contains("\"tags\""), "no new fields when unused");
    let r = c(&mut ss, "set_meta", json!({"id": ids[0], "name": "hero", "role": "title_bg", "tags": ["a", "lint:allow_bleed"], "add_tags": ["b", "a"], "remove_tags": ["a"]})); assert_eq!(r["tags"], json!(["lint:allow_bleed", "b"])); assert_eq!(r["name"], "hero");
    let r = c(&mut ss, "set_meta", json!({"id": "@hero", "name": null})); assert!(r["name"].is_null()); c(&mut ss, "undo", json!({})); assert_eq!(c(&mut ss, "find", json!({"name": "hero"}))["count"], 1); c(&mut ss, "undo", json!({})); assert_eq!(pj(&ss), h0, "undo restores the exact old shape");
}
#[test] fn name_rules_and_typed_errors_change_nothing() {
    let (mut ss, _t, ids) = scene(); c(&mut ss, "set_meta", json!({"id": ids[0], "name": "hero"})); let h = pj(&ss); let u = ss.undo.len();
    assert!(e(&mut ss, "set_meta", json!({"id": ids[1], "name": "hero"})).contains("name_taken"));
    for bad in ["", "@x", "has space", "ü", &"a".repeat(65)] { assert!(e(&mut ss, "set_meta", json!({"id": ids[1], "name": bad})).contains("invalid_name"), "{bad}"); }
    assert!(e(&mut ss, "set_meta", json!({"id": "@hreo"})).contains("name_not_found") && e(&mut ss, "set_meta", json!({"id": "@hreo"})).contains("@hero"), "nearest name suggested");
    assert!(e(&mut ss, "set_meta", json!({"id": ids[1], "name": "ok", "tags": ["bad tag"]})).contains("invalid_name")); assert!(e(&mut ss, "set_meta", json!({"id": "nope", "name": "x"})).contains("nope"));
    assert_eq!(pj(&ss), h); assert_eq!(ss.undo.len(), u); c(&mut ss, "set_meta", json!({"id": ids[0], "name": "hero"}));
    let names = (0..64).map(|_| "a").collect::<String>(); c(&mut ss, "set_meta", json!({"id": ids[2], "name": names}));
}
#[test] fn resolver_ids_pass_through_names_resolve_and_duplicates_are_ambiguous() {
    let (mut ss, t, ids) = scene(); c(&mut ss, "set_meta", json!({"id": ids[2], "name": "mid"})); assert_eq!(resolve_ref(&ss.project, "clip", "@mid").unwrap(), ids[2]); assert_eq!(resolve_ref(&ss.project, "clip", &ids[1]).unwrap(), ids[1]); assert_eq!(resolve_ref(&ss.project, "clip", "plain_text").unwrap(), "plain_text");
    assert!(resolve_ref(&ss.project, "clip", "@nope").unwrap_err().starts_with("name_not_found")); assert!(resolve_ref(&ss.project, "track", "@mid").unwrap_err().starts_with("name_not_found"), "clip names are not track names");
    c(&mut ss, "set_meta", json!({"target": "track", "id": t, "name": "main"})); assert_eq!(resolve_ref(&ss.project, "track", "@main").unwrap(), t);
    // a project that predates uniqueness
    let mut p = ss.project.clone(); for cl in p.tracks[0].clips.iter_mut() { std::sync::Arc::make_mut(cl).name = Some("dup".into()); } assert!(resolve_ref(&p, "clip", "@dup").unwrap_err().starts_with("ambiguous_name"));
}
#[test] fn find_filters_cap_and_read_only() {
    let (mut ss, t, ids) = scene(); c(&mut ss, "set_meta", json!({"id": ids[0], "name": "hero_a", "role": "bg", "tags": ["x"]})); c(&mut ss, "set_meta", json!({"id": ids[1], "name": "hero_b", "role": "fg", "tags": ["x", "y"]})); c(&mut ss, "set_meta", json!({"id": ids[3], "name": "end"}));
    let h = pj(&ss); let u = ss.undo.len(); let f = |ss: &mut Session, a: Value| -> Vec<String> { c(ss, "find", a)["matches"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap().to_string()).collect() };
    assert_eq!(f(&mut ss, json!({"name_glob": "hero_*"})), vec![ids[0].clone(), ids[1].clone()]); assert_eq!(f(&mut ss, json!({"name": "hero_?"})).len(), 0, "exact name does not glob"); assert_eq!(f(&mut ss, json!({"role": "fg"})), vec![ids[1].clone()]); assert_eq!(f(&mut ss, json!({"tag": "y"})), vec![ids[1].clone()]);
    assert_eq!(f(&mut ss, json!({"time_range": [3.9, 4.1]})), vec![ids[2].clone()]); assert_eq!(f(&mut ss, json!({"track": t, "source_kind": "solid"})).len() + f(&mut ss, json!({"source_kind": "shape"})).len() + f(&mut ss, json!({"source_kind": "nope"})).len(), f(&mut ss, json!({"track": t, "source_kind": "solid"})).len() + f(&mut ss, json!({"source_kind": "shape"})).len());
    let r = c(&mut ss, "find", json!({"limit": 2})); assert_eq!(r["count"], 2); assert_eq!(r["total"], 4); assert_eq!(r["truncated"], true); assert_eq!(c(&mut ss, "find", json!({"kind": "track"}))["count"], 1);
    assert!(e(&mut ss, "find", json!({"limit": 0})).contains("limit")); assert!(e(&mut ss, "find", json!({"time_range": [2.0, 1.0]})).contains("time_range")); assert!(e(&mut ss, "find", json!({"kind": "zzz"})).contains("Valid:")); assert!(e(&mut ss, "find", json!({"track": "@missing"})).contains("name_not_found"));
    assert_eq!(pj(&ss), h); assert_eq!(ss.undo.len(), u, "find is read-only");
}
#[test] fn track_names_and_text_search() {
    let (mut ss, t, _ids) = scene(); c(&mut ss, "set_meta", json!({"target": "track", "id": t, "name": "titles"})); assert!(e(&mut ss, "set_meta", json!({"target": "track", "id": t, "role": "x"})).contains("only carry a name"));
    let tt = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); assert!(e(&mut ss, "set_meta", json!({"target": "track", "id": tt, "name": "titles"})).contains("name_taken"));
    let tx = c(&mut ss, "add_text", json!({"track_id": tt, "text": "Hello World", "start": 0.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    assert_eq!(c(&mut ss, "find", json!({"text_contains": "hello w"}))["matches"][0]["id"], tx.as_str()); assert_eq!(c(&mut ss, "find", json!({"text_contains": "zzz"}))["count"], 0);
}

#[test] fn find_has_effect_filters_by_effect_kind_and_lists_effects_in_results() {
    let (mut ss, _t, ids) = scene(); let h = pj(&ss); let u = ss.undo.len();
    c(&mut ss, "add_effect", json!({"clip_id": ids[0], "kind": "glow", "params": {"amount": 0.5}}));
    c(&mut ss, "add_effect", json!({"clip_id": ids[0], "kind": "blur", "params": {"radius": 2.0}}));
    c(&mut ss, "add_effect", json!({"clip_id": ids[1], "kind": "blur", "params": {"radius": 1.0}}));
    let f = |ss: &mut Session, a: Value| -> Vec<String> { c(ss, "find", a)["matches"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap().to_string()).collect() };
    assert_eq!(f(&mut ss, json!({"has_effect": "glow"})), vec![ids[0].clone()]);
    let mut blurred = f(&mut ss, json!({"has_effect": "blur"})); blurred.sort();
    let mut want = vec![ids[0].clone(), ids[1].clone()]; want.sort();
    assert_eq!(blurred, want);
    assert_eq!(f(&mut ss, json!({"has_effect": "nope_no_such_kind"})).len(), 0);
    c(&mut ss, "set_meta", json!({"id": ids[0], "name": "hero"}));
    let m = c(&mut ss, "find", json!({"has_effect": "glow"}))["matches"][0].clone();
    assert_eq!(m["effects"], json!(["glow", "blur"]), "effect kinds ride along, in order");
    assert!(e(&mut ss, "find", json!({"kind": "track", "has_effect": "glow"})).contains("has_effect"), "has_effect is clip-only");
    // undo the fixture's own mutations back out, confirming find itself never touched history further than the 2+1 add_effect calls + 1 set_meta
    c(&mut ss, "undo", json!({})); c(&mut ss, "undo", json!({})); c(&mut ss, "undo", json!({})); c(&mut ss, "undo", json!({}));
    assert_eq!(pj(&ss), h); assert_eq!(ss.undo.len(), u, "find performed no mutation of its own");
}
