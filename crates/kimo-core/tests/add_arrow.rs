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


/// Vertical project (width is the shorter side), the same shape as a real 720x1280 phone clip -- the
/// edge case below only reproduces on a frame where width is the min dimension.
fn vertical_project() -> Session { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 360, "height": 640, "fps": FPS})); s }

/// KF17b: reproduces the real-footage bug exactly. With NO explicit `from`, the auto default continues
/// the center->target ray outward by 0.25 of the frame's shorter side; when `target.x` is exactly 0.25
/// (and y is centered), that 0.25 cancels algebraically and lands the tail (`from_px`) precisely on the
/// left edge -- the exact shape of the real clip1.mp4 left-bottle case where the "THIS ONE" label was
/// clamped back onto the shaft and swallowed the arrowhead.
fn edge_case_args(label: &str, animate: &str) -> Value {
    json!({"target": {"x": 0.25, "y": 0.5}, "at_s": 0.0, "duration_s": 2.0, "animate": animate, "label": label})
}

#[test]
fn label_bbox_never_overlaps_the_shaft_or_head() {
    let mut s = vertical_project();
    let r = c(&mut s, "add_arrow", edge_case_args("THIS ONE", "pop")); // pop: fully in almost immediately
    let (fx, fy) = (r["from_px"]["x"].as_f64().unwrap(), r["from_px"]["y"].as_f64().unwrap());
    let (tx, ty) = (r["target_px"]["x"].as_f64().unwrap(), r["target_px"]["y"].as_f64().unwrap());
    let thickness = r["thickness_px"].as_f64().unwrap();
    // Same conservative head-radius relationship the tool itself uses (thickness scaled to frame, head
    // radius ~4.2x it, floor 16px) -- no private helper is exposed to call directly, so this mirrors the
    // tool's own documented sizing rather than guessing a number.
    let head_r = (thickness * 4.2_f64).max(16.0);
    let forbid_half_w = head_r + thickness / 2.0 + 6.0;
    let (x0, x1) = (fx.min(tx) - forbid_half_w, fx.max(tx) + forbid_half_w);
    let (y0, y1) = (fy.min(ty) - forbid_half_w, fy.max(ty) + forbid_half_w);

    let f = rendered(&s, 1.0); // well past "pop"'s brief fade-in
    let is_white_ish = |p: [f32; 4]| p[3] > 0.3 && p[0] > 0.8 && p[1] > 0.8 && p[2] > 0.8; // the label's own white fill
    let (fw, fh) = (f.w as i64, f.h as i64);
    let ix0 = x0.floor().max(0.0) as u32;
    let ix1 = x1.ceil().min(fw as f64 - 1.0).max(0.0) as u32;
    let iy0 = y0.floor().max(0.0) as u32;
    let iy1 = y1.ceil().min(fh as f64 - 1.0).max(0.0) as u32;

    let mut overlap = false;
    for y in iy0..=iy1 { for x in ix0..=ix1 { if is_white_ish(px(&f, x, y)) { overlap = true; } } }
    assert!(!overlap, "label (white fill) pixels found inside the shaft/head's own bounding region [{ix0}..{ix1}]x[{iy0}..{iy1}] -- the label overlaps the arrow");

    // Sanity: the label must still actually be drawn somewhere, so this isn't vacuously passing because
    // nothing rendered at all.
    let mut label_drawn_elsewhere = false;
    'outer: for y in 0..f.h as u32 {
        for x in 0..f.w as u32 {
            let inside_forbidden = (ix0..=ix1).contains(&x) && (iy0..=iy1).contains(&y);
            if !inside_forbidden && is_white_ish(px(&f, x, y)) { label_drawn_elsewhere = true; break 'outer; }
        }
    }
    assert!(label_drawn_elsewhere, "sanity: label should be visible somewhere outside the forbidden zone");
}

#[test]
fn arrowhead_is_visible_and_wider_than_the_shaft_even_with_a_label() {
    let mut s = vertical_project();
    let r = c(&mut s, "add_arrow", edge_case_args("THIS ONE", "draw")); // default animate, matches the real-footage check's timing
    let (fx, _fy) = (r["from_px"]["x"].as_f64().unwrap(), r["from_px"]["y"].as_f64().unwrap());
    let (tx, ty) = (r["target_px"]["x"].as_f64().unwrap(), r["target_px"]["y"].as_f64().unwrap());

    let f = rendered(&s, 0.6); // at_s(0.0) + 0.6, the same mid-hold offset the real-footage check used
    let is_yellowish = |p: [f32; 4]| p[3] > 0.3 && p[0] > 0.6 && p[1] > 0.35 && p[2] < 0.35;
    let target_xy = (tx.round().clamp(0.0, f.w as f64 - 1.0) as u32, ty.round().clamp(0.0, f.h as f64 - 1.0) as u32);
    assert!(is_yellowish(px(&f, target_xy.0, target_xy.1)), "arrowhead pixels should be visible AT the target by at_s+0.6, not hidden behind the label: {:?}", px(&f, target_xy.0, target_xy.1));

    // "head clearly wider than the shaft": compare the yellow vertical span a few px into the head (back
    // toward the tail from the target, so still inside the triangle, not past it) against the yellow
    // vertical span mid-shaft (clear of the head) -- the shaft here is horizontal (target.y == from.y), so
    // "width" is the vertical extent at a fixed column.
    let yellow_span = |x: u32| -> u32 {
        let (mut lo, mut hi): (Option<u32>, Option<u32>) = (None, None);
        for y in 0..f.h as u32 { if is_yellowish(px(&f, x, y)) { lo.get_or_insert(y); hi = Some(y); } }
        match (lo, hi) { (Some(a), Some(b)) => b - a + 1, _ => 0 }
    };
    let dir_to_tail = if fx < tx { -1.0 } else { 1.0 };
    let head_col = (tx + dir_to_tail * 6.0).round().clamp(0.0, f.w as f64 - 1.0) as u32;
    let shaft_col = ((fx + tx) / 2.0).round().clamp(0.0, f.w as f64 - 1.0) as u32;
    let head_span = yellow_span(head_col);
    let shaft_span = yellow_span(shaft_col);
    assert!(head_span > 0, "head column should have some yellow span: {head_span}");
    assert!(shaft_span > 0, "shaft column should have some yellow span: {shaft_span}");
    assert!(head_span as f64 > shaft_span as f64 * 1.5, "head should read clearly wider than the shaft: head_span={head_span} shaft_span={shaft_span}");
}

#[test]
fn search_tools_ranks_add_arrow_in_the_top_3_for_the_natural_phrasings() {
    let mut ss = Session::new();
    for q in ["arrow", "point at something", "callout", "look here arrow"] {
        let names = top_names(&mut ss, q, 8);
        assert!(names.iter().take(3).any(|n| n == "add_arrow"), "query {q:?}: add_arrow not in top 3: {names:?}");
    }
}
