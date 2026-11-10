//! AE shape path operators via add_path_operator: zig zag puts its ridges around the outline, wiggle moves the outline over time
//! within its size, pucker/bloat pulls the shape toward a star or a circle, twist turns the rim more than the centre, and round
//! corners fillets rect and polygon corners exactly while the edges stay put.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 160; const H: usize = 120; const CX: f64 = 80.0; const CY: f64 = 60.0;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn shape(kind: &str, w: f64, h: f64, extra: Value) -> (Session, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let mut a = json!({"track_id": t, "shape": kind, "width": w, "height": h, "fill": [1, 1, 1, 1], "start": 0.0, "duration": 3.0}); if let Some(o) = extra.as_object() { for (k, v) in o { a[k] = v.clone(); } }
    let id = c(&mut s, "add_shape", a)["clip_id"].as_str().unwrap().to_string(); (s, id)
}
fn alpha_frame(s: &Session, t: f64) -> Vec<f32> { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); let f = render::render_frame(&s.project, t, &cache).unwrap();
    (0..W * H).map(|i| 0.2126 * f.d[i * 4] + 0.7152 * f.d[i * 4 + 1] + 0.0722 * f.d[i * 4 + 2]).collect() }
fn at(f: &[f32], x: f64, y: f64) -> f32 { f[(y.round() as usize) * W + x.round() as usize] }
fn polar(f: &[f32], r: f64, deg: f64) -> f32 { at(f, CX + r * deg.to_radians().cos(), CY + r * deg.to_radians().sin()) }
fn op(s: &mut Session, id: &str, op: &str, params: Value) -> String { c(s, "add_path_operator", json!({"clip_id": id, "op": op, "params": params}))["effect_id"].as_str().unwrap().to_string() }

#[test]
fn zig_zag_puts_its_ridges_around_the_outline() {
    let (mut s, id) = shape("ellipse", 60.0, 60.0, json!({})); op(&mut s, &id, "zig_zag", json!({"size": 6, "ridges": 12}));
    let f = alpha_frame(&s, 0.5);
    // inside/outside along the radius-30 circle with hysteresis (in above 0.75, out below 0.25) so anti-aliased pixels do not count as crossings
    let v: Vec<f32> = (0..720).map(|i| polar(&f, 30.0, i as f64 * 0.5)).collect(); let mut cur = v.iter().rev().find(|x| **x > 0.75 || **x < 0.25).map_or(false, |x| *x > 0.75);
    let inside: Vec<bool> = v.iter().map(|x| { if *x > 0.75 { cur = true } else if *x < 0.25 { cur = false } cur }).collect();
    let flips = (0..720).filter(|&i| inside[i] != inside[(i + 1) % 720]).count();
    assert_eq!(flips, 24, "12 ridges must cross the radius-30 circle 24 times");
    for i in 0..72 { let d = i as f64 * 5.0; assert!(polar(&f, 22.0, d) > 0.5 && polar(&f, 38.0, d) < 0.5, "ridges must stay within +-6 px of the outline (at {d} deg)"); }
}

#[test]
fn wiggle_moves_the_outline_over_time_within_its_size() {
    let (mut s, id) = shape("ellipse", 60.0, 60.0, json!({})); op(&mut s, &id, "wiggle", json!({"size": 6, "detail": 10, "speed": 1, "seed": 3}));
    let (a, b, a2) = (alpha_frame(&s, 0.2), alpha_frame(&s, 1.7), alpha_frame(&s, 0.2));
    let diff = |x: &[f32], y: &[f32]| x.iter().zip(y).filter(|(p, q)| (*p - *q).abs() > 0.5).count();
    assert_eq!(diff(&a, &a2), 0, "the same time must render the same outline");
    assert!(diff(&a, &b) > 60, "the outline must move between 0.2 s and 1.7 s: {} px changed", diff(&a, &b));
    let base = { let (s0, _) = shape("ellipse", 60.0, 60.0, json!({})); alpha_frame(&s0, 0.2) }; assert!(diff(&a, &base) > 60, "wiggle must change the outline: {} px", diff(&a, &base));
    for i in 0..72 { let d = i as f64 * 5.0; assert!(polar(&b, 22.0, d) > 0.5 && polar(&b, 38.0, d) < 0.5, "wiggle must stay within its 6 px (at {d} deg)"); }
}

