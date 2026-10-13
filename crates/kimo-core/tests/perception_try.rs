//! P4: every perception tool error carries "; try <registered tool>(<args>)" and the suggested call actually works; describe_frame has a summary and valid next_steps.
use kimo_core::commands::{dispatch, find_command, Session};
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn scene() -> Session {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":160,"height":90,"fps":24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","color":[0.2,0.5,0.8],"start":0,"duration":3})); s
}
/// (tool, args) pairs that must fail.
fn bad() -> Vec<(&'static str, Value)> { vec![
    ("describe_frame", json!({})), ("describe_frame", json!({"time":1,"roi":[0.9,0,0.1,1]})), ("describe_frame", json!({"time":1,"roi":[1,2]})), ("describe_frame", json!({"time":1,"isolate":["nope"]})),
    ("diff_frames", json!({})), ("diff_frames", json!({"a":{"time":0},"b":{"time":1},"metrics":["bogus"]})), ("diff_frames", json!({"a":{"time":0},"b":{"time":1},"threshold":5})), ("diff_frames", json!({"a":{"time":0},"b":{"time":1},"scale":0})), ("diff_frames", json!({"a":{"time":0},"b":{"time":1},"grid":{"cols":0,"rows":1}})),
    ("detect_cuts", json!({"source":"bogus"})), ("detect_cuts", json!({"method":"bogus"})), ("detect_cuts", json!({"sensitivity":9})), ("detect_cuts", json!({"source":"clip:nope"})),
    // edge cases and boundary limits: empty, zero-size, negative, huge, just outside the documented range
    ("describe_frame", json!({"time":1,"roi":[0,0,0,0]})), ("describe_frame", json!({"time":1,"roi":[]})), ("describe_frame", json!({"time":1,"roi":[0,0,1.0000001,1]})), ("describe_frame", json!({"time":1,"roi":[-0.1,0,1,1]})),
    ("diff_frames", json!({"a":{"time":0},"b":{"time":1},"scale":1e12})), ("diff_frames", json!({"a":{"time":0},"b":{"time":1},"scale":-1})), ("diff_frames", json!({"a":{"time":0},"b":{"time":1},"threshold":-0.0001})), ("diff_frames", json!({"a":{"time":0},"b":{"time":1},"grid":{"cols":65,"rows":1}})), ("diff_frames", json!({"a":{"time":0},"b":{"time":1},"region":[100000,0,5,5]})),
    ("detect_cuts", json!({"sensitivity":-1})), ("detect_cuts", json!({"threshold":1.0001})), ("detect_cuts", json!({"fps":0})), ("detect_cuts", json!({"scale":0})), ("detect_cuts", json!({"min_gap_s":-1})), ("detect_cuts", json!({"start":2,"end":1})),
    ("lint_frame", json!({"times":[]})), ("lint_frame", json!({"time":-5})), ("lint_frame", json!({"time":1,"max_findings":1.5})),
    ("lint_frame", json!({})), ("lint_frame", json!({"time":99999})), ("lint_frame", json!({"time":1,"checks":["bogus"]})), ("lint_frame", json!({"time":1,"params":{"bogus":1}})), ("lint_frame", json!({"time":1,"max_findings":"x"})),
] }
fn parse_try(e: &str) -> (String, Value) {
    let i = e.rfind("; try ").unwrap_or_else(|| panic!("error has no '; try ': {e}")); let call = &e[i + 6..]; let p = call.find('(').unwrap_or_else(|| panic!("try has no args: {e}"));
    assert!(call.ends_with(')'), "{e}"); (call[..p].to_string(), serde_json::from_str(&call[p + 1..call.len() - 1]).unwrap_or_else(|x| panic!("try args are not JSON ({x}): {e}")))
}
#[test] fn every_perception_error_names_a_registered_tool_to_try_and_that_call_works() {
    let mut s = scene();
    for (tool, args) in bad() {
        let e = dispatch(&mut s, tool, &args).expect_err(&format!("{tool} {args} should fail")); assert!(e.contains("; try "), "{tool} {args}: {e}");
        let (t, a) = parse_try(&e); assert!(find_command(&t).is_some(), "{t} is not a registered tool: {e}");
        dispatch(&mut s, &t, &a).unwrap_or_else(|x| panic!("suggested call {t}({a}) failed ({x}) for {e}"));
    }
}
#[test] fn describe_frame_has_a_summary_and_valid_next_steps() {
    let mut s = scene(); let r = c(&mut s, "describe_frame", json!({"time":1}));
    let sm = r["summary"].as_str().unwrap(); assert!(sm.starts_with("Frame at 1.00s is 160x90"), "{sm}"); assert!(sm.contains("1 visible clip"), "{sm}");
    let st = r["next_steps"].as_array().unwrap(); assert!(!st.is_empty() && st.len() <= 3);
    for x in st { let t = x["tool"].as_str().unwrap(); assert!(find_command(t).is_some()); assert!(x["why"].as_str().unwrap().chars().any(|c| c.is_ascii_digit()) || x["why"].as_str().unwrap().len() > 10); dispatch(&mut s, t, &x["args"]).unwrap_or_else(|e| panic!("next step {t}: {e}")); }
    let r = c(&mut s, "describe_frame", json!({"time":50})); assert!(r["summary"].as_str().unwrap().contains("no visible clips")); assert_eq!(r["next_steps"][0]["tool"], "get_project");
}
#[test] fn the_other_perception_tools_keep_valid_next_steps() {
    let mut s = scene(); let l = c(&mut s, "lint_frame", json!({"time":1,"annotate":false})); assert!(l["next_steps"].as_array().unwrap().len() <= 3);
    let d = c(&mut s, "detect_cuts", json!({})); assert!(d["next_steps"].as_array().unwrap().len() <= 3); for x in d["next_steps"].as_array().unwrap() { assert!(find_command(x["tool"].as_str().unwrap()).is_some()); }
    let f = c(&mut s, "diff_frames", json!({"a":{"time":0},"b":{"time":1}})); assert!(f["next_steps"].is_array());
}
