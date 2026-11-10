//! U3a: dependency-hashed export manifests. Each row edits the project and asks explain_invalidation which chunks went stale.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn id(v: &Value) -> String { v["clip_id"].as_str().unwrap().to_string() }
/// 25 fps, 36 s, chunks of 4 s = 9 chunks. Track 1: shape A 0-12 s, shape B 12-24 s. Track 2: text T 28-36 s.
fn scene() -> (Session, String, String, String, String) {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t1 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let t2 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = id(&c(&mut ss, "add_shape", json!({"track_id": t1, "shape": "rect", "width": 20, "height": 20, "start": 0.0, "duration": 12.0})));
    let b = id(&c(&mut ss, "add_shape", json!({"track_id": t1, "shape": "rect", "width": 30, "height": 20, "start": 12.0, "duration": 12.0})));
    let t = id(&c(&mut ss, "add_text", json!({"track_id": t2, "text": "hello", "start": 28.0, "duration": 8.0})));
    (ss, t1, a, b, t)
}
fn man(ss: &mut Session) -> Value { c(ss, "export_manifest", json!({"chunk_s": 4})) }
fn stale(ss: &mut Session, prev: &Value) -> (Vec<u64>, Value) { let r = c(ss, "explain_invalidation", json!({"previous": prev})); let v = r["chunks"].as_array().unwrap().iter().filter(|x| x["stale"] == true).map(|x| x["index"].as_u64().unwrap()).collect(); (v, r) }

