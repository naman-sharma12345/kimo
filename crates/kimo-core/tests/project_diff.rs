//! N1.2: project_diff.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
fn scene() -> (Session, String, String, String) { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "start": 0.0, "duration": 4.0}))["clip_id"].as_str().unwrap().to_string();
    let b = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 30, "height": 10, "start": 1.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string(); (ss, t, a, b) }
#[test] fn known_edits_are_reported_exactly_and_the_diff_is_read_only() {
    let (mut ss, t, a, b) = scene(); c(&mut ss, "checkpoint", json!({"name": "v1"})); let v1 = pj(&ss);
    c(&mut ss, "set_property", json!({"clip_id": a, "property": "x", "value": 12.0})); c(&mut ss, "delete_clip", json!({"clip_id": b}));
    let n = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 5, "height": 5, "start": 2.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "set_keyframe", json!({"clip_id": a, "property": "opacity", "time": 0.0, "value": 0.0})); c(&mut ss, "set_keyframe", json!({"clip_id": a, "property": "opacity", "time": 1.0, "value": 1.0}));
    let (h, u) = (pj(&ss), ss.undo.len()); let r = c(&mut ss, "project_diff", json!({"a": "checkpoint:v1", "b": "current"}));
    assert_eq!(r["counts"], json!({"added": 1, "removed": 1, "changed": 1})); assert_eq!(r["added"][0]["id"], n.as_str()); assert_eq!(r["removed"][0]["id"], b.as_str()); assert_eq!(r["changed"][0]["id"], a.as_str());
    let fields: Vec<String> = r["changed"][0]["changes"].as_array().unwrap().iter().map(|x| x["field"].as_str().unwrap().to_string()).collect(); assert!(fields.iter().any(|f| f == "props.x.base"), "{fields:?}"); assert!(fields.iter().any(|f| f == "props.opacity.keys.len"), "{fields:?}");
    let x = r["changed"][0]["changes"].as_array().unwrap().iter().find(|x| x["field"] == "props.x.base").unwrap(); assert_eq!((x["before"].as_f64().unwrap(), x["after"].as_f64().unwrap()), (0.0, 12.0)); assert_eq!(r["identical"], false);
    assert_eq!(pj(&ss), h); assert_eq!(ss.undo.len(), u, "read-only"); let same = c(&mut ss, "project_diff", json!({"a": "current", "b": "current"})); assert_eq!(same["identical"], true); assert_eq!(same["counts"], json!({"added": 0, "removed": 0, "changed": 0}));
    // undo:n and file:
    let r2 = c(&mut ss, "project_diff", json!({"a": "undo:1", "b": "current"})); assert_eq!(r2["changed"][0]["id"], a.as_str()); assert_eq!(r2["counts"]["added"], 0);
    let path = "/tmp/pd_v1.kimo"; std::fs::write(path, &v1).unwrap(); let r3 = c(&mut ss, "project_diff", json!({"a": format!("file:{path}"), "b": "current"})); assert_eq!(r3["counts"], r["counts"]);
}
#[test] fn moves_renames_tracks_canvas_caps_and_errors() {
    let (mut ss, t, a, _b) = scene(); c(&mut ss, "checkpoint", json!({"name": "v1"})); let t2 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "set_meta", json!({"id": a, "name": "hero"})); c(&mut ss, "set_meta", json!({"target": "track", "id": t, "name": "main"})); c(&mut ss, "project_set", json!({"width": 320, "height": 180}));
    let r = c(&mut ss, "project_diff", json!({"a": "checkpoint:v1", "b": "current"})); assert_eq!(r["tracks"]["added"][0]["id"], t2.as_str()); assert_eq!(r["tracks"]["renamed"][0]["after"], "main"); assert_eq!(r["canvas"]["after"][0], 320); assert!(r["changed"][0]["changes"].as_array().unwrap().iter().any(|x| x["field"] == "name" && x["after"] == "hero"));
    let r = c(&mut ss, "project_diff", json!({"a": "checkpoint:v1", "b": "current", "limit": 1})); assert_eq!(r["changed"].as_array().unwrap().len(), 1); assert_eq!(r["truncated"], false);
    for (args, want) in [(json!({"a": "undo:0", "b": "current"}), "n >= 1"), (json!({"a": "undo:99", "b": "current"}), "out of range"), (json!({"a": "checkpoint:zzz", "b": "current"}), "no checkpoint"), (json!({"a": "nope", "b": "current"}), "Valid:"), (json!({"a": "file:/tmp/does_not_exist.kimo", "b": "current"}), "cannot read"), (json!({"a": "current"}), "revision string"), (json!({"a": "current", "b": "current", "limit": 0}), "limit")] { let m = e(&mut ss, "project_diff", args); assert!(m.contains(want), "{m}"); }
}

#[test] fn reordering_clips_in_a_track_is_reported_and_untouched_projects_are_identical() {
    let (mut ss, _t, a, b) = scene(); c(&mut ss, "checkpoint", json!({"name": "v1"}));
    assert_eq!(c(&mut ss, "project_diff", json!({"a": "checkpoint:v1", "b": "current"}))["identical"], true, "no false positives on an untouched project");
    ss.project.tracks[0].clips.reverse(); let r = c(&mut ss, "project_diff", json!({"a": "checkpoint:v1", "b": "current"}));
    assert_eq!(r["reordered"][0]["before"], json!([a, b])); assert_eq!(r["reordered"][0]["after"], json!([b, a])); assert_eq!(r["counts"], json!({"added": 0, "removed": 0, "changed": 0})); assert_eq!(r["identical"], false);
}
