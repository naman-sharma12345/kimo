//! U2 benchmark row: a Codex-style segment (40 captions + 12 named clips with keyframes) built serially and again with
//! foreach / $calc / refs in <= 5 calls. The two projects must be byte-identical; calls and response bytes come from session_stats.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
/// (calls, response bytes) of everything except the stats calls themselves.
fn measure(ss: &mut Session) -> (u64, u64) {
    let r = c(ss, "session_stats", json!({})); let rows = r["per_tool"].as_array().unwrap();
    rows.iter().filter(|x| x["tool"] != "session_stats").fold((0, 0), |(a, b), x| (a + x["calls"].as_u64().unwrap(), b + x["bytes"].as_u64().unwrap()))
}
const CAPS: u32 = 40; const NAMED: u32 = 12;
fn serial() -> (Session, (u64, u64)) {
    let mut ss = Session::new(); c(&mut ss, "session_stats", json!({"reset": true}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    for k in 0..CAPS { c(&mut ss, "add_text", json!({"track_id": t, "text": format!("Caption {k}"), "start": k as f64 * 1.5, "duration": 1.25})); }
    for k in 0..NAMED {
        let id = c(&mut ss, "add_text", json!({"track_id": t, "text": format!("Title {k}"), "start": 60.0 + k as f64 * 4.0, "duration": 3.5}))["clip_id"].as_str().unwrap().to_string();
        c(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "opacity", "keys": [{"t": 0, "v": 0}, {"t": 0.5, "v": 1}, {"t": 3.0, "v": 1}, {"t": 3.5, "v": 0}]}));
        c(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "y", "keys": [{"t": 0, "v": k as f64 * 8.0}, {"t": 3.5, "v": k as f64 * 8.0 - 40.0}]}));
    }
    let m = measure(&mut ss); (ss, m)
}
fn batched() -> (Session, (u64, u64)) {
    let mut ss = Session::new(); c(&mut ss, "session_stats", json!({"reset": true}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "batch", json!({"return": "minimal", "ops": [{"foreach": {"var": "k", "in": {"range": [0, CAPS]}}, "ops": [
        {"tool": "add_text", "args": {"track_id": t, "text": "Caption {{k}}", "start": {"$calc": "i*1.5"}, "duration": 1.25}}]}]}));
    c(&mut ss, "batch", json!({"return": "minimal", "ops": [{"foreach": {"var": "k", "in": {"range": [0, NAMED]}}, "ops": [
        {"id": "title", "tool": "add_text", "args": {"track_id": t, "text": "Title {{k}}", "start": {"$calc": "60+i*4"}, "duration": 3.5}},
        {"tool": "set_keyframes", "args": {"clip_id": "$title.clip_id", "property": "opacity", "keys": [{"t": 0, "v": 0}, {"t": 0.5, "v": 1}, {"t": 3.0, "v": 1}, {"t": 3.5, "v": 0}]}},
        {"tool": "set_keyframes", "args": {"clip_id": "$title.clip_id", "property": "y", "keys": [{"t": 0, "v": {"$calc": "i*8"}}, {"t": 3.5, "v": {"$calc": "i*8-40"}}]}}]}]}));
    let m = measure(&mut ss); (ss, m)
}
#[test] fn codex_segment_serial_vs_foreach_calc_refs() {
    let (a, (ca, ba)) = serial(); let (b, (cb, bb)) = batched();
    assert_eq!(pj(&a), pj(&b), "project hashes equal: the batched build is the serial build");
    assert_eq!(a.project.tracks[0].clips.len(), (CAPS + NAMED) as usize);
    assert!(cb <= 5, "batched build must take <= 5 calls, took {cb}"); assert_eq!(ca, 1 + CAPS as u64 + 3 * NAMED as u64);
    assert!(bb < ba, "batched responses must be smaller: {bb} vs {ba}");
    println!("BENCH serial: {ca} calls, {ba} response bytes | batched: {cb} calls, {bb} response bytes");
}
