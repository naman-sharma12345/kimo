//! U2.3: $calc in batch args.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
fn ev(ss: &mut Session, exprs: &[&str]) -> Vec<f64> { let ops: Vec<Value> = exprs.iter().map(|x| json!({"tool": "find", "args": {"limit": {"$calc": x}}})).collect(); let r = c(ss, "batch", json!({"expand_only": true, "ops": ops})); r["ops"].as_array().unwrap().iter().map(|o| o["args"]["limit"].as_f64().unwrap()).collect() }
#[test] fn analytic_rows_and_precedence() {
    let mut ss = Session::new();
    let got = ev(&mut ss, &["2+3*4", "(2+3)*4", "2^3^2", "-2^2", "10%4", "10-4-3", "8/4/2", "min(3,1,2)", "max(3,1,2)", "clamp(15,0,10)", "abs(-2.5)", "floor(2.7)", "ceil(2.1)", "round(2.5)", "sqrt(16)", "mix(10,20,0.25)", "smoothstep(0,1,0.5)", "sin(0)+cos(0)", "2^-1"]);
    let want = [14.0, 20.0, 512.0, -4.0, 2.0, 3.0, 1.0, 1.0, 3.0, 10.0, 2.5, 2.0, 3.0, 3.0, 4.0, 12.5, 0.5, 1.0, 0.5];
    for (k, w) in want.iter().enumerate() { assert!((got[k] - w).abs() < 1e-12, "row {k}: got {} want {w}", got[k]); }
}
#[test] fn loop_variables_and_refs_at_execution_time() {
    let mut ss = Session::new();
    let r = c(&mut ss, "batch", json!({"expand_only": true, "ops": [{"foreach": {"var": "k", "in": {"range": [0, 3]}}, "ops": [{"tool": "find", "args": {"limit": {"$calc": "k*10+i+n"}}}]}]})); let l: Vec<i64> = r["ops"].as_array().unwrap().iter().map(|o| o["args"]["limit"].as_i64().unwrap()).collect(); assert_eq!(l, vec![3, 14, 25]);
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let before = pj(&ss);
    let ops = |expr: &str| json!([{"id": "t", "tool": "add_text", "args": {"track_id": t, "text": "a", "start": 0, "duration": 1, "x": 100}}, {"id": "g", "tool": "get_transform", "args": {"clip_id": "$t.clip_id"}}, {"tool": "set_property", "args": {"clip_id": "$t.clip_id", "property": "y", "value": {"$calc": expr}}}]);
    let d = c(&mut ss, "batch", json!({"ops": ops("$g.transform.x*2+1"), "dry_run": true})); assert_eq!(d["resolved_calcs"][0]["args"]["value"], 201); assert_eq!(pj(&ss), before, "dry run leaves the project alone");
    c(&mut ss, "batch", json!({"ops": ops("$g.transform.x*2+1")})); assert_eq!(ss.project.tracks[0].clips[0].props["y"].base, 201.0);
    let before = pj(&ss); let m = e(&mut ss, "batch", json!({"ops": ops("$g.transform.x/0")})); assert!(m.contains("calc_error: division by zero at position 14"), "{m}"); assert_eq!(pj(&ss), before, "atomic: rolled back");
    let m = e(&mut ss, "batch", json!({"ops": [{"tool": "find", "args": {"limit": {"$calc": "$later.x"}}}, {"id": "later", "tool": "find", "args": {}}]})); assert!(m.contains("invalid_reference"), "{m}");
}
#[test] fn errors_carry_positions_and_caps_are_named() {
    let mut ss = Session::new();
    let bad = |ss: &mut Session, x: &str| e(ss, "batch", json!({"expand_only": true, "ops": [{"tool": "find", "args": {"limit": {"$calc": x}}}]}));
    assert!(bad(&mut ss, "1/0").contains("calc_error: division by zero at position 1")); assert!(bad(&mut ss, "5 % 0").contains("modulo by zero at position 2")); assert!(bad(&mut ss, "2+foo").contains("unknown name 'foo' (variables, pi and the listed functions only) at position 2"));
    assert!(bad(&mut ss, "sqrt(0-1)").contains("not a finite number")); assert!(bad(&mut ss, "'a'+1").contains("unexpected character") && bad(&mut ss, "'a'+1").contains("position 0")); assert!(bad(&mut ss, "(1+2").contains("expected ) at position 4")); assert!(bad(&mut ss, "clamp(1,5,0)").contains("lo <= hi"));
    assert!(bad(&mut ss, &"1+".repeat(70)).contains("120 tokens")); assert!(bad(&mut ss, &"1".repeat(401)).contains("400-character"));
    assert!(bad(&mut ss, "10^1000").contains("not a finite number"));
}
