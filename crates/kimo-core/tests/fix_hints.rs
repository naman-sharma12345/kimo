//! U-FIX: lint_frame fix_hint + next_steps. Hints are advisory; applying their args through dispatch must clear the finding.
use kimo_core::commands::{dispatch, find_command, Session};
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn scene(w: u32, h: u32) -> (Session, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":w,"height":h,"fps":24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(&mut s, "add_shape", json!({"track_id":t,"shape":"rect","width":40,"height":20,"start":0,"duration":6}))["clip_id"].as_str().unwrap().to_string();
    c(&mut s, "set_meta", json!({"id":id,"name":"plate","role":"foreground"})); (s, id)
}
fn text_scene(color: [f64; 3], size: f64, w: u32, h: u32) -> (Session, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":w,"height":h,"fps":25}));
    let bg = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); c(&mut s, "add_clip", json!({"track_id":bg,"source_type":"solid","color":[1,1,1],"start":0,"duration":3}));
    let tr = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(&mut s, "add_text", json!({"track_id":tr,"text":"H title","font":"DejaVuSans","size":size,"color":color,"start":0,"duration":3}))["clip_id"].as_str().unwrap().to_string();
    c(&mut s, "set_meta", json!({"id":id,"name":"title","role":"title"})); (s, id)
}
fn lint(s: &mut Session, checks: Value) -> Value { c(s, "lint_frame", json!({"time":1,"checks":checks,"annotate":false})) }
fn rows(r: &Value) -> Vec<Value> { r["frames"].as_array().unwrap().iter().flat_map(|f| f["findings"].as_array().unwrap().clone()).collect() }
/// Validate hint args against the live tool schema: tool exists, required present, no unknown property, enum and basic types.
fn validate(tool: &str, args: &Value) {
    let cmd = find_command(tool).unwrap_or_else(|| panic!("hint names unknown tool {tool}")); let sc = &cmd.schema; let props = sc["properties"].as_object().unwrap();
    for r in sc["required"].as_array().cloned().unwrap_or_default() { assert!(args.get(r.as_str().unwrap()).is_some(), "{tool}: missing required {r}"); }
    for (k, v) in args.as_object().unwrap() { let p = props.get(k).unwrap_or_else(|| panic!("{tool}: unknown arg {k}"));
        if let Some(e) = p["enum"].as_array() { assert!(e.contains(v), "{tool}.{k}: {v} not in {e:?}"); }
        match p["type"].as_str() { Some("number") => assert!(v.is_number(), "{tool}.{k} must be a number"), Some("string") => assert!(v.is_string(), "{tool}.{k}"), _ => {} } }
}
fn check_hint(f: &Value) { let h = &f["fix_hint"]; assert!(h.is_object(), "every finding needs a fix_hint: {f}"); let k = h["kind"].as_str().unwrap();
    assert!(kimo_core::perception::fix_hint::KINDS.contains(&k), "{k}");
    if k == "none" { assert!(!h["why"].as_str().unwrap().is_empty()); } else { validate(h["tool"].as_str().unwrap(), &h["args"]); assert!(h["why"].as_str().unwrap().chars().any(|c| c.is_ascii_digit()), "why carries a measured number: {h}"); assert!(["exact", "estimate"].contains(&h["confidence"].as_str().unwrap())); } }
