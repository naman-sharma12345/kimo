//! U2.4: user-authored templates.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
fn fresh() -> (Session, String) { let mut ss = Session::new(); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); (ss, t) }
fn define(ss: &mut Session) { c(ss, "template_define", json!({"name": "captions", "params": [{"name": "track", "type": "id"}, {"name": "label", "type": "string"}, {"name": "n", "type": "number", "default": 3}, {"name": "gap", "type": "number", "default": 2}],
    "ops": [{"foreach": {"var": "k", "in": {"range": [0, "{{n}}"]}}, "ops": [{"id": "c", "tool": "add_text", "args": {"track_id": "{{track}}", "text": "{{label}} {{k}}", "start": {"$calc": "k*gap"}, "duration": 1.5}}, {"tool": "set_property", "args": {"clip_id": "$c.clip_id", "property": "x", "value": {"$calc": "i*10"}}}]}]})); }
#[test] fn define_list_describe_delete_round_trip_and_old_projects_stay_byte_equal() {
    let (mut ss, _) = fresh(); let plain = pj(&ss); assert!(!plain.contains("templates"), "no templates key when none are defined");
    define(&mut ss); assert!(pj(&ss).contains("\"templates\""));
    let l = c(&mut ss, "template_list", json!({})); assert_eq!(l["count"], 1); assert_eq!(l["templates"][0]["params"], json!(["track", "label", "n", "gap"]));
    let d = c(&mut ss, "template_describe", json!({"name": "captions"})); assert_eq!(d["params"][2], json!({"name": "n", "type": "number", "default": 3, "required": false})); assert_eq!(d["params"][0]["required"], true);
    assert_eq!(c(&mut ss, "template_define", json!({"name": "captions", "ops": [{"tool": "find", "args": {}}]}))["replaced"], true);
    c(&mut ss, "template_delete", json!({"name": "captions"})); assert_eq!(pj(&ss), plain, "delete restores the exact bytes");
    assert!(e(&mut ss, "template_describe", json!({"name": "captions"})).contains("unknown_template")); assert!(e(&mut ss, "template_delete", json!({"name": "x"})).contains("unknown_template"));
    assert!(e(&mut ss, "template_define", json!({"name": "t", "params": [{"name": "a", "type": "color"}], "ops": [{"tool": "find"}]})).contains("Valid: number, string, boolean, id, array, object, any"));
    assert!(e(&mut ss, "template_define", json!({"name": "bad name", "ops": [{"tool": "find"}]})).contains("template name")); assert!(e(&mut ss, "template_define", json!({"name": "t", "ops": []})).contains("ops"));
}
#[test] fn apply_equals_the_hand_written_batch_in_one_undo_entry_and_echoes_the_expansion() {
    let (mut a, ta) = fresh(); let (mut b, tb) = fresh(); define(&mut a); let before = pj(&a); let u0 = a.undo.len();
    let ex = c(&mut a, "template_apply", json!({"name": "captions", "args": {"track": ta, "label": "Line"}, "expand_only": true})); assert_eq!(pj(&a), before, "expand_only changes nothing"); assert_eq!(ex["ops"].as_array().unwrap().len(), 6);
    let dry = c(&mut a, "template_apply", json!({"name": "captions", "args": {"track": ta, "label": "Line"}, "dry_run": true})); assert_eq!(dry["expansion"], ex["ops"], "dry_run echo equals expand_only"); assert_eq!(a.undo.len(), u0);
    let r = c(&mut a, "template_apply", json!({"name": "captions", "args": {"track": ta, "label": "Line"}, "expansion": true})); assert_eq!(a.undo.len(), u0 + 1, "one undo entry"); assert_eq!(r["expansion"], ex["ops"], "executed ops equal the echoed ops"); assert_eq!(r["args"]["n"], 3);
    c(&mut b, "batch", json!({"ops": ex["ops"]})); let mut pa: Value = serde_json::from_str(&pj(&a)).unwrap(); pa.as_object_mut().unwrap().remove("templates"); assert_eq!(pa, serde_json::from_str::<Value>(&pj(&b)).unwrap(), "template apply == hand-written batch");
    c(&mut a, "undo", json!({})); assert_eq!(pj(&a), before, "a single undo reverts the whole template");
    let r2 = c(&mut a, "template_apply", json!({"name": "captions", "args": {"track": ta, "label": "Z", "n": 2, "gap": 5}, "expansion": true})); assert_eq!(r2["expansion"][2]["args"]["start"], 5, "defaults and overrides resolve, calc folded");
}
#[test] fn parameter_errors_name_the_parameter() {
    let (mut ss, t) = fresh(); define(&mut ss); let before = pj(&ss);
    assert!(e(&mut ss, "template_apply", json!({"name": "captions", "args": {"track": t}})).contains("template_param: missing parameter 'label' (string)"));
    assert!(e(&mut ss, "template_apply", json!({"name": "captions", "args": {"track": t, "label": "a", "color": 1}})).contains("unknown parameter 'color'") );
    assert!(e(&mut ss, "template_apply", json!({"name": "captions", "args": {"track": t, "label": "a", "n": "three"}})).contains("parameter 'n' must be a number"));
    assert!(e(&mut ss, "template_apply", json!({"name": "nope"})).contains("unknown_template: no template 'nope' (defined: captions)")); assert_eq!(pj(&ss), before);
    assert!(e(&mut ss, "batch", json!({"ops": [{"tool": "template_apply", "args": {"name": "captions"}}]})).contains("forbidden"));
}
#[test] fn library_export_and_import_round_trip() {
    let (mut a, _) = fresh(); define(&mut a); let path = "/tmp/u24_library.json"; c(&mut a, "template_list", json!({"export_path": path}));
    let (mut b, _) = fresh(); assert_eq!(c(&mut b, "template_define", json!({"library_path": path}))["imported"], json!(["captions"])); assert_eq!(c(&mut b, "template_describe", json!({"name": "captions"})), c(&mut a, "template_describe", json!({"name": "captions"})));
}