#[test]
fn pucker_makes_a_star_and_bloat_makes_a_circle() {
    // a 60x60 square: edge midpoints at r = 30, corners at r = 42.4
    let (mut s, id) = shape("rect", 60.0, 60.0, json!({})); let e = op(&mut s, &id, "pucker_bloat", json!({"amount": -1}));
    let p = alpha_frame(&s, 0.5);
    assert!(polar(&p, 46.0, 45.0) > 0.5, "pucker must stretch the corners out (r 46 on the diagonal is {})", polar(&p, 46.0, 45.0));
    assert!(polar(&p, 28.0, 0.0) < 0.5 && polar(&p, 28.0, 90.0) < 0.5, "pucker must pull the edges in (r 28 on the edge normals: {} {})", polar(&p, 28.0, 0.0), polar(&p, 28.0, 90.0));
    c(&mut s, "add_path_operator", json!({"clip_id": id, "op": "pucker_bloat", "effect_id": e, "params": {"amount": 1}}));
    let b = alpha_frame(&s, 0.5);
    assert!(polar(&b, 32.5, 0.0) > 0.5 && polar(&b, 36.0, 45.0) < 0.5, "bloat 1 must give a circle of the mean radius (~33.7): edge r 32.5 {} corner r 36 {}", polar(&b, 32.5, 0.0), polar(&b, 36.0, 45.0));
}

#[test]
fn twist_turns_the_rim_more_than_the_centre() {
    let (mut s, id) = shape("rect", 80.0, 12.0, json!({})); op(&mut s, &id, "twist", json!({"angle": 90}));
    let f = alpha_frame(&s, 0.5); let tip = 90.0 * 38.0 / (40.0f64.hypot(6.0));
    assert!(at(&f, CX + 38.0, CY) < 0.5, "the bar's tip must have turned away from 0 deg");
    assert!(polar(&f, 38.0, tip) > 0.5 || polar(&f, 38.0, tip + 180.0) > 0.5, "the tip must sit near {tip:.0} deg");
    assert!(at(&f, CX + 4.0, CY) > 0.5, "near the centre the bar barely turns");
}

#[test]
fn round_corners_fillets_corners_and_keeps_edges() {
    let (mut s, id) = shape("rect", 60.0, 40.0, json!({})); op(&mut s, &id, "round_corners", json!({"radius": 12}));
    let f = alpha_frame(&s, 0.5);
    assert!(at(&f, CX + 28.5, CY + 18.5) < 0.1, "the corner must be cut by the fillet, alpha {}", at(&f, CX + 28.5, CY + 18.5));
    assert!(at(&f, CX + 28.5, CY) > 0.9 && at(&f, CX, CY + 18.5) > 0.9, "edge midpoints stay");
    let (mut s2, id2) = shape("polygon", 60.0, 60.0, json!({"sides": 6})); op(&mut s2, &id2, "round_corners", json!({"radius": 14}));
    let (s0, _) = shape("polygon", 60.0, 60.0, json!({"sides": 6}));
    let (g, g0) = (alpha_frame(&s2, 0.5), alpha_frame(&s0, 0.5)); let corner_px = |f: &[f32]| (0..W * H).filter(|&i| f[i] > 0.5).count();
    assert!(corner_px(&g) < corner_px(&g0) && corner_px(&g0) - corner_px(&g) > 30, "rounding a hexagon must remove its 6 corner tips: {} -> {} px", corner_px(&g0), corner_px(&g));
    assert!(polar(&g, 24.0, 0.0) > 0.9 || polar(&g, 24.0, 30.0) > 0.9, "the hexagon body stays");
}

#[test]
fn refusals_name_the_problem_and_change_nothing() {
    let (mut s, id) = shape("ellipse", 40.0, 40.0, json!({})); let before = serde_json::to_string(&s.project).unwrap();
    for (a, want) in [(json!({"clip_id": id, "op": "round_corners"}), "rect and polygon"), (json!({"clip_id": id, "op": "spiral"}), "unknown op 'spiral'"),
        (json!({"clip_id": id, "op": "zig_zag", "params": {"teeth": 3}}), "unknown param 'teeth'"), (json!({"clip_id": id, "op": "pucker_bloat", "params": {"amount": 3}}), "out of range"),
        (json!({"clip_id": id, "op": "twist", "effect_id": "fx99"}), "no path operator 'fx99'"), (json!({"clip_id": id, "op": "twist", "angel": 3}), "unknown argument 'angel'")] {
        let e = dispatch(&mut s, "add_path_operator", &a).expect_err(&format!("{a} must be refused")); assert!(e.contains(want), "{a}: '{e}' lacks '{want}'"); }
    assert_eq!(before, serde_json::to_string(&s.project).unwrap(), "refusals must leave the project unchanged");
}