fn apply_steps(s: &mut Session, r: &Value) { for st in r["next_steps"].as_array().unwrap() { validate(st["tool"].as_str().unwrap(), &st["args"]); c(s, st["tool"].as_str().unwrap(), st["args"].clone()); } }
#[test] fn out_of_bounds_one_axis_hint_clears_the_finding() {
    let (mut s, id) = scene(160, 90); c(&mut s, "set_property", json!({"clip_id":id,"property":"x","value":70}));
    let r = lint(&mut s, json!(["out_of_bounds"])); let f = rows(&r); assert_eq!(f.len(), 1); check_hint(&f[0]); assert_eq!(f[0]["fix_hint"]["tool"], "set_property", "{}", f[0]); assert_eq!(f[0]["fix_hint"]["args"]["property"], "x");
    assert_eq!(r["next_steps"].as_array().unwrap().len(), 1);
    let (tool, args) = (f[0]["fix_hint"]["tool"].as_str().unwrap().to_string(), f[0]["fix_hint"]["args"].clone()); c(&mut s, &tool, args);
    assert!(rows(&lint(&mut s, json!(["out_of_bounds"]))).is_empty(), "applying the hint clears the finding");
}
#[test] fn out_of_bounds_two_axes_next_steps_clear_the_finding_and_are_capped_at_3() {
    let (mut s, id) = scene(160, 90); c(&mut s, "set_property", json!({"clip_id":id,"property":"x","value":-70})); c(&mut s, "set_property", json!({"clip_id":id,"property":"y","value":45}));
    let r = lint(&mut s, json!(["out_of_bounds"])); assert_eq!(rows(&r).len(), 1); check_hint(&rows(&r)[0]); assert_eq!(r["next_steps"].as_array().unwrap().len(), 2); assert!(r["next_steps"].as_array().unwrap().len() <= 3);
    apply_steps(&mut s, &r); assert!(rows(&lint(&mut s, json!(["out_of_bounds"]))).is_empty());
}
#[test] fn invisible_layer_and_animated_property_and_static_frame_hints() {
    let (mut s, id) = scene(160, 90); c(&mut s, "set_property", json!({"clip_id":id,"property":"opacity","value":0}));
    let r = lint(&mut s, json!(["invisible_layer"])); let f = rows(&r); assert_eq!(f.len(), 1); check_hint(&f[0]); apply_steps(&mut s, &r); assert!(rows(&lint(&mut s, json!(["invisible_layer"]))).is_empty());
    let (mut s, id) = scene(160, 90); c(&mut s, "set_property", json!({"clip_id":id,"property":"x","value":70}));
    c(&mut s, "set_keyframes", json!({"clip_id":id,"property":"x","keys":[{"t":0,"v":70},{"t":5,"v":70}]})); let f = rows(&lint(&mut s, json!(["out_of_bounds"]))); assert_eq!(f[0]["fix_hint"]["kind"], "none"); assert!(f[0]["fix_hint"]["why"].as_str().unwrap().contains("animated"));
    let (mut s, _) = scene(160, 90); let r = c(&mut s, "lint_frame", json!({"range":{"start":0,"end":5,"step":0.5},"checks":["static_frame"],"annotate":false})); assert_eq!(r["intervals"][0]["fix_hint"]["kind"], "none");
}
#[test] fn every_emitted_lint_code_has_a_valid_hint_or_an_explicit_none() {
    let mut seen = std::collections::BTreeSet::new(); let mut extra: Vec<String> = vec![];
    let mut run = |s: &mut Session, checks: Value| { let r = lint(s, checks); for f in rows(&r) { check_hint(&f); seen.insert(f["code"].as_str().unwrap().to_string()); } for iv in r["intervals"].as_array().unwrap() { check_hint(iv); seen.insert(iv["code"].as_str().unwrap().to_string()); } r };
    let (mut s, id) = scene(160, 90); c(&mut s, "set_property", json!({"clip_id":id,"property":"x","value":70})); run(&mut s, json!(["out_of_bounds"]));
    let (mut s, id) = scene(160, 90); c(&mut s, "set_property", json!({"clip_id":id,"property":"opacity","value":0})); run(&mut s, json!(["invisible_layer"]));
    let (mut s, _) = scene(160, 90); let st = c(&mut s, "lint_frame", json!({"range":{"start":0,"end":5,"step":0.5},"checks":["static_frame"],"annotate":false})); for iv in st["intervals"].as_array().unwrap() { check_hint(iv); extra.push(iv["code"].as_str().unwrap().to_string()); }
    let (mut s, _) = text_scene([0.95, 0.95, 0.95], 40.0, 320, 180); run(&mut s, json!(["text_contrast"]));
    let (mut s, _) = text_scene([0., 0., 0.], 3.0, 320, 180); run(&mut s, json!(["min_text_size"]));
    let (mut s, id) = text_scene([0., 0., 0.], 40.0, 320, 180); c(&mut s, "set_property", json!({"clip_id":id,"property":"x","value":300})); run(&mut s, json!(["text_overflow"]));
    seen.extend(extra);
    for code in ["out_of_bounds", "invisible_layer", "static_frame", "text_contrast", "min_text_size", "text_overflow"] { assert!(seen.contains(code), "scene for {code} produced no finding: {seen:?}"); }
}
#[test] fn min_text_size_and_text_overflow_hints_clear_in_a_closed_loop() {
    let (mut s, _) = text_scene([0., 0., 0.], 3.0, 320, 180); let r = lint(&mut s, json!(["min_text_size"])); let f = rows(&r); assert_eq!(f.len(), 1); check_hint(&f[0]); assert_eq!(f[0]["fix_hint"]["tool"], "set_text");
    apply_steps(&mut s, &r); let r2 = lint(&mut s, json!(["min_text_size"])); assert!(rows(&r2).is_empty(), "set_text size from the hint clears min_text_size: before {} after {}", f[0], r2["frames"]);
    let (mut s, id) = text_scene([0., 0., 0.], 40.0, 320, 180); c(&mut s, "set_property", json!({"clip_id":id,"property":"x","value":300}));
    let r = lint(&mut s, json!(["text_overflow"])); check_hint(&rows(&r)[0]); apply_steps(&mut s, &r); assert!(rows(&lint(&mut s, json!(["text_overflow"]))).is_empty());
}
#[test] fn diff_frames_and_detect_cuts_carry_hints_and_valid_next_steps() {
    let (mut s, id) = scene(160, 90); c(&mut s, "set_property", json!({"clip_id":id,"property":"opacity","value":0}));
    let r = c(&mut s, "diff_frames", json!({"a":{"time":1},"b":{"time":2}})); check_hint(&r); assert_eq!(r["fix_hint"]["kind"], "none"); assert!(r["next_steps"].is_array());
    c(&mut s, "set_property", json!({"clip_id":id,"property":"opacity","value":1}));
    let r = c(&mut s, "diff_frames", json!({"a":{"source":"snapshot:undo:1","time":1},"b":{"time":1}})); check_hint(&r);
    let r = c(&mut s, "detect_cuts", json!({})); assert!(r["next_steps"].as_array().unwrap().len() <= 3); for st in r["next_steps"].as_array().unwrap() { validate(st["tool"].as_str().unwrap(), &st["args"]); } for cut in r["cuts"].as_array().unwrap() { check_hint(cut); }
}
