//! N1: isolate / hide render overrides (read-only).
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::view::view_of;
use kimo_core::render;
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
fn sid(v: &Value) -> String { v["clip_id"].as_str().unwrap().to_string() }
/// bg solid, a red box "box", a text-like green box "title" on top
fn scene() -> (Session, Vec<String>) { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = sid(&c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 160, "height": 90, "fill": [0.1, 0.2, 0.6], "start": 0.0, "duration": 5.0})));
    let b = sid(&c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 50, "height": 40, "fill": [0.9, 0.1, 0.1], "start": 0.0, "duration": 5.0})));
    let d = sid(&c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 70, "height": 20, "fill": [0.1, 0.9, 0.2], "start": 0.0, "duration": 5.0})));
    c(&mut ss, "set_property", json!({"clip_id": d, "property": "y", "value": 25.0})); c(&mut ss, "set_property", json!({"clip_id": b, "property": "x", "value": -30.0}));
    c(&mut ss, "set_meta", json!({"id": a, "name": "bg"})); c(&mut ss, "set_meta", json!({"id": b, "name": "box"})); c(&mut ss, "set_meta", json!({"id": d, "name": "title"})); (ss, vec![a, b, d]) }
fn hash(p: &kimo_core::model::Project, t: f64) -> u64 { let fr = render::render_frame(p, t, &render::Cache::new()).unwrap(); let mut h: u64 = 0xcbf29ce484222325; for v in &fr.d { for b in v.to_le_bytes() { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); } } h }
fn png(ss: &mut Session, extra: Value, path: &str) -> Vec<u8> { let mut a = json!({"time": 1.0, "path": path, "inline": "never"}); for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); } c(ss, "render_frame", a); std::fs::read(path).unwrap() }

#[test] fn isolate_and_hide_equal_deleting_the_other_clips_bit_for_bit() {
    let (ss, ids) = scene();
    let (mut del, _) = scene(); c(&mut del, "delete_clip", json!({"clip_id": ids[0]})); c(&mut del, "delete_clip", json!({"clip_id": ids[2]}));
    assert_eq!(hash(&view_of(&ss.project, Some(&[ids[1].clone()]), None), 1.0), hash(&del.project, 1.0), "isolate == others deleted");
    let (mut del2, _) = scene(); c(&mut del2, "delete_clip", json!({"clip_id": ids[2]}));
    assert_eq!(hash(&view_of(&ss.project, None, Some(&[ids[2].clone()])), 1.0), hash(&del2.project, 1.0), "hide == deleted");
    assert_ne!(hash(&ss.project, 1.0), hash(&del2.project, 1.0)); assert_eq!(hash(&view_of(&ss.project, None, None), 1.0), hash(&ss.project, 1.0), "no override = same pixels");
    let both = view_of(&ss.project, Some(&[ids[1].clone(), ids[2].clone()]), Some(&[ids[2].clone()])); assert_eq!(hash(&both, 1.0), hash(&del.project, 1.0), "hide wins over isolate");
}
#[test] fn render_frame_describe_and_contact_sheet_accept_the_override_and_leave_the_project_alone() {
    let (mut ss, _ids) = scene(); let (mut del, ids2) = scene(); c(&mut del, "delete_clip", json!({"clip_id": ids2[0]})); c(&mut del, "delete_clip", json!({"clip_id": ids2[2]}));
    let h = pj(&ss); let u = ss.undo.len();
    let (p1, p2, p3) = ("/tmp/n1_iso.png", "/tmp/n1_del.png", "/tmp/n1_full.png");
    let a = png(&mut ss, json!({"isolate": ["@box"]}), p1); let b = png(&mut del, json!({}), p2); assert_eq!(a, b, "tool output equals the deleted-project render");
    let full = png(&mut ss, json!({}), p3); assert_ne!(full, a); assert_eq!(png(&mut ss, json!({"hide": ["@title", "@bg"]}), p1), b);
    assert_eq!(pj(&ss), h); assert_eq!(ss.undo.len(), u);
    let d = c(&mut ss, "describe_frame", json!({"time": 1.0, "isolate": ["@box"]})); assert_eq!(d["visible_clips_total"], 1); assert_eq!(c(&mut ss, "describe_frame", json!({"time": 1.0}))["visible_clips_total"], 3);
    let r = c(&mut ss, "contact_sheet", json!({"path": "/tmp/n1_sheet.png", "times": [0.5, 1.0], "hide": ["@bg"]})); assert!(r["view"]["hide"].is_array()); assert_eq!(pj(&ss), h);
    assert_eq!(png(&mut ss, json!({}), p3), full, "caches and project are restored: a plain render afterwards is unchanged");
}
#[test] fn override_errors_change_nothing() {
    let (mut ss, _ids) = scene(); let h = pj(&ss);
    for (args, want) in [(json!({"isolate": []}), "at least one"), (json!({"isolate": "box"}), "array"), (json!({"hide": [5]}), "array"), (json!({"isolate": ["@nope"]}), "name_not_found"), (json!({"hide": ["clip999"]}), "no clip")] {
        let mut a = json!({"time": 1.0, "path": "/tmp/n1_err.png"}); for (k, v) in args.as_object().unwrap() { a[k] = v.clone(); } let m = e(&mut ss, "render_frame", a); assert!(m.contains(want), "{m}"); }
    assert_eq!(pj(&ss), h);
}
#[test] fn isolated_child_keeps_its_resolved_pose_and_hiding_a_parent_leaves_its_children() {
    let (mut ss, ids) = scene(); let t = ss.project.tracks[0].id.clone(); let p = sid(&c(&mut ss, "add_null", json!({"track_id": t, "start": 0.0, "duration": 5.0, "x": 20.0, "rotation": 30.0, "scale": 1.3, "opacity": 0.8})));
    c(&mut ss, "set_parent", json!({"clip_id": ids[1], "parent_id": p, "mode": "keep_local"}));
    let w = c(&mut ss, "get_transform", json!({"clip_id": ids[1], "t": 1.0, "space": "world"}))["world"].clone();
    let iso = view_of(&ss.project, Some(&[ids[1].clone()]), None);
    // hand-flattened child alone with the world pose set directly
    let (mut fl, f_ids) = scene(); for k in [0usize, 2] { c(&mut fl, "delete_clip", json!({"clip_id": f_ids[if k == 0 { 0 } else { 2 }]})); } for k in ["x", "y", "scale", "rotation", "opacity"] { c(&mut fl, "set_property", json!({"clip_id": f_ids[1], "property": k, "value": w[k]})); }
    assert_eq!(hash(&iso, 1.0), hash(&fl.project, 1.0), "isolated child == its world pose drawn alone");
    let unparented_local = { let (mut x, i2) = scene(); for k in [0usize, 2] { c(&mut x, "delete_clip", json!({"clip_id": i2[k]})); } hash(&x.project, 1.0) }; assert_ne!(hash(&iso, 1.0), unparented_local, "the parent transform is still applied");
    let hid = view_of(&ss.project, None, Some(&[p.clone()])); assert_eq!(hash(&hid, 1.0), hash(&ss.project, 1.0), "a null parent draws nothing, hiding it changes no pixel");
    let hide_child_parent = view_of(&ss.project, None, Some(&[ids[0].clone()])); assert_ne!(hash(&hide_child_parent, 1.0), hash(&ss.project, 1.0));
}

