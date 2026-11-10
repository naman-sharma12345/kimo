//! v6 eyes: edit_check, the changed pixels between two frames counted inside vs outside allowed rects, as 8-connected
//! regions. Known references on synthetic frames worked out by hand: three blocks (one inside the allowed rect, one
//! outside, a diagonal pair that is one region under 8-connectivity), a block straddling the allowed edge, the threshold
//! (strictly greater than), outside.max_delta over every outside pixel (changed or not), max_regions / min_region_px, and
//! through dispatch a black -> white timeline (half the frame allowed) plus an identical pair; refused arguments last.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::edit_check::check;
use kimo_core::perception::analysis::Image;
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn put(im: &mut Image, x: usize, y: usize, v: f64) { let i = y * im.w + x; for ch in 0..3 { im.rgb[ch][i] = v; } }
fn block(im: &mut Image, x0: usize, y0: usize, w: usize, h: usize, v: f64) { for y in y0..y0 + h { for x in x0..x0 + w { put(im, x, y, v); } } }
/// 40x30 black; after: A 5x4 at (2,2) = 1.0 (inside [0,0,10,10]), B 3x3 at (20,10) = 0.5, diagonal pair (30,20)+(31,21) = 0.2,
/// one pixel at (35,25) = 0.03 (under the 0.04 default).
fn pair() -> (Image, Image) {
    let a = Image::new(40, 30); let mut b = a.clone();
    block(&mut b, 2, 2, 5, 4, 1.0); block(&mut b, 20, 10, 3, 3, 0.5); put(&mut b, 30, 20, 0.2); put(&mut b, 31, 21, 0.2); put(&mut b, 35, 25, 0.03);
    (a, b)
}
fn f(v: &Value) -> f64 { v.as_f64().unwrap_or_else(|| panic!("not a number: {v}")) }
fn close(a: f64, b: f64) -> bool { (a - b).abs() < 1e-6 }

#[test] fn regions_inside_outside() {
    let (a, b) = pair();
    let r = check(&a, &b, &[[0, 0, 10, 10]], 0.04, 32, 1);
    assert_eq!(r["bit_identical"], false); assert_eq!(r["connectivity"], 8);
    assert_eq!(r["changed_pixels"], 31); assert!(close(f(&r["changed_percent"]), 100.0 * 31.0 / 1200.0), "{r}");
    assert!(close(f(&r["max_delta"]), 1.0));
    assert_eq!(r["allowed_area_px"], 100); assert_eq!(r["allowed"], json!([{"rect": [0, 0, 10, 10], "changed_pixels": 20}]));
    assert_eq!(r["inside"]["changed_pixels"], 20); assert!(close(f(&r["inside"]["percent_of_allowed"]), 20.0));
    let o = &r["outside"]; assert_eq!(o["changed_pixels"], 11); assert!(close(f(&o["percent_of_outside"]), 100.0 * 11.0 / 1100.0));
    // B is the largest outside delta; scan order puts its first pixel at (20,10). The 0.03 pixel is not changed.
    assert!(close(f(&o["max_delta"]), 0.5)); assert_eq!(o["max_delta_at"], json!([20, 10]));
    assert_eq!(o["bbox"], json!([20, 10, 12, 12])); assert_eq!(o["regions"], 2);
    // the diagonal pair is ONE region (8-connected), so 3 regions, largest first
    assert_eq!(r["region_count"], 3); assert_eq!(r["regions_listed"], 3); assert_eq!(r["regions_truncated"], false);
    let rg = r["regions"].as_array().unwrap();
    assert_eq!(rg[0]["bbox"], json!([2, 2, 5, 4])); assert_eq!(rg[0]["pixels"], 20); assert_eq!(rg[0]["outside_pixels"], 0); assert!(close(f(&rg[0]["max_delta"]), 1.0));
    assert_eq!(rg[1]["bbox"], json!([20, 10, 3, 3])); assert_eq!(rg[1]["pixels"], 9); assert_eq!(rg[1]["outside_pixels"], 9); assert!(close(f(&rg[1]["max_delta"]), 0.5));
    assert_eq!(rg[2]["bbox"], json!([30, 20, 2, 2])); assert_eq!(rg[2]["pixels"], 2); assert!(close(f(&rg[2]["max_delta"]), 0.2));
    // no allowed rects: every change is outside, percent_of_allowed is null
    let r = check(&a, &b, &[], 0.04, 32, 1);
    assert_eq!(r["inside"]["changed_pixels"], 0); assert_eq!(r["inside"]["percent_of_allowed"], Value::Null); assert_eq!(r["outside"]["changed_pixels"], 31);
    assert!(close(f(&r["outside"]["max_delta"]), 1.0)); assert_eq!(r["outside"]["max_delta_at"], json!([2, 2])); assert_eq!(r["allowed_count"], 0);
}

