//! U2.5: session_stats counts top-level calls and bytes; read-only except reset.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn row(r: &Value, t: &str) -> Value { r["per_tool"].as_array().unwrap().iter().find(|x| x["tool"] == t).cloned().unwrap_or(json!(null)) }
#[test] fn counts_calls_bytes_errors_and_resets() {
    let mut ss = Session::new(); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    for i in 0..3 { c(&mut ss, "add_text", json!({"track_id": t, "text": "x", "start": i, "duration": 1})); }
    let _ = dispatch(&mut ss, "add_text", &json!({"track_id": "nope", "text": "x", "start": 0, "duration": 1}));
    let before = serde_json::to_string(&ss.project).unwrap();
    let r = c(&mut ss, "session_stats", json!({})); assert_eq!(r["calls_total"], 5); assert_eq!(r["errors_total"], 1); assert_eq!(row(&r, "add_text")["calls"], 4); assert_eq!(row(&r, "add_text")["errors"], 1); assert_eq!(row(&r, "add_track")["calls"], 1); assert_eq!(r["per_tool"][0]["tool"], "add_text", "most called first");
    assert!(row(&r, "add_text")["bytes"].as_u64().unwrap() > 20); assert_eq!(serde_json::to_string(&ss.project).unwrap(), before, "read-only");
    let r2 = c(&mut ss, "session_stats", json!({"top": 1})); assert_eq!(r2["calls_total"], 6, "the previous stats call was counted"); assert_eq!(r2["per_tool"].as_array().unwrap().len(), 1); assert_eq!(r2["truncated"], true);
    c(&mut ss, "batch", json!({"ops": [{"tool": "add_text", "args": {"track_id": t, "text": "b", "start": 9, "duration": 1}}, {"tool": "add_text", "args": {"track_id": t, "text": "c", "start": 10, "duration": 1}}]}));
    let r3 = c(&mut ss, "session_stats", json!({"reset": true})); assert_eq!(row(&r3, "batch")["calls"], 1); assert_eq!(row(&r3, "add_text")["calls"], 4, "ops inside a batch count once, as the batch");
    let r4 = c(&mut ss, "session_stats", json!({})); assert_eq!(r4["calls_total"], 1, "after reset only the reset call itself is counted"); assert!(dispatch(&mut ss, "session_stats", &json!({"top": 0})).is_err());
}
