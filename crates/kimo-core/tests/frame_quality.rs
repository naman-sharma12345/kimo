//! v6 eyes: lint_frame quality numbers. Known references worked out by hand: an 8-bit luma ramp from code 50 to 75 in 10 px
//! plateaus (26 levels, 25 one-code steps per row, 24 plateaus of exactly 10 px between them), black (crushed 1, mean 0),
//! white (clipped 1), a pure-red pixel (channel-clipped but not luma-clipped), identical / one-pixel-changed previous frames;
//! through dispatch a black -> white cut at 1 s on a 24 fps timeline (frozen frames read identical to the frame before, the
//! cut frame reads max delta 1). Without the argument the output equals quality:false (timings aside) and carries no quality
//! keys; refused arguments last.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::frame_quality::{params, quality, QParams};
use kimo_core::perception::analysis::Image;
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn f(v: &Value) -> f64 { v.as_f64().unwrap_or_else(|| panic!("not a number: {v}")) }
fn grey(w: usize, h: usize, v: f64) -> Image { let mut im = Image::new(w, h); for ch in 0..3 { for p in im.rgb[ch].iter_mut() { *p = v; } } im }

#[test] fn ramp_levels_steps_and_plateaus() {
    let (w, h) = (260, 4); let mut im = Image::new(w, h);
    for y in 0..h { for x in 0..w { let v = (50 + x / 10) as f64 / 255.0; for ch in 0..3 { im.rgb[ch][y * w + x] = v; } } }
    let q = quality(&im, None, &QParams::default());
    assert_eq!(q["luma_levels"], 26, "{q}"); assert_eq!(f(&q["band_steps_per_row"]), 25.0); assert_eq!(q["band_plateaus"], 24 * 4); assert_eq!(f(&q["band_plateau_px"]), 10.0);
    assert_eq!(f(&q["crushed_frac"]), 0.0); assert_eq!(f(&q["clipped_frac"]), 0.0); assert_eq!(q["prev_identical"], Value::Null);
    // band_step 0 codes is refused; a ramp in 3-code steps is invisible to band_step 2 and seen by band_step 3
    let mut im3 = Image::new(w, 1); for x in 0..w { let v = (50 + 3 * (x / 10)) as f64 / 255.0; for ch in 0..3 { im3.rgb[ch][x] = v; } }
    assert_eq!(f(&quality(&im3, None, &QParams::default())["band_steps_per_row"]), 0.0);
    assert_eq!(f(&quality(&im3, None, &QParams { band_step: 3, ..QParams::default() })["band_steps_per_row"]), 25.0);
}

#[test] fn crushed_clipped_and_channel_clip() {
    let q = quality(&grey(8, 8, 0.0), None, &QParams::default()); assert_eq!(f(&q["crushed_frac"]), 1.0); assert_eq!(f(&q["mean_luma"]), 0.0); assert_eq!(q["luma_levels"], 1);
    let q = quality(&grey(8, 8, 1.0), None, &QParams::default()); assert_eq!(f(&q["clipped_frac"]), 1.0); assert_eq!(f(&q["channel_clipped_frac"]), 1.0);
    let mut im = grey(4, 4, 0.5); im.rgb[0][0] = 1.0; im.rgb[1][0] = 0.0; im.rgb[2][0] = 0.0;
    let q = quality(&im, None, &QParams::default()); assert_eq!(f(&q["channel_clipped_frac"]), 1.0 / 16.0); assert_eq!(f(&q["clipped_frac"]), 0.0);
    let q = quality(&grey(4, 4, 0.03), None, &QParams { crush_below: 0.05, ..QParams::default() }); assert_eq!(f(&q["crushed_frac"]), 1.0);
}

#[test] fn previous_frame_delta() {
    let a = grey(4, 4, 0.2); let q = quality(&a, Some(&a.clone()), &QParams::default());
    assert_eq!(q["prev_identical"], true); assert_eq!(f(&q["prev_mean_delta"]), 0.0); assert_eq!(f(&q["prev_max_delta"]), 0.0);
    let mut b = a.clone(); for ch in 0..3 { b.rgb[ch][5] = 0.7; }
    let q = quality(&b, Some(&a), &QParams::default()); assert_eq!(q["prev_identical"], false); assert!((f(&q["prev_max_delta"]) - 0.5).abs() < 1e-9); assert!((f(&q["prev_mean_delta"]) - 0.5 / 16.0).abs() < 1e-6);
}

fn cut() -> Session {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0, 0, 0], "start": 0.0, "duration": 1.0}));
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [1, 1, 1], "start": 1.0, "duration": 1.0}));
    s
}
fn strip(mut v: Value) -> String { if let Some(fr) = v["frames"].as_array_mut() { for x in fr { x.as_object_mut().unwrap().remove("elapsed_ms"); } } serde_json::to_string(&v).unwrap() }

#[test] fn through_lint_frame() {
    let mut s = cut();
    let a = c(&mut s, "lint_frame", json!({"times": [0.5, 1.0, 1.5], "annotate": false})); let b = c(&mut s, "lint_frame", json!({"times": [0.5, 1.0, 1.5], "annotate": false, "quality": false}));
    assert_eq!(strip(a.clone()), strip(b)); assert!(a["summary"].get("quality").is_none() && a["frames"][0].get("quality").is_none());
    let r = c(&mut s, "lint_frame", json!({"times": [0.5, 1.0, 1.5], "annotate": false, "quality": true}));
    let q: Vec<&Value> = r["frames"].as_array().unwrap().iter().map(|x| &x["quality"]).collect();
    assert_eq!(f(&q[0]["crushed_frac"]), 1.0); assert_eq!(q[0]["prev_identical"], true);
    assert_eq!(f(&q[1]["clipped_frac"]), 1.0); assert_eq!(q[1]["prev_identical"], false); assert_eq!(f(&q[1]["prev_max_delta"]), 1.0);
    assert_eq!(q[2]["prev_identical"], true);
    let sq = &r["summary"]["quality"]; assert_eq!(sq["identical_to_prev"], 2); assert_eq!(sq["identical_to_prev_times"], json!([0.5, 1.5]));
    assert_eq!(f(&sq["max_clipped_frac"]), 1.0); assert_eq!(f(&sq["max_clipped_frac_t"]), 1.0); assert_eq!(sq["min_luma_levels"], 1);
    assert_eq!(f(&sq["max_crushed_frac_t"]), 0.5); assert_eq!(sq["params"], json!({"crush_below": 0.02, "clip_above": 0.98, "band_step": 2}));
    // t = 0 has no frame before it
    let r0 = c(&mut s, "lint_frame", json!({"time": 0.0, "annotate": false, "quality": {"band_step": 4}}));
    assert_eq!(r0["frames"][0]["quality"]["prev_identical"], Value::Null); assert_eq!(r0["summary"]["quality"]["params"]["band_step"], 4);
}

#[test] fn refused_arguments() {
    for v in [json!("yes"), json!(1), json!({"band_step": 0}), json!({"band_step": 33}), json!({"crush_below": 0.5, "clip_above": 0.4}), json!({"clip_above": 2}), json!({"nope": 1})] {
        assert!(params(&v).is_err(), "accepted {v}");
    }
    assert!(params(&json!(false)).unwrap().is_none()); assert!(params(&Value::Null).unwrap().is_none());
    let mut s = cut(); assert!(dispatch(&mut s, "lint_frame", &json!({"time": 0.5, "quality": {"band_step": 0}})).is_err());
}
