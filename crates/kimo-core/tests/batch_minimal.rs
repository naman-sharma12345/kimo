//! U2.1: batch return modes (ids, minimal) and per-op quiet.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn ops(track: &str, n: usize) -> Vec<Value> { (0..n).map(|i| json!({"id": format!("t{i}"), "tool": "add_text", "args": {"track_id": track, "text": format!("Caption number {i}"), "start": i as f64, "duration": 0.9}})).collect() }
fn fresh() -> (Session, String) { let mut ss = Session::new(); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); (ss, t) }
fn bytes(v: &Value) -> usize { serde_json::to_string(v).unwrap().len() }
fn norm(mut v: Value) -> Value { v.as_object_mut().unwrap().remove("elapsed_ms"); v }
#[test] fn modes_shrink_the_reply_and_default_is_unchanged() {
    let sizes: Vec<(usize, Value)> = ["summary", "ids", "minimal"].iter().map(|m| { let (mut ss, t) = fresh(); let r = c(&mut ss, "batch", json!({"ops": ops(&t, 40), "return": m})); (bytes(&r), r) }).collect();
    let (mut ss, t) = fresh(); let dflt = c(&mut ss, "batch", json!({"ops": ops(&t, 40)}));
    assert_eq!(norm(dflt), norm(sizes[0].1.clone()), "no return arg == return summary");
    assert!(sizes[2].0 < sizes[1].0 && sizes[1].0 < sizes[0].0, "summary {} ids {} minimal {}", sizes[0].0, sizes[1].0, sizes[2].0);
    let min = &sizes[2].1; assert_eq!(min["ok"], true); assert_eq!(min["count"], 40); assert!(min.get("results").is_none()); assert_eq!(min["created"].as_array().unwrap().len(), 41 - 1 + 0 * 1, "{min}");
    let ids = &sizes[1].1["results"][0]; assert_eq!(ids["ok"], true); assert!(ids["created"][0].as_str().unwrap().starts_with("clip"));
    eprintln!("U2.1 bytes: summary {} ids {} minimal {}", sizes[0].0, sizes[1].0, sizes[2].0);
}
#[test] fn quiet_ops_drop_their_body_and_failures_keep_full_detail() {
    let (mut ss, t) = fresh(); let mut o = ops(&t, 3); o[1]["quiet"] = json!(true);
    let r = c(&mut ss, "batch", json!({"ops": o, "return": "full"})); assert_eq!(r["results"][1], json!({"index": 1, "id": "t1", "tool": "add_text", "quiet": true})); assert!(r["results"][0]["result"].is_object());
    let (mut ss, t) = fresh(); let mut o = ops(&t, 2); o.push(json!({"id": "bad", "tool": "set_property", "args": {"clip_id": "nope", "property": "x", "value": 1.0}}));
    let r = c(&mut ss, "batch", json!({"ops": o, "return": "minimal", "atomic": false})); assert_eq!(r["ok"], false); assert_eq!(r["failed_op"]["id"], "bad"); assert!(r["failed_op"]["error"]["message"].as_str().unwrap().len() > 5, "{r}");
    let e = dispatch(&mut fresh().0, "batch", &json!({"ops": [], "return": "tiny"})).err().unwrap(); assert!(e.contains("Valid: summary, full, ids, minimal") || e.contains("ops"), "{e}");
}
