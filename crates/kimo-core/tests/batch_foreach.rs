//! U2.2: batch foreach is a pure, visible pre-pass.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn fresh() -> (Session, String) { let mut ss = Session::new(); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); (ss, t) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
#[test] fn range_expands_to_the_exact_flat_ops_with_typed_slots() {
    let (mut ss, t) = fresh();
    let r = c(&mut ss, "batch", json!({"expand_only": true, "ops": [{"foreach": {"var": "k", "in": {"range": [0, 3]}}, "ops": [{"id": "cap", "tool": "add_text", "args": {"track_id": t, "text": "Item {{k}} of {{n}}", "start": "{{i}}", "duration": 0.5}}]}]}));
    assert_eq!(r["count"], 3); assert_eq!(r["ops"][2], json!({"id": "cap#2", "tool": "add_text", "args": {"track_id": t, "text": "Item 2 of 3", "start": 2, "duration": 0.5}}));
    assert_eq!(c(&mut ss, "project_diff", json!({"a": "current", "b": "current"}))["identical"], true);
}
#[test] fn foreach_run_equals_hand_written_batch_and_refs_resolve_per_iteration() {
    let (mut a, ta) = fresh(); let (mut b, tb) = fresh(); assert_eq!(ta, tb);
    c(&mut a, "batch", json!({"ops": [{"foreach": {"var": "k", "in": [10, 20, 30]}, "ops": [{"id": "t", "tool": "add_text", "args": {"track_id": ta, "text": "n{{k}}", "start": "{{i}}", "duration": 1}}, {"tool": "set_property", "args": {"clip_id": "$t.clip_id", "property": "x", "value": "{{k}}"}}]}, {"tool": "set_property", "args": {"clip_id": "$t#2.clip_id", "property": "y", "value": 7}}]}));
    let mut hand = vec![]; for (i, k) in [10, 20, 30].iter().enumerate() { hand.push(json!({"id": format!("t#{i}"), "tool": "add_text", "args": {"track_id": tb, "text": format!("n{k}"), "start": i, "duration": 1}})); hand.push(json!({"tool": "set_property", "args": {"clip_id": format!("$t#{i}.clip_id"), "property": "x", "value": k}})); }
    hand.push(json!({"tool": "set_property", "args": {"clip_id": "$t#2.clip_id", "property": "y", "value": 7}}));
    c(&mut b, "batch", json!({"ops": hand})); assert_eq!(pj(&a), pj(&b), "foreach == hand-written");
    assert_eq!(a.undo.len(), b.undo.len(), "one undo entry each");
}
#[test] fn find_source_nesting_and_errors() {
    let (mut ss, t) = fresh();
    for i in 0..3 { let id = c(&mut ss, "add_text", json!({"track_id": t, "text": format!("c{i}"), "start": i, "duration": 1}))["clip_id"].as_str().unwrap().to_string(); c(&mut ss, "set_meta", json!({"id": id, "role": "cap"})); }
    c(&mut ss, "batch", json!({"ops": [{"foreach": {"var": "c", "in": {"find": {"role": "cap"}}}, "ops": [{"tool": "set_property", "args": {"clip_id": "{{c}}", "property": "x", "value": "{{i}}"}}]}]}));
    let xs: Vec<f64> = ss.project.tracks[0].clips.iter().map(|c| c.props["x"].base).collect(); assert_eq!(xs, vec![0.0, 1.0, 2.0]);
    let n = c(&mut ss, "batch", json!({"expand_only": true, "ops": [{"foreach": {"var": "a", "in": [1, 2, 3]}, "ops": [{"foreach": {"var": "b", "in": [1, 2]}, "ops": [{"id": "x", "tool": "find", "args": {"name": "q{{a}}{{b}}"}}]}]}]})); assert_eq!(n["count"], 6); assert_eq!(n["ops"][5]["id"], "x#2#1"); assert_eq!(n["ops"][5]["args"]["name"], "q32");
    let deep = json!({"expand_only": true, "ops": [{"foreach": {"var": "a", "in": [1]}, "ops": [{"foreach": {"var": "b", "in": [1]}, "ops": [{"foreach": {"var": "d", "in": [1]}, "ops": [{"tool": "find", "args": {}}]}]}]}]}); assert!(e(&mut ss, "batch", deep).contains("invalid_foreach"));
    assert!(e(&mut ss, "batch", json!({"expand_only": true, "ops": [{"foreach": {"var": "a", "in": {"range": [0, 600]}, "max": 1000}, "ops": [{"tool": "find", "args": {}}]}]})).contains("batch_expansion_too_large"));
    assert!(e(&mut ss, "batch", json!({"expand_only": true, "ops": [{"foreach": {"var": "a", "in": [1]}, "ops": [{"tool": "find", "args": {"name": "{{zz}}"}}]}]})).contains("unknown variable"));
    assert!(e(&mut ss, "batch", json!({"expand_only": true, "ops": [{"foreach": {"var": "a", "in": {"range": [0, 5]}, "max": 3}, "ops": [{"tool": "find", "args": {}}]}]})).contains("max is 3"));
}
#[test] fn failures_inside_a_foreach_name_the_expanded_op_under_atomic_and_non_atomic() {
    let mk = |t: &str, extra: Value| { let mut v = json!({"ops": [{"foreach": {"var": "tr", "in": [t, "no_such_track", t]}, "ops": [{"id": "a", "tool": "add_text", "args": {"track_id": "{{tr}}", "text": "x{{i}}", "start": "{{i}}", "duration": 1}}]}]}); for (k, x) in extra.as_object().unwrap() { v[k] = x.clone(); } v };
    let (mut ss, t) = fresh(); let before = pj(&ss); let undo = ss.undo.len();
    let m = e(&mut ss, "batch", mk(&t, json!({}))); assert!(m.starts_with("batch_failed"), "{m}"); assert!(m.contains("\"id\":\"a#1\"") && m.contains("\"rolled_back\":true"), "{m}"); assert_eq!(pj(&ss), before, "atomic: nothing applied"); assert_eq!(ss.undo.len(), undo);
    let r = c(&mut ss, "batch", mk(&t, json!({"atomic": false}))); assert_eq!(r["ok"], false); assert_eq!(r["applied"], 1); assert_eq!(r["failed_op"]["id"], "a#1"); assert_eq!(ss.project.tracks[0].clips.len(), 1, "non-atomic keeps iteration 0 and stops");
    let (mut ss, t) = fresh(); let r = c(&mut ss, "batch", mk(&t, json!({"atomic": false, "stop_on_error": false}))); assert_eq!((r["applied"].clone(), r["failed_ops"].as_array().unwrap().len()), (json!(2), 1)); assert_eq!(ss.project.tracks[0].clips.len(), 2);
    let (mut ss, t) = fresh(); let again = mk(&t, json!({"atomic": false})); let r1 = c(&mut ss, "batch", again.clone()); let (mut s2, t2) = fresh(); let r2 = c(&mut s2, "batch", mk(&t2, json!({"atomic": false}))); assert_eq!(r1["failed_op"], r2["failed_op"], "deterministic error");
}
