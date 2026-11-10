//! U1 dispatch hook: "@name" is accepted wherever a clip_id / parent_id / track_id (or clip_ids) argument exists, at top level and inside batch ops.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn scene() -> (Session, String, String, String) {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "start": 0.0, "duration": 4.0}))["clip_id"].as_str().unwrap().to_string();
    let b = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "start": 4.0, "duration": 4.0}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "set_meta", json!({"id": a, "name": "hero"})); c(&mut ss, "set_meta", json!({"target": "track", "id": t, "name": "main"})); (ss, t, a, b)
}
fn x_of(ss: &mut Session, id: &str) -> f64 { c(ss, "get_transform", json!({"clip_id": id, "t": 0.5, "space": "local"}))["position"]["x"].as_f64().or_else(|| c(ss, "get_transform", json!({"clip_id": id, "t": 0.5, "space": "local"}))["x"].as_f64()).unwrap_or(f64::NAN) }
#[test] fn at_name_works_for_clip_and_track_arguments_and_plain_ids_still_do() {
    let (mut ss, t, a, _) = scene();
    c(&mut ss, "set_property", json!({"clip_id": "@hero", "property": "x", "value": 12.0})); assert_eq!(ss.project.find_clip_mut(&a).unwrap().props["x"].at(0.0), 12.0);
    let n = c(&mut ss, "add_shape", json!({"track_id": "@main", "shape": "rect", "width": 5, "height": 5, "start": 8.0, "duration": 1.0})); assert!(n["clip_id"].is_string());
    assert!(ss.project.tracks.iter().find(|x| x.id == t).unwrap().clips.iter().any(|k| k.id == n["clip_id"].as_str().unwrap()));
    c(&mut ss, "set_property", json!({"clip_id": a, "property": "x", "value": 3.0})); assert_eq!(ss.project.find_clip_mut(&a).unwrap().props["x"].at(0.0), 3.0);
    let _ = x_of;
}
#[test] fn parent_id_and_batch_ops_resolve_names() {
    let (mut ss, _, a, b) = scene(); c(&mut ss, "set_meta", json!({"id": b, "name": "kid"}));
    c(&mut ss, "set_parent", json!({"clip_id": "@kid", "parent_id": "@hero"})); assert_eq!(ss.project.find_clip_mut(&b).unwrap().parent.as_deref(), Some(a.as_str()));
    let r = c(&mut ss, "batch", json!({"ops": [{"tool": "set_property", "args": {"clip_id": "@hero", "property": "y", "value": 7.0}}]})); assert!(r.to_string().contains("ok") || r["applied"].is_number() || r.is_object());
    assert_eq!(ss.project.find_clip_mut(&a).unwrap().props["y"].at(0.0), 7.0);
}
#[test] fn bad_names_fail_before_anything_runs_and_free_text_is_untouched() {
    let (mut ss, t, ..) = scene(); let before = serde_json::to_string(&ss.project).unwrap();
    let e = dispatch(&mut ss, "set_property", &json!({"clip_id": "@hreo", "property": "x", "value": 1.0})).unwrap_err(); assert!(e.contains("name_not_found") && e.contains("@hero"), "{e}");
    assert_eq!(before, serde_json::to_string(&ss.project).unwrap());
    let tx = c(&mut ss, "add_text", json!({"track_id": t, "text": "@hero", "start": 9.0, "duration": 1.0}));
    let id = tx["clip_id"].as_str().unwrap().to_string(); let txt = serde_json::to_string(&ss.project.find_clip_mut(&id).unwrap().source).unwrap(); assert!(txt.contains("@hero"), "text argument must not be resolved: {txt}");
}
#[test] fn edge_cases_empty_at_whitespace_and_non_strings() {
    let (mut ss, ..) = scene(); let before = serde_json::to_string(&ss.project).unwrap();
    for bad in ["@", "@ ", "@ hero", "@hero ", "@HERO", "@@hero"] {
        let e = dispatch(&mut ss, "set_property", &json!({"clip_id": bad, "property": "x", "value": 1.0})).unwrap_err(); assert!(e.contains("name_not_found"), "{bad:?}: {e}");
        let e = dispatch(&mut ss, "add_shape", &json!({"track_id": bad, "shape": "rect", "width": 5, "height": 5, "start": 8.0, "duration": 1.0})).unwrap_err(); assert!(e.contains("name_not_found") && e.contains("track"), "{bad:?}: {e}");
    }
    assert_eq!(before, serde_json::to_string(&ss.project).unwrap(), "no failed call changed anything");
    // a non-string id is left to the normal argument check, never turned into a name error
    let e = dispatch(&mut ss, "set_property", &json!({"clip_id": 5, "property": "x", "value": 1.0})).unwrap_err(); assert!(!e.contains("name_not_found"), "{e}");
    // a clip name is not a track name
    let e = dispatch(&mut ss, "add_shape", &json!({"track_id": "@hero", "shape": "rect", "width": 5, "height": 5, "start": 8.0, "duration": 1.0})).unwrap_err(); assert!(e.contains("name_not_found") && e.contains("track"), "{e}");
}
#[test] fn duplicate_names_are_ambiguous_and_list_the_ids() {
    let (mut ss, _, a, b) = scene();
    let e = dispatch(&mut ss, "set_meta", &json!({"id": b, "name": "hero"})).unwrap_err(); assert!(e.contains("name_taken"), "names are unique: {e}");
    for cl in ss.project.tracks[0].clips.iter_mut() { std::sync::Arc::make_mut(cl).name = Some("hero".into()); } // projects that predate uniqueness
    let e = dispatch(&mut ss, "set_property", &json!({"clip_id": "@hero", "property": "x", "value": 1.0})).unwrap_err(); assert!(e.contains("ambiguous_name") && e.contains(&a) && e.contains(&b), "{e}");
    c(&mut ss, "set_property", json!({"clip_id": a, "property": "x", "value": 1.0}));
}
#[test] fn clip_ids_lists_resolve_each_entry_and_one_bad_entry_runs_nothing() {
    let (mut ss, _, a, b) = scene(); c(&mut ss, "set_meta", json!({"id": b, "name": "kid"})); let before = serde_json::to_string(&ss.project).unwrap();
    let e = dispatch(&mut ss, "group_clips", &json!({"clip_ids": ["@hero", "@nope"]})).unwrap_err(); assert!(e.contains("name_not_found") && e.contains("nope"), "{e}");
    assert_eq!(before, serde_json::to_string(&ss.project).unwrap());
    c(&mut ss, "group_clips", json!({"clip_ids": ["@hero", b.clone()]})); assert!(ss.project.find_clip_mut(&a).is_none(), "grouped into a precomp");
}
#[test] fn names_inside_batch_ops_resolve_per_op_and_a_bad_one_rolls_the_batch_back() {
    let (mut ss, _, a, b) = scene(); c(&mut ss, "set_meta", json!({"id": b, "name": "kid"})); let before = serde_json::to_string(&ss.project).unwrap();
    let ops = json!([{"tool": "set_property", "args": {"clip_id": "@hero", "property": "x", "value": 9.0}}, {"tool": "set_property", "args": {"clip_id": "@kid", "property": "x", "value": 8.0}}, {"tool": "set_parent", "args": {"clip_id": "@kid", "parent_id": "@hero"}}]);
    c(&mut ss, "batch", json!({"ops": ops})); assert_eq!(ss.project.find_clip_mut(&a).unwrap().props["x"].at(0.0), 9.0); assert_eq!(ss.project.find_clip_mut(&b).unwrap().parent.as_deref(), Some(a.as_str()));
    let (mut ss, ..) = scene(); let before2 = serde_json::to_string(&ss.project).unwrap(); let _ = before;
    let ops = json!([{"tool": "set_property", "args": {"clip_id": "@hero", "property": "x", "value": 9.0}}, {"tool": "set_property", "args": {"clip_id": "@ghost", "property": "x", "value": 8.0}}]);
    let r = dispatch(&mut ss, "batch", &json!({"ops": ops})); let txt = match &r { Ok(v) => v.to_string(), Err(e) => e.clone() }; assert!(txt.contains("name_not_found"), "{txt}");
    assert_eq!(before2, serde_json::to_string(&ss.project).unwrap(), "atomic: the first op was rolled back");
}
