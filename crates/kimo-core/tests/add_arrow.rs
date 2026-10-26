//! add_arrow (new, lane KF17): an animated "look here" arrow/callout pointing at a spot on screen, with
//! an optional short label. All tests below must fail before the tool exists (dispatch errors with
//! "unknown action").
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{self, render_frame, Cache};
use kimo_core::toolgroups::search_tools;
use serde_json::{json, Value};

const W: u32 = 640;
const H: u32 = 360;
const FPS: f64 = 30.0;

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn project() -> Session { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": FPS})); s }

fn rendered(s: &Session, t: f64) -> kimo_core::effects::Frame {
    let mut cache = Cache::new();
    render::prepare_at(&s.project, &mut cache, Some(t)).unwrap();
    render_frame(&s.project, t, &cache).unwrap()
}
fn px(f: &kimo_core::effects::Frame, x: u32, y: u32) -> [f32; 4] {
    let o = ((y as usize) * f.w + x as usize) * 4;
    [f.d[o], f.d[o + 1], f.d[o + 2], f.d[o + 3]]
}
/// Bright yellow #FFD400, the tool's own default -- matched loosely (alpha-weighted blending at AA edges
/// means a strict per-channel tolerance is too tight right at a shape boundary), not by exact channel.
fn is_yellowish(got: [f32; 4]) -> bool {
    got[3] > 0.3 && got[0] > 0.6 && got[1] > 0.35 && got[2] < 0.35
}

/// Horizontal arrow, left to right, in the exact middle row -- lets pixel checks walk along y = H/2.
fn horizontal_args(at_s: f64, duration_s: f64) -> Value {
    json!({"target": {"x": 0.8, "y": 0.5}, "from": {"x": 0.2, "y": 0.5}, "at_s": at_s, "duration_s": duration_s, "animate": "draw"})
}

#[test]
fn arrow_pixels_appear_near_target_only_from_at_s_on() {
    let mut s = project();
    c(&mut s, "add_arrow", horizontal_args(1.0, 2.0));
    let target_xy = ((0.8 * W as f64) as u32, (0.5 * H as f64) as u32);
    let before = rendered(&s, 0.5);
    assert!(!is_yellowish(px(&before, target_xy.0, target_xy.1)), "no arrow pixels before at_s: {:?}", px(&before, target_xy.0, target_xy.1));
    let after = rendered(&s, 1.0 + 0.6);
    assert!(is_yellowish(px(&after, target_xy.0, target_xy.1)), "arrow pixels should be near target by at_s+0.6: {:?}", px(&after, target_xy.0, target_xy.1));
}

#[test]
fn draw_animation_shaft_is_shorter_early_than_at_mid_hold() {
    let mut s = project();
    c(&mut s, "add_arrow", horizontal_args(0.0, 2.0));
    // Sample a point 70% of the way from the tail (x=0.2) toward the target (x=0.8), well clear of the
    // arrowhead triangle itself, along the shaft's own row.
    let sample_x = (0.2 * W as f64 + 0.7 * (0.8 - 0.2) * W as f64) as u32;
    let y = (0.5 * H as f64) as u32;
    let early = rendered(&s, 0.05);
    let mid_hold = rendered(&s, 0.6);
    let early_hit = is_yellowish(px(&early, sample_x, y));
    let mid_hit = is_yellowish(px(&mid_hold, sample_x, y));
    assert!(!early_hit, "at at_s+0.05 the shaft should not have reached 70% of the way to target yet: {:?}", px(&early, sample_x, y));
    assert!(mid_hit, "at at_s+0.6 (after the ~0.3s draw-in) the shaft should fully span tail..head: {:?}", px(&mid_hold, sample_x, y));
}

#[test]
fn label_text_pixels_present_when_label_set() {
    let mut s = project();
    let mut args = horizontal_args(0.0, 2.0);
    args["label"] = json!("THIS ONE");
    args["animate"] = json!("pop"); // fully in immediately, no draw-in timing to account for
    c(&mut s, "add_arrow", args);
    let f = rendered(&s, 1.0);
    // The label sits beyond the tail (x=0.2 continuing further left), roughly centered on y=0.5 --
    // somewhere in that region should be non-background (white fill / dark stroke / shadow), not the
    // plain black render background.
    let mut any_non_bg = false;
    let y = (0.5 * H as f64) as u32;
    for x in 0..((0.18 * W as f64) as u32) {
        let p = px(&f, x, y);
        if p[3] > 0.3 && (p[0] + p[1] + p[2]) > 0.05 { any_non_bg = true; break; }
    }
    assert!(any_non_bg, "label text pixels should be present near the tail when label is set");
}

#[test]
fn no_label_means_no_extra_pixels_in_the_label_region() {
    let mut s = project();
    let mut args = horizontal_args(0.0, 2.0);
    args["animate"] = json!("pop");
    c(&mut s, "add_arrow", args); // no label
    let f = rendered(&s, 1.0);
    let y = (0.5 * H as f64) as u32;
    for x in 0..((0.10 * W as f64) as u32) {
        let p = px(&f, x, y);
        assert!(p[3] < 0.05 || (p[0] + p[1] + p[2]) < 0.05, "far from the shaft with no label, region should stay background at x={x}: {:?}", p);
    }
}

#[test]
fn dry_run_changes_nothing() {
    let mut s = project();
    let before = serde_json::to_string(&s.project).unwrap();
    let r = c(&mut s, "add_arrow", json!({"target": {"x": 0.8, "y": 0.5}, "at_s": 0.0, "dry_run": true}));
    assert_eq!(r["dry_run"], json!(true));
    assert_eq!(serde_json::to_string(&s.project).unwrap(), before, "dry_run must not change the project");
}

#[test]
fn undo_restores_the_timeline_exactly() {
    let mut s = project();
    let before = serde_json::to_string(&s.project).unwrap();
    c(&mut s, "add_arrow", horizontal_args(0.0, 2.0));
    assert_ne!(serde_json::to_string(&s.project).unwrap(), before, "sanity: add_arrow should have changed the project");
    c(&mut s, "undo", json!({}));
    assert_eq!(serde_json::to_string(&s.project).unwrap(), before, "undo must restore the exact pre-add_arrow timeline in one step");
}

#[test]
fn bad_args_error() {
    let mut s = project();
    assert!(dispatch(&mut s, "add_arrow", &json!({"target": {"x": 1.5, "y": 0.5}, "at_s": 0.0})).is_err(), "target.x outside 0..1 must error");
    assert!(dispatch(&mut s, "add_arrow", &json!({"target": {"x": 0.5, "y": -0.2}, "at_s": 0.0})).is_err(), "target.y outside 0..1 must error");
    assert!(dispatch(&mut s, "add_arrow", &json!({"target": {"x": 0.8, "y": 0.5}, "from": {"x": 0.2, "y": 0.5}, "at_s": 0.0, "duration_s": -1.0})).is_err(), "negative duration_s must error");
    assert!(dispatch(&mut s, "add_arrow", &json!({"target": {"x": 0.8, "y": 0.5}, "at_s": -1.0})).is_err(), "negative at_s must error");
    assert!(dispatch(&mut s, "add_arrow", &json!({"at_s": 0.0})).is_err(), "missing target must error");
    assert!(dispatch(&mut s, "add_arrow", &json!({"target": {"x": 0.5, "y": 0.5}})).is_err(), "missing at_s must error");
    assert!(dispatch(&mut s, "add_arrow", &json!({"target": {"x": 0.8, "y": 0.5}, "from": {"x": 0.8, "y": 0.5}, "at_s": 0.0})).is_err(), "from == target must error");
    assert!(dispatch(&mut s, "add_arrow", &json!({"target": {"x": 0.8, "y": 0.5}, "at_s": 0.0, "animate": "spin"})).is_err(), "unknown animate must error");
    assert!(dispatch(&mut s, "add_arrow", &json!({"target": {"x": 0.8, "y": 0.5}, "at_s": 0.0, "bogus_arg": 1})).is_err(), "unknown argument must error");
}

fn catalogs(ss: &mut Session) -> (Value, Value) {
    (dispatch(ss, "list_effects", &json!({})).unwrap_or(json!([])), dispatch(ss, "list_transitions", &json!({})).unwrap_or(json!([])))
}
fn top_names(ss: &mut Session, q: &str, limit: usize) -> Vec<String> {
    let (ef, tr) = catalogs(ss);
    let r = search_tools(q, limit, None, &ef, &tr);
    r["matches"].as_array().unwrap().iter().filter_map(|m| m["name"].as_str().map(str::to_string)).collect()
}

#[test]
fn search_tools_ranks_add_arrow_in_the_top_3_for_the_natural_phrasings() {
    let mut ss = Session::new();
    for q in ["arrow", "point at something", "callout", "look here arrow"] {
        let names = top_names(&mut ss, q, 8);
        assert!(names.iter().take(3).any(|n| n == "add_arrow"), "query {q:?}: add_arrow not in top 3: {names:?}");
    }
}