#[test] fn diff_frames_compares_two_project_revisions_at_the_same_time() {
    let (mut ss, ids) = scene(); c(&mut ss, "checkpoint", json!({"name": "v1"}));
    let same = c(&mut ss, "diff_frames", json!({"a": {"time": 1.0, "source": "snapshot:checkpoint:v1"}, "b": {"time": 1.0, "source": "snapshot:current"}})); assert_eq!(same["metrics"]["changed_ratio"].as_f64().unwrap(), 0.0); assert_eq!(same["a"]["source"], "snapshot:checkpoint:v1");
    c(&mut ss, "set_property", json!({"clip_id": ids[1], "property": "x", "value": 40.0})); let (h, u) = (pj(&ss), ss.undo.len());
    let d = c(&mut ss, "diff_frames", json!({"a": {"time": 1.0, "source": "snapshot:checkpoint:v1"}, "b": {"time": 1.0, "source": "snapshot:current"}})); assert!(d["metrics"]["changed_ratio"].as_f64().unwrap() > 0.0, "{d}"); assert_eq!(same["delta"]["bit_identical"], true); assert!(d["delta"]["changed_pixels"].as_u64().unwrap() > 0 && d["delta"]["bbox"].is_array(), "{d}");
    let back = c(&mut ss, "diff_frames", json!({"a": {"time": 1.0, "source": "snapshot:undo:1"}, "b": {"time": 1.0}})); assert!(back["metrics"]["changed_ratio"].as_f64().unwrap() > 0.0, "undo:1 is the state before the edit, composite is current");
    let path = "/tmp/n1_rev.kimo"; let v1: Value = serde_json::to_value(&ss.undo[ss.undo.len() - 1]).unwrap(); std::fs::write(path, v1.to_string()).unwrap();
    let f = c(&mut ss, "diff_frames", json!({"a": {"time": 1.0, "source": format!("file:{path}")}, "b": {"time": 1.0, "source": "snapshot:checkpoint:v1"}})); assert_eq!(f["metrics"]["changed_ratio"].as_f64().unwrap(), 0.0, "a saved file of the same revision is identical");
    assert_eq!(pj(&ss), h); assert_eq!(ss.undo.len(), u, "read-only");
    assert!(e(&mut ss, "diff_frames", json!({"a": {"time": 1.0, "source": "snapshot:nope"}, "b": {"time": 1.0}})).contains("Valid:"));
}
