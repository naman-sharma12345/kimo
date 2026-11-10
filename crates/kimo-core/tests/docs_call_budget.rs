//! docs/examples/call_budget: the documented 1-call batch and the template route build the same project as the 77-call serial build. Counts and bytes come from session_stats.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
fn ex(n: &str) -> String { format!("{}/../../docs/examples/call_budget/{n}", env!("CARGO_MANIFEST_DIR")) }
fn measure(ss: &mut Session) -> (u64, u64) { let r = c(ss, "session_stats", json!({})); r["per_tool"].as_array().unwrap().iter().filter(|x| x["tool"] != "session_stats").fold((0, 0), |(a, b), x| (a + x["calls"].as_u64().unwrap(), b + x["bytes"].as_u64().unwrap())) }
fn serial() -> (Session, (u64, u64)) {
    let mut ss = Session::new(); c(&mut ss, "session_stats", json!({"reset": true})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    for k in 0..40 { c(&mut ss, "add_text", json!({"track_id": t, "text": format!("Caption {k}"), "start": k as f64 * 1.5, "duration": 1.25})); }
    for k in 0..12 { let id = c(&mut ss, "add_text", json!({"track_id": t, "text": format!("Title {k}"), "start": 60.0 + k as f64 * 4.0, "duration": 3.5}))["clip_id"].as_str().unwrap().to_string();
        c(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "opacity", "keys": [{"t": 0, "v": 0}, {"t": 0.5, "v": 1}, {"t": 3.0, "v": 1}, {"t": 3.5, "v": 0}]}));
        c(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "y", "keys": [{"t": 0, "v": k as f64 * 8.0}, {"t": 3.5, "v": k as f64 * 8.0 - 40.0}]})); }
    let m = measure(&mut ss); (ss, m)
}
#[test] fn the_documented_one_call_batch_and_the_template_route_equal_the_serial_build() {
    let (a, (ca, ba)) = serial();
    let mut b = Session::new(); c(&mut b, "session_stats", json!({"reset": true})); let body: Value = serde_json::from_str(&std::fs::read_to_string(ex("segment_one_batch.json")).unwrap()).unwrap(); c(&mut b, "batch", body); let (cb, bb) = measure(&mut b);
    let mut t = Session::new(); c(&mut t, "session_stats", json!({"reset": true})); c(&mut t, "template_define", json!({"library_path": ex("segment_template.json")})); c(&mut t, "template_apply", json!({"name": "codex_segment", "args": {}})); let (ct, bt) = measure(&mut t);
    let mut tj: Value = serde_json::from_str(&pj(&t)).unwrap(); tj.as_object_mut().unwrap().remove("templates");
    assert_eq!(pj(&a), pj(&b), "one batch == serial"); assert_eq!(serde_json::from_str::<Value>(&pj(&a)).unwrap(), tj, "template route == serial");
    assert_eq!((ca, cb, ct), (77, 1, 2)); assert!(bt < 1024, "template route {bt} bytes"); println!("CALLBUDGET serial {ca} calls {ba} bytes | one batch {cb} call {bb} bytes | template define+apply {ct} calls {bt} bytes");
}