#[test] fn manifest_has_nine_chunks_and_is_deterministic() {
    let (mut ss, ..) = scene(); let m = man(&mut ss); assert_eq!(m["chunks"].as_array().unwrap().len(), 9); assert_eq!(m["chunks"][0]["range"], json!([0, 100]));
    assert_eq!(m["chunks"][8]["range"], json!([800, 900])); assert_eq!(m, man(&mut ss), "same project, same manifest");
    let (s, r) = stale(&mut ss, &m); assert!(s.is_empty()); assert_eq!(r["unchanged"], 9);
}
#[test] fn opacity_key_on_one_clip_stales_only_its_chunks() {
    let (mut ss, _, a, ..) = scene(); let m = man(&mut ss);
    c(&mut ss, "set_keyframes", json!({"clip_id": a, "property": "opacity", "keys": [{"t": 0.0, "v": 1.0}, {"t": 6.0, "v": 0.5}]}));
    let (s, r) = stale(&mut ss, &m); assert_eq!(s, vec![0, 1, 2]); assert_eq!(r["unchanged"], 6);
    let why = &r["chunks"][1]["reason"][0]; assert_eq!(why["kind"], "clip_changed"); assert_eq!(why["clip"], json!(a)); assert!(why["property"].as_array().unwrap().iter().any(|p| p == "props"), "{why}");
}
#[test] fn text_edit_stales_only_its_range() {
    let (mut ss, _, _, _, t) = scene(); let m = man(&mut ss); c(&mut ss, "set_text", json!({"clip_id": t, "text": "changed"}));
    let (s, _) = stale(&mut ss, &m); assert_eq!(s, vec![7, 8]);
}
#[test] fn unrelated_edits_stale_nothing() {
    let (mut ss, t1, ..) = scene(); let m = man(&mut ss);
    c(&mut ss, "add_marker", json!({"time": 5.0})); c(&mut ss, "set_track", json!({"track_id": t1, "locked": true, "name": "renamed"}));
    let (s, r) = stale(&mut ss, &m); assert!(s.is_empty(), "markers, track lock and track rename must not invalidate: {r}"); assert_eq!(m["manifest_hash"], man(&mut ss)["manifest_hash"]);
}
#[test] fn reparenting_changes_the_childs_dep_hash_and_names_the_parent_part() {
    let (mut ss, t1, _, b, _) = scene(); let n = id(&c(&mut ss, "add_null", json!({"track_id": t1, "start": 12.0, "duration": 12.0})));
    let m0 = man(&mut ss); c(&mut ss, "set_parent", json!({"clip_id": b, "parent_id": n})); let m1 = man(&mut ss);
    assert_ne!(m0["clips"][&b]["dep"], m1["clips"][&b]["dep"]); assert!(m1["clips"][&b]["parts"].as_object().unwrap().contains_key("parent"));
    let m2 = m1.clone(); c(&mut ss, "set_property", json!({"clip_id": n, "property": "x", "value": 9.0})); let m3 = man(&mut ss);
    assert_ne!(m2["clips"][&b]["dep"], m3["clips"][&b]["dep"], "moving the parent changes the child's dep hash");
    let (s, _) = stale(&mut ss, &m2); assert_eq!(s, vec![3, 4, 5]);
}
#[test] fn replacing_an_asset_file_at_the_same_path_stales_the_chunks_using_it() {
    let dir = std::env::temp_dir().join(format!("kimo_u3_{}", std::process::id())); std::fs::create_dir_all(&dir).unwrap(); let p = dir.join("a.mp4");
    let mk = |color: &str| { assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i"]).arg(format!("color=c={color}:s=64x36:r=25:d=2")).args(["-pix_fmt", "yuv420p"]).arg(&p).status().unwrap().success()); };
    mk("red"); let (mut ss, _, _, _, _) = scene(); let t3 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let asset = c(&mut ss, "import_media", json!({"path": p.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": t3, "source_type": "media", "asset_id": asset, "start": 24.0, "duration": 2.0}));
    let m = man(&mut ss); assert!(m["assets"].as_array().unwrap().iter().any(|a| a["hash"].as_str().map_or(false, |h| h.len() == 64)));
    let (s0, _) = stale(&mut ss, &m); assert!(s0.is_empty());
    mk("blue"); let (s, r) = stale(&mut ss, &m); assert_eq!(s, vec![6]); assert_eq!(r["chunks"][6]["reason"][0]["kind"], "asset_changed", "{r}");
    std::fs::remove_dir_all(&dir).ok();
}
#[test] fn save_and_reload_gives_identical_hashes() {
    let (mut ss, ..) = scene(); let m = man(&mut ss); let txt = serde_json::to_string(&ss.project).unwrap();
    let mut s2 = Session::new(); s2.project = serde_json::from_str(&txt).unwrap(); let m2 = man(&mut s2);
    assert_eq!(m["manifest_hash"], m2["manifest_hash"]); assert_eq!(m["project_hash"], m2["project_hash"]);
}
#[test] fn engine_or_settings_change_stales_everything_with_that_reason() {
    let (mut ss, ..) = scene(); let m = man(&mut ss); let mut old = m.clone(); old["engine"]["version"] = json!("0.0.0-other");
    let r = c(&mut ss, "explain_invalidation", json!({"previous": old})); assert_eq!(r["stale"], 9); assert_eq!(r["chunks"][4]["reason"][0]["kind"], "engine");
    c(&mut ss, "project_set", json!({"width": 320, "height": 180})); let r = c(&mut ss, "explain_invalidation", json!({"previous": m})); assert_eq!(r["stale"], 9); assert_eq!(r["chunks"][0]["reason"][0]["kind"], "settings");
}
#[test] fn adding_and_removing_clips_are_named() {
    let (mut ss, _, _, b, _) = scene(); let m = man(&mut ss); let t = ss.project.tracks[0].id.clone();
    let n = id(&c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 5, "height": 5, "start": 24.0, "duration": 4.0})));
    let (s, r) = stale(&mut ss, &m); assert_eq!(s, vec![6]); assert_eq!(r["chunks"][6]["reason"][0], json!({"kind": "clip_added", "clip": n}));
    let m2 = man(&mut ss); c(&mut ss, "delete_clip", json!({"clip_id": b})); let (s, r) = stale(&mut ss, &m2); assert_eq!(s, vec![3, 4, 5]); assert_eq!(r["chunks"][3]["reason"][0], json!({"kind": "clip_removed", "clip": b}));
}
#[test] fn bad_arguments_are_named_errors() {
    let (mut ss, ..) = scene(); assert!(dispatch(&mut ss, "export_manifest", &json!({"chunk_s": 0})).unwrap_err().contains("chunk_s"));
    assert!(dispatch(&mut ss, "export_manifest", &json!({"range": [5, 1]})).unwrap_err().contains("range"));
    assert!(dispatch(&mut ss, "explain_invalidation", &json!({})).unwrap_err().contains("previous"));
}
#[test] fn manifest_of_a_two_minute_project_takes_milliseconds() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 24})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    for i in 0..60 { c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 10, "height": 10, "start": i as f64 * 2.0, "duration": 2.0})); }
    let t0 = std::time::Instant::now(); let m = man(&mut ss); let ms = t0.elapsed().as_secs_f64() * 1000.0; assert_eq!(m["chunks"].as_array().unwrap().len(), 30); eprintln!("U3_MANIFEST_MS {ms:.2} (gate profile, 60 clips, 2 min, 24 fps)");
    assert!(ms < 2000.0);
}
/// U2.4 interaction: batch templates are authoring data, never pixels. Defining one must not stale a single chunk or move the project hash.
#[test] fn defining_a_batch_template_changes_no_chunk_key_and_no_project_hash() {
    let (mut ss, ..) = scene(); let before = c(&mut ss, "export_manifest", json!({"chunk_s": 4}));
    c(&mut ss, "template_define", json!({"name": "t1", "ops": [{"tool": "find", "args": {}}]}));
    let after = c(&mut ss, "export_manifest", json!({"chunk_s": 4}));
    assert_eq!(before["project_hash"], after["project_hash"]); assert_eq!(before["chunks"], after["chunks"]);
    let ex = c(&mut ss, "explain_invalidation", json!({"previous": before})); assert_eq!(ex["stale"], 0, "{ex}");
}
