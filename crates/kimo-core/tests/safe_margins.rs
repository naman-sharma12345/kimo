//! RV-28 C3: query_safe_margins is a read-only data table.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, v: Value) -> Value { dispatch(ss, "query_safe_margins", &v).unwrap_or_else(|e| panic!("{e}")) }
fn e(ss: &mut Session, v: Value) -> String { dispatch(ss, "query_safe_margins", &v).err().unwrap_or_else(|| panic!("should fail")) }
fn rect(v: &Value, i: usize) -> Vec<f64> { v["zones"][i]["rect"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() }
#[test] fn standard_profiles_are_exact_fractions_of_any_aspect() {
    let mut ss = Session::new(); let before = serde_json::to_string(&ss.project).unwrap();
    assert_eq!(rect(&c(&mut ss, json!({"width": 1920, "height": 1080})), 0), vec![96.0, 54.0, 1728.0, 972.0]);
    assert_eq!(rect(&c(&mut ss, json!({"profile": "action_safe", "width": 1000, "height": 1000})), 0), vec![35.0, 35.0, 930.0, 930.0]);
    let v = c(&mut ss, json!({"profile": "tiktok", "aspect": "9:16", "width": 1080})); assert_eq!(v["canvas"], json!({"width": 1080.0, "height": 1920.0})); assert_eq!(v["estimated"], true); assert_eq!(v["source"], "documented-constant"); assert!(v["as_of"].is_string());
    assert_eq!(v["zones"].as_array().unwrap().iter().filter(|z| z["kind"] == "avoid").count(), 3);
    let sq = c(&mut ss, json!({"profile": "reels", "aspect": "1:1", "width": 1080})); assert_eq!(sq["canvas"]["height"], 1080.0);
    assert_eq!(v, c(&mut ss, json!({"profile": "tiktok", "aspect": "9:16", "width": 1080})), "deterministic"); assert_eq!(serde_json::to_string(&ss.project).unwrap(), before, "read-only");
}
#[test] fn custom_zones_round_trip_and_bad_input_errors_list_values() {
    let mut ss = Session::new();
    let z = json!([{"name": "logo", "rect": [10, 10, 100, 50], "kind": "avoid", "notes": "bug"}, {"name": "body", "rect": [0, 0, 640, 360], "kind": "safe"}]);
    let v = c(&mut ss, json!({"profile": "custom", "width": 640, "height": 360, "zones": z})); assert_eq!(v["source"], "user"); assert_eq!(rect(&v, 0), vec![10.0, 10.0, 100.0, 50.0]); assert_eq!(v["zones"][0]["notes"], "bug");
    assert!(e(&mut ss, json!({"profile": "nope"})).contains("Valid: title_safe, action_safe, tiktok, reels, shorts, youtube, custom"));
    assert!(e(&mut ss, json!({"profile": "custom", "width": 640, "height": 360, "zones": [{"name": "x", "rect": [600, 0, 100, 10]}]})).contains("inside"));
    assert!(e(&mut ss, json!({"profile": "custom", "width": 640, "height": 360, "zones": [{"name": "x", "rect": [0, 0, 10, 10], "kind": "bad"}]})).contains("Valid: safe, avoid"));
    assert!(e(&mut ss, json!({"profile": "custom"})).contains("zones")); assert!(e(&mut ss, json!({"aspect": "x"})).contains("aspect"));
}
