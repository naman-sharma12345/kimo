//! RV-22: seven tools used to accept any string for an enum argument and silently fall back to the renderer's default arm. They now reject at tool input
//! with the allowed list; saved projects holding old junk still load and render exactly as before; the previous valid defaults still work.
use kimo_core::commands::{dispatch, dispatch_ex, registry_ref, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn base() -> (Session, String, String, String) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 160, "height": 90, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let tx = c(&mut s, "add_text", json!({"track_id": t, "text": "Hi", "start": 0, "duration": 2}))["clip_id"].as_str().unwrap().to_string();
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let sh = c(&mut s, "add_shape", json!({"track_id": t2, "shape": "rect", "width": 40, "height": 40, "start": 0, "duration": 2}))["clip_id"].as_str().unwrap().to_string(); (s, t, tx, sh) }
fn enum_of(tool: &str, prop: &str) -> Vec<String> { registry_ref().iter().find(|x| x.name == tool).unwrap().schema.pointer(&format!("/properties/{prop}/enum")).unwrap().as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect() }
#[test] fn seven_tools_reject_bogus_enum_values_with_the_allowed_list() {
    let (mut s, t, tx, sh) = base();
    let calls: Vec<(&str, &str, Value)> = vec![
        ("add_text", "align", json!({"track_id": t, "text": "x", "start": 0, "duration": 1, "align": "bogus"})),
        ("set_mask", "shape", json!({"clip_id": sh, "shape": "bogus"})),
        ("add_particles", "kind", json!({"track_id": t, "kind": "bogus", "start": 0, "duration": 1})),
        ("add_text_animator", "unit", json!({"clip_id": tx, "preset": "reveal", "unit": "bogus"})),
        ("add_transition", "ease", json!({"clip_id": sh, "kind": "crossfade", "duration": 0.5, "ease": "bogus"})),
        ("add_transition", "kind", json!({"clip_id": sh, "kind": "bogus", "duration": 0.5})),
        ("set_blend", "mode", json!({"clip_id": sh, "mode": "bogus"}))];
    for (tool, prop, a) in calls {
        let before = serde_json::to_string(&s.project).unwrap(); let e = dispatch_ex(&mut s, tool, &a).expect_err(&format!("{tool}.{prop} accepted a bogus value"));
        let allowed = enum_of(tool, prop); assert_eq!(e.code, "invalid_value", "{tool}: {e:?}"); assert_eq!(e.param.as_deref(), Some(prop)); assert_eq!(e.allowed_total, Some(allowed.len()), "{tool}.{prop}");
        for v in &allowed { assert!(e.message.contains(v.as_str()), "{tool}.{prop}: message misses {v}: {}", e.message); }
        assert_eq!(before, serde_json::to_string(&s.project).unwrap(), "{tool}.{prop}: a rejected call must not change the project"); }
}
#[test] fn valid_values_defaults_and_aliases_still_work() {
    let (mut s, t, _tx, sh) = base();
    for al in ["left", "center", "right", "centre", "LEFT"] { c(&mut s, "add_text", json!({"track_id": t, "text": "x", "start": 0, "duration": 1, "align": al})); }
    c(&mut s, "add_text", json!({"track_id": t, "text": "x", "start": 0, "duration": 1}));
    assert!(serde_json::to_string(&s.project).unwrap().matches("\"align\":\"center\"").count() >= 3, "default and 'centre' are stored as center");
    for m in enum_of("set_blend", "mode") { c(&mut s, "set_blend", json!({"clip_id": sh, "mode": m})); }
    for k in enum_of("add_transition", "kind") { c(&mut s, "add_transition", json!({"clip_id": sh, "kind": k, "duration": 0.5})); }
    for e in enum_of("add_transition", "ease") { c(&mut s, "add_transition", json!({"clip_id": sh, "kind": "crossfade", "duration": 0.5, "ease": e})); }
    c(&mut s, "add_transition", json!({"clip_id": sh, "kind": "dissolve", "duration": 0.5})); assert!(enum_of("add_transition", "kind").contains(&"dissolve".to_string()), "dissolve is accepted by the renderer, so it is in the schema");
    for k in enum_of("add_particles", "kind") { c(&mut s, "add_particles", json!({"track_id": t, "kind": k, "start": 0, "duration": 1})); }
    for sh_ in enum_of("set_mask", "shape") { c(&mut s, "set_mask", json!({"clip_id": sh, "shape": sh_})); }
}
#[test] fn saved_projects_with_junk_values_still_load_and_render_as_before() {
    let (mut s, _t, _tx, sh) = base(); c(&mut s, "set_blend", json!({"clip_id": sh, "mode": "add"}));
    let p = std::env::temp_dir().join(format!("kimo_enum_{}.kimo", std::process::id())); let ps = p.to_str().unwrap(); c(&mut s, "save_project", json!({"path": ps}));
    let good = std::fs::read_to_string(&p).unwrap(); let want = render_frame(&s.project, 0.5, &Cache::new()).unwrap();
    let junk = good.replace("\"blend\": \"add\"", "\"blend\": \"bogus_old_value\"").replace("\"align\": \"center\"", "\"align\": \"bogus_old_value\""); assert_ne!(good, junk, "fixture must contain the fields it corrupts");
    std::fs::write(&p, &junk).unwrap(); let mut u = Session::new(); c(&mut u, "load_project", json!({"path": ps})); let got = render_frame(&u.project, 0.5, &Cache::new()).unwrap();
    // junk blend renders as normal (the old fallback), junk align as center; the text clip is unchanged because "center" is the old fallback too
    let mut n = Session::new(); c(&mut n, "load_project", json!({"path": ps})); let _ = want; assert!(got.d.len() == want.d.len());
    let mut s2 = Session::new(); std::fs::write(&p, good.replace("\"blend\": \"add\"", "\"blend\": \"normal\"")).unwrap(); c(&mut s2, "load_project", json!({"path": ps})); let normal = render_frame(&s2.project, 0.5, &Cache::new()).unwrap();
    assert!(got.d == normal.d, "junk blend must render exactly like the old default (normal)"); let _ = std::fs::remove_file(&p);
}
#[test] fn dissolve_renders_identically_to_crossfade() {
    // the renderer treats "dissolve" as an alias of "crossfade"; widening the schema must not change pixels
    let frame = |kind: &str| { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 160, "height": 90, "fps": 24})); let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
        c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.9, 0.1, 0.1], "start": 0, "duration": 2}));
        let t2 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let cl = c(&mut s, "add_clip", json!({"track_id": t2, "source_type": "solid", "color": [0.1, 0.2, 0.9], "start": 0.5, "duration": 1.5}))["clip_id"].as_str().unwrap().to_string();
        c(&mut s, "add_transition", json!({"clip_id": cl, "kind": kind, "duration": 1.0})); render_frame(&s.project, 1.0, &Cache::new()).unwrap().d };
    let (a, b) = (frame("crossfade"), frame("dissolve")); assert!(a == b, "dissolve and crossfade must render bit-identically");
    let mid = a.iter().step_by(4).copied().fold(0f32, f32::max); assert!(mid > 0.05, "the transition frame must contain something (not a blank-equal pass)");
}
#[test] fn normalised_aliases_render_like_their_canonical_value() {
    // "centre" and "LEFT" are stored as center / left; an old project that stored "centre" renders as center (the old fallback) - same pixels
    let frame = |al: &str| { let (mut s, t, _, _) = base(); let id = c(&mut s, "add_text", json!({"track_id": t, "text": "Hello", "start": 0, "duration": 1, "align": al}))["clip_id"].as_str().unwrap().to_string(); let _ = id; render_frame(&s.project, 0.5, &Cache::new()).unwrap().d };
    assert!(frame("centre") == frame("center")); assert!(frame("LEFT") == frame("left"));
}
