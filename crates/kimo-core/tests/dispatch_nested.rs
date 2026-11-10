//! RV-11 core: dispatch_nested runs a tool through the SAME validation/lock path as dispatch but records no history.
use kimo_core::commands::{dispatch, dispatch_nested, Session};
use serde_json::json;

#[test] fn nested_calls_leave_no_undo_entry_but_behave_like_dispatch() {
    let mut a = Session::new(); let mut b = Session::new();
    let ta = dispatch(&mut a, "add_track", &json!({})).unwrap(); let tb = dispatch_nested(&mut b, "add_track", &json!({})).unwrap();
    assert_eq!(ta, tb, "same result and ids"); assert_eq!(serde_json::to_string(&a.project).unwrap(), serde_json::to_string(&b.project).unwrap(), "same project bytes");
    assert_eq!(a.undo.len(), 1); assert_eq!(b.undo.len(), 0, "no undo entry"); assert!(b.undo_labels.is_empty() && b.redo.is_empty());
    let e = dispatch_nested(&mut b, "no_such_tool", &json!({})).unwrap_err(); assert!(e.contains("unknown tool"));
    let e2 = dispatch_nested(&mut b, "add_shape", &json!({"track_id": "nope", "shape": "blob"})).unwrap_err(); assert!(e2.contains("blob") || e2.contains("shape"), "enum guard still applies: {e2}");
}
#[test] fn nested_calls_enforce_track_locks_and_keep_the_media_list() {
    let mut s = Session::new(); let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    let c = dispatch(&mut s, "add_shape", &json!({"track_id": t, "shape": "rect", "width": 10, "height": 10, "start": 0.0, "duration": 1.0})).unwrap()["clip_id"].as_str().unwrap().to_string();
    dispatch(&mut s, "set_track", &json!({"track_id": t, "locked": true})).unwrap(); let before = serde_json::to_string(&s.project).unwrap();
    let e = dispatch_nested(&mut s, "delete_clip", &json!({"clip_id": c})).unwrap_err(); assert!(e.contains("is locked"), "{e}"); assert_eq!(serde_json::to_string(&s.project).unwrap(), before, "lock failure restores the project");
    s.media.push(kimo_core::commands::Media { path: "x".into(), w: 1, h: 1, mime: "image/png" }); let n = s.media.len(); let _ = dispatch_nested(&mut s, "list_effects", &json!({})); assert_eq!(s.media.len(), n, "nested dispatch must not clear the media list");
}
