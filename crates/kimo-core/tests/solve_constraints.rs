//! RV-28 C4: solve_constraints analytic cases (read-only).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn solve(ss: &mut Session, v: Value) -> Value { dispatch(ss, "solve_constraints", &v).unwrap_or_else(|e| panic!("{e}")) }
fn xy(r: &Value, id: &str) -> (f64, f64) { let p = r["positions"].as_array().unwrap().iter().find(|p| p["id"] == id).unwrap(); (p["x"].as_f64().unwrap(), p["y"].as_f64().unwrap()) }
fn it(id: &str, w: f64, h: f64) -> Value { json!({"id": id, "w": w, "h": h}) }
#[test] fn closed_form_cases_match_hand_computed_numbers() {
    let mut ss = Session::new(); let before = serde_json::to_string(&ss.project).unwrap();
    let r = solve(&mut ss, json!({"box": [0, 0, 1000, 100], "items": [it("a", 100.0, 50.0), it("b", 100.0, 50.0), it("c", 100.0, 50.0)], "constraints": [{"type": "distribute", "axis": "x", "items": ["a", "b", "c"]}, {"type": "align", "edge": "vcenter", "items": ["a", "b", "c"]}]}));
    assert_eq!((xy(&r, "a"), xy(&r, "b"), xy(&r, "c")), ((0.0, 25.0), (450.0, 25.0), (900.0, 25.0))); assert_eq!(r["satisfied"], json!([0, 1])); assert_eq!(r["violated"], json!([]));
    let r = solve(&mut ss, json!({"box": [0, 0, 400, 400], "items": [it("a", 100.0, 20.0), it("b", 100.0, 30.0), it("c", 100.0, 40.0)], "constraints": [{"type": "stack", "axis": "y", "gap": 10, "items": ["a", "b", "c"]}]}));
    assert_eq!((xy(&r, "a").1, xy(&r, "b").1, xy(&r, "c").1), (0.0, 30.0, 70.0));
    let r = solve(&mut ss, json!({"box": [0, 0, 1920, 1080], "items": [it("t", 200.0, 100.0), it("l", 100.0, 50.0)], "constraints": [{"type": "center", "items": ["t"]}, {"type": "anchor", "anchor": "br", "margin": 10, "items": ["l"]}, {"type": "align", "edge": "right", "items": ["t"]}]}));
    assert_eq!(xy(&r, "t"), (860.0, 490.0), "the later align would undo center, so it is left unapplied"); assert_eq!(xy(&r, "l"), (1810.0, 1020.0));
    assert_eq!(r["violated"][0]["idx"], 2); assert!(r["violated"][0]["reason"].as_str().unwrap().starts_with("conflicts_with_constraint_0")); assert_eq!(r["satisfied"], json!([0, 1]));
    assert_eq!(serde_json::to_string(&ss.project).unwrap(), before, "read-only");
}
#[test] fn conflicts_are_reported_and_pinned_items_never_move() {
    let mut ss = Session::new();
    let r = solve(&mut ss, json!({"box": [0, 0, 1000, 100], "items": [it("a", 300.0, 50.0), it("b", 300.0, 50.0), it("c", 300.0, 50.0), it("d", 300.0, 50.0)], "constraints": [{"type": "stack", "axis": "x", "gap": 0, "items": ["a", "b", "c", "d"]}, {"type": "within", "items": ["a", "b", "c", "d"]}]}));
    assert_eq!(r["satisfied"], json!([0])); assert_eq!(r["violated"][0]["idx"], 1); assert!(r["violated"][0]["reason"].as_str().unwrap().starts_with("conflicts_with_constraint_0")); assert_eq!(xy(&r, "d"), (900.0, 0.0), "stack kept, within left unapplied");
    let r = solve(&mut ss, json!({"box": [0, 0, 1000, 100], "items": [{"id": "p", "w": 100, "h": 50, "pos": [950, 10]}], "constraints": [{"type": "within", "items": ["p"]}]}));
    assert_eq!(xy(&r, "p"), (950.0, 10.0)); assert_eq!(r["violated"][0]["reason"], "pinned_item_cannot_move"); assert_eq!(r["violated"][0]["residual"], 50.0);
    let r = solve(&mut ss, json!({"box": [0, 0, 1000, 100], "items": [{"id": "p", "w": 100, "h": 50, "pos": [100, 0], "anchor": "tl"}, it("q", 100.0, 50.0)], "constraints": [{"type": "min_gap", "gap": 10, "items": ["p", "q"]}]}));
    assert_eq!(xy(&r, "p"), (100.0, 0.0)); assert_eq!(r["violated"], json!([])); let q = xy(&r, "q"); assert!(q.0 >= 210.0 - 1e-6 || q.0 <= -10.0 + 1e-6 || q.1 >= 60.0 - 1e-6 || q.1 + 50.0 <= -10.0 + 1e-6, "{q:?}");
    let a = solve(&mut ss, json!({"box": [0, 0, 1000, 100], "items": [it("a", 100.0, 50.0), it("b", 100.0, 50.0), it("c", 100.0, 50.0)], "constraints": [{"type": "min_gap", "gap": 10, "items": ["a", "b", "c"]}]})); assert_eq!(a, solve(&mut ss, json!({"box": [0, 0, 1000, 100], "items": [it("a", 100.0, 50.0), it("b", 100.0, 50.0), it("c", 100.0, 50.0)], "constraints": [{"type": "min_gap", "gap": 10, "items": ["a", "b", "c"]}]})), "deterministic");
    assert_eq!(a["violated"], json!([]), "{a}");
}
#[test] fn bad_input_names_valid_values_and_1000_items_finish() {
    let mut ss = Session::new();
    let e = |ss: &mut Session, v: Value| dispatch(ss, "solve_constraints", &v).err().unwrap();
    assert!(e(&mut ss, json!({"box": "canvas", "items": [it("a", 1.0, 1.0)], "constraints": [{"type": "align", "edge": "diag", "items": ["a"]}]})).contains("Valid: left, right"));
    assert!(e(&mut ss, json!({"box": "canvas", "items": [it("a", 1.0, 1.0)], "constraints": [{"type": "nope", "items": ["a"]}]})).contains("Valid: align"));
    assert!(e(&mut ss, json!({"box": "canvas", "items": [it("a", 1.0, 1.0)], "constraints": [{"type": "center", "items": ["zz"]}]})).contains("unknown item"));
    let items: Vec<Value> = (0..1001).map(|i| it(&format!("i{i}"), 10.0, 10.0)).collect(); assert!(e(&mut ss, json!({"box": "canvas", "items": items, "constraints": []})).contains("at most 1000"));
    let items: Vec<Value> = (0..1000).map(|i| json!({"id": format!("i{i}"), "w": 10, "h": 10, "pos": [(i % 40) * 12, (i / 40) * 12]})).collect(); let ids: Vec<String> = (0..1000).map(|i| format!("i{i}")).collect();
    let t = std::time::Instant::now(); let r = solve(&mut ss, json!({"box": "canvas", "items": items, "constraints": [{"type": "min_gap", "gap": 1, "items": ids}]})); assert!(t.elapsed().as_secs() < 10, "1000-item cap took {:?}", t.elapsed()); assert_eq!(r["violated"], json!([]));
}
