//! V1c-D plumbing rows: common-argument validation, typed bg_* codes, toolgroups entries, one apply/undo-group wrapper. These run without any bg_* tool.
use kimo_core::commands::{dispatch, Session};
use kimo_core::errors::classify;
use kimo_core::ext::bgcommon::{self, Common, COMMON};
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn base() -> (Session, String) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":256,"height":256,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    (s, t)
}
fn parse(s: &Session, a: Value) -> Result<Common, String> { Common::parse(s, &a, "bg_grid", "grid", &["spacing_px"]) }
#[test]
fn defaults_echo_equals_the_spec_table() {
    let (s, t) = base();
    let cm = parse(&s, json!({"track":t,"duration":5})).unwrap();
    let e = cm.echo();
    for a in COMMON { if let Some(d) = a.default { assert_eq!(e[a.name].as_f64(), Some(d), "default of {} is the table value", a.name); } }
    assert_eq!(e["blend"], "normal"); assert_eq!(e["apply"], false); assert_eq!(e["tags"], json!(["bg:grid"]));
    let props = bgcommon::common_props();
    for a in COMMON { assert!(props.iter().any(|(k, _)| *k == a.name), "schema has {}", a.name); }
    for (k, p) in &props { if let (Some(d), Some(spec)) = (p.get("default").and_then(|d| d.as_f64()), COMMON.iter().find(|a| a.name == *k)) { assert_eq!(Some(d), spec.default, "schema default of {k}"); } }
}
#[test]
fn boundaries_accept_the_edge_and_reject_just_outside() {
    let (s, t) = base();
    let ok = |extra: Value| { let mut a = json!({"track":t,"duration":5}); for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); } parse(&s, a) };
    let eps = 1e-6;
    for (k, lo, hi) in [("start", 0.0, 1e7), ("opacity", 0.0, 1.0)] {
        assert!(ok(json!({k:lo})).is_ok() && ok(json!({k:hi})).is_ok(), "{k} edges accepted");
        for bad in [lo - eps, hi + eps] { let e = ok(json!({k:bad})).err().unwrap_or_else(|| panic!("{k}={bad} must be rejected")); assert!(e.starts_with("bg_param_out_of_range:") && e.contains("fix_hint"), "{k}: {e}"); }
    }
    assert!(ok(json!({"duration":3600})).is_ok() && ok(json!({"duration":0.001})).is_ok());
    for bad in [3600.0 + eps, 0.0, -1.0] { let e = ok(json!({"duration":bad})).err().unwrap(); assert!(e.starts_with("bg_param_out_of_range:"), "duration {bad}: {e}"); }
    assert!(ok(json!({"seed":9007199254740991u64})).is_ok());
    for bad in [json!(-1), json!(1.5), json!(9007199254740992u64), json!("x")] { assert!(ok(json!({"seed":bad.clone()})).unwrap_err().starts_with("bg_param_out_of_range:"), "seed {bad}"); }
    for bad in [json!("NaN"), json!("inf")] { assert!(ok(json!({"opacity":bad.clone()})).is_err(), "opacity {bad} rejected"); }
    assert!(ok(json!({"name":"a".repeat(64)})).is_ok()); assert!(ok(json!({"name":"a".repeat(65)})).is_err()); assert!(ok(json!({"name":"bad name"})).is_err());
    assert!(ok(json!({"tags":["a","b","c","d","e","f","g","h"]})).is_ok()); assert!(ok(json!({"tags":["a","b","c","d","e","f","g","h","i"]})).is_err());
    assert!(ok(json!({"tags":["x".repeat(32)]})).is_ok()); assert!(ok(json!({"tags":["x".repeat(33)]})).is_err()); assert!(ok(json!({"tags":["a:b.c-d_e"]})).is_ok());
    for b in ["normal", "add", "screen", "multiply"] { assert!(ok(json!({"blend":b})).is_ok()); }
    assert!(ok(json!({"blend":"overlay"})).unwrap_err().contains("Valid: normal, add, screen, multiply"));
    assert!(parse(&s, json!({"duration":5})).unwrap_err().contains("track"), "track is required");
    assert!(parse(&s, json!({"track":t})).unwrap_err().contains("duration"), "duration is required");
}
#[test]
fn unknown_properties_are_rejected_listing_the_valid_names() {
    let (s, t) = base();
    let e = parse(&s, json!({"track":t,"duration":5,"colour":[1,1,1]})).unwrap_err();
    assert!(e.starts_with("bg_unknown_property:") && e.contains("'colour'") && e.contains("spacing_px") && e.contains("blend") && e.contains("fix_hint"), "{e}");
    assert!(parse(&s, json!({"track":t,"duration":5,"spacing_px":8})).is_ok(), "a tool's own property is accepted");
}
#[test]
fn every_bg_code_classifies_and_hint_survives() {
    for code in bgcommon::CODES { let m = bgcommon::bg_err(code, "x is bad", "bg_grid(x=1)"); assert_eq!(classify(&m).0, *code, "{m}"); assert!(m.contains("fix_hint: try bg_grid(x=1)")); }
}
#[test]
fn bg_tools_are_in_the_graphics_group() {
    for n in ["bg_grid", "bg_orbs", "bg_wave"] { assert_eq!(kimo_core::toolgroups::group_of(n), "graphics", "{n}"); }
}
#[test]
fn finish_applies_atomically_and_dry_run_changes_nothing() {
    let (mut s, t) = base();
    let cm = parse(&s, json!({"track":t,"duration":2,"opacity":0.5,"blend":"screen","name":"bgx","apply":true,"spacing_px":8})).unwrap();
    let mut ops = vec![json!({"id":"clip","tool":"add_clip","args":{"track_id":cm.track,"source_type":"solid","color":[0.1,0.1,0.1],"start":cm.start,"duration":cm.duration}})];
    ops.extend(cm.finish_ops("clip"));
    let before = serde_json::to_vec(&s.project).unwrap();
    let mut dry = parse(&s, json!({"track":t,"duration":2,"opacity":0.5,"blend":"screen","name":"bgx"})).unwrap(); dry.apply = false;
    let r0 = bgcommon::finish(&mut s, &dry, cm.echo(), ops.clone(), &["clip"], json!({}), vec![]).unwrap();
    assert_eq!(r0["applied"], false); assert!(r0["clip_ids"].is_null());
    assert_eq!(before, serde_json::to_vec(&s.project).unwrap(), "dry run changes nothing");
    let r = bgcommon::finish(&mut s, &cm, cm.echo(), ops, &["clip"], json!({}), vec![]).unwrap();
    assert_eq!(r["applied"], true); let id = r["clip_ids"][0].as_str().unwrap().to_string();
    let found = c(&mut s, "find", json!({"tag":"bg:grid"})); assert_eq!(found["total"], 1);
    let cl = s.project.find_clip(&id).unwrap(); assert_eq!(cl.blend, "screen");
    // The undo step itself is recorded by the outer dispatch of a registered tool (finish runs inside one), so the one-undo row lives with the first bg tool
    // (bg_tools) and with spotlight/callout_card in dim_mask/callout, which use the same dispatch_nested batch path.
}
#[test]
fn a_failing_op_leaves_no_partial_clip() {
    let (mut s, t) = base();
    let cm = parse(&s, json!({"track":t,"duration":2,"apply":true})).unwrap();
    let before = serde_json::to_vec(&s.project).unwrap();
    let ops = vec![json!({"id":"clip","tool":"add_clip","args":{"track_id":cm.track,"source_type":"solid","color":[0,0,0],"start":0,"duration":2}}), json!({"tool":"set_property","args":{"clip_id":"$clip.clip_id","property":"no_such_property","value":1}})];
    assert!(bgcommon::finish(&mut s, &cm, cm.echo(), ops, &["clip"], json!({}), vec![]).is_err());
    assert_eq!(before, serde_json::to_vec(&s.project).unwrap(), "atomic: nothing left behind");
}