#[test] fn straddling_block_and_threshold() {
    let a = Image::new(20, 12); let mut b = a.clone(); block(&mut b, 8, 0, 4, 2, 0.6);
    let r = check(&a, &b, &[[0, 0, 10, 10]], 0.04, 32, 1);
    assert_eq!(r["inside"]["changed_pixels"], 4); assert_eq!(r["outside"]["changed_pixels"], 4); assert_eq!(r["region_count"], 1);
    assert_eq!(r["regions"][0]["bbox"], json!([8, 0, 4, 2])); assert_eq!(r["regions"][0]["outside_pixels"], 4); assert_eq!(r["outside"]["bbox"], json!([10, 0, 2, 2]));
    // two overlapping allowed rects: the union is counted once
    let r = check(&a, &b, &[[0, 0, 10, 10], [5, 0, 10, 2]], 0.04, 32, 1);
    assert_eq!(r["allowed_area_px"], 100 + 10); assert_eq!(r["inside"]["changed_pixels"], 8); assert_eq!(r["outside"]["changed_pixels"], 0); assert_eq!(r["outside"]["bbox"], Value::Null);
    assert_eq!(r["allowed"][1]["changed_pixels"], 8); assert!(close(f(&r["outside"]["max_delta"]), 0.0)); assert_eq!(r["outside"]["max_delta_at"], Value::Null);
    // threshold is strictly greater than: 0.6 is not changed at threshold 0.6, but outside.max_delta still reads it
    let (a, b) = pair();
    let r = check(&a, &b, &[[0, 0, 10, 10]], 0.5, 32, 1);
    assert_eq!(r["changed_pixels"], 20); assert_eq!(r["outside"]["changed_pixels"], 0); assert!(close(f(&r["outside"]["max_delta"]), 0.5)); assert_eq!(r["region_count"], 1);
    let r = check(&a, &b, &[[0, 0, 10, 10]], 0.0, 32, 1);
    assert_eq!(r["changed_pixels"], 32); assert_eq!(r["region_count"], 4);
}

#[test] fn region_cap_and_min_size() {
    let (a, b) = pair();
    let r = check(&a, &b, &[], 0.04, 1, 1);
    assert_eq!(r["region_count"], 3); assert_eq!(r["regions_listed"], 1); assert_eq!(r["regions_truncated"], true); assert_eq!(r["regions"][0]["pixels"], 20);
    let r = check(&a, &b, &[], 0.04, 32, 5);
    assert_eq!(r["region_count"], 3); assert_eq!(r["regions_listed"], 2); assert_eq!(r["regions_below_min"], 1); assert_eq!(r["regions_truncated"], false);
    assert_eq!(r["changed_pixels"], 31, "small regions still count");
    // 40 single pixels on a grid: 40 regions, 32 listed by default, ties ordered top-to-bottom then left-to-right
    let a = Image::new(40, 30); let mut b = a.clone(); for k in 0..40 { put(&mut b, (k % 10) * 4, (k / 10) * 4, 1.0); }
    let r = check(&a, &b, &[], 0.04, 32, 1);
    assert_eq!(r["region_count"], 40); assert_eq!(r["regions_listed"], 32); assert_eq!(r["regions_truncated"], true);
    assert_eq!(r["regions"][0]["bbox"], json!([0, 0, 1, 1])); assert_eq!(r["regions"][1]["bbox"], json!([4, 0, 1, 1])); assert_eq!(r["regions"][10]["bbox"], json!([0, 4, 1, 1]));
}

fn timeline() -> Session {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    for (k, col) in [[0., 0., 0.], [1., 1., 1.]].iter().enumerate() { c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": col, "start": k as f64, "duration": 1.0})); }
    s
}

#[test] fn through_dispatch() {
    let mut s = timeline();
    let r = c(&mut s, "edit_check", json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "allowed": [[0, 0, 32, 36]]}));
    assert_eq!(r["width"], 64); assert_eq!(r["height"], 36); assert_eq!(r["threshold"], 0.04);
    assert_eq!(r["changed_pixels"], 64 * 36); assert_eq!(r["inside"]["changed_pixels"], 32 * 36); assert_eq!(r["outside"]["changed_pixels"], 32 * 36);
    assert!(close(f(&r["outside"]["max_delta"]), 1.0)); assert_eq!(r["outside"]["max_delta_at"], json!([32, 0]));
    assert_eq!(r["region_count"], 1); assert_eq!(r["regions"][0]["bbox"], json!([0, 0, 64, 36])); assert_eq!(r["regions"][0]["outside_pixels"], 32 * 36);
    assert_eq!(r["a"], json!({"time": 0.5})); assert_eq!(r["b"], json!({"time": 1.5}));
    let r = c(&mut s, "edit_check", json!({"a": {"time": 0.25}, "b": {"time": 0.75}}));
    assert_eq!(r["bit_identical"], true); assert_eq!(r["changed_pixels"], 0); assert_eq!(r["region_count"], 0); assert_eq!(r["regions"], json!([]));
    assert_eq!(r["outside"]["max_delta"], 0.0); assert_eq!(r["outside"]["max_delta_at"], Value::Null); assert_eq!(r["outside"]["bbox"], Value::Null);
    // read-only: no undo entry
    let h = c(&mut s, "history_list", json!({})); let h2 = { c(&mut s, "edit_check", json!({"a": {"time": 0.5}, "b": {"time": 1.5}})); c(&mut s, "history_list", json!({})) }; assert_eq!(h, h2);
}

#[test] fn refused_arguments() {
    let mut s = timeline();
    let many: Vec<Value> = (0..65).map(|_| json!([0, 0, 1, 1])).collect();
    let bad = [
        json!({"a": {"time": 0.5}}),
        json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "allowed": [[60, 0, 8, 8]]}),
        json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "allowed": [[0, 0, 0, 8]]}),
        json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "allowed": [[0, 0, 8]]}),
        json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "allowed": [[0.5, 0, 8, 8]]}),
        json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "allowed": [0, 0, 8, 8]}),
        json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "allowed": "all"}),
        json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "allowed": many}),
        json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "threshold": 1.0}),
        json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "threshold": -0.1}),
        json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "max_regions": 0}),
        json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "max_regions": 257}),
        json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "min_region_px": 0}),
    ];
    for a in &bad { assert!(dispatch(&mut s, "edit_check", a).is_err(), "accepted {a}"); }
    let e = dispatch(&mut s, "edit_check", &json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "allowed": [[60, 0, 8, 8]]})).unwrap_err(); assert!(e.contains("allowed[0]") && e.contains("64x36"), "{e}");
}
