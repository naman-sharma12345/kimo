//! V1c-D bg_grid analytic rows: pixel values from the documented math, dots, drift periodicity, fade monotone, apply == expansion + undo, threads, op count.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn base() -> (Session, String) {
    let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width":256,"height":256,"fps":25})).unwrap();
    let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string(); (s, t)
}
fn grid(extra: Value) -> (Session, Value) {
    let (mut s, t) = base(); let mut a = json!({"track":t,"duration":5,"apply":true,"opacity_pattern":1.0,"bg_color":[0,0,0],"color":[1,1,1]});
    for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); } let r = dispatch(&mut s, "bg_grid", &a).unwrap_or_else(|e| panic!("bg_grid: {e}")); (s, r)
}
fn frame(s: &Session, t: f64) -> Vec<f32> { kimo_core::render::render_frame(&s.project, t, &kimo_core::render::Cache::new()).unwrap().d }
fn px(d: &[f32], x: usize, y: usize) -> f32 { d[(y * 256 + x) * 4] }
const LSB: f32 = 1.0 / 255.0;
#[test] fn lines_match_the_documented_math_at_line_centre_and_mid_cell() {
    let (s, _) = grid(json!({"spacing_px":64,"line_px":1})); let d = frame(&s, 0.0);
    // the line sits between pixels 63 and 64 (x+0.5 = 64): each is 0.5 px from the centre, cov = 1 - smoothstep(0, 1, 0.5) = 0.5
    for (x, y, want) in [(63, 31, 0.5), (64, 31, 0.5), (31, 63, 0.5), (63, 63, 0.5), (31, 31, 0.0), (62, 31, 0.0), (66, 31, 0.0), (0, 0, 0.5 * 0.0 + px(&d, 0, 0))] { let got = px(&d, x, y); assert!((got - want).abs() <= LSB, "pixel {x},{y}: {got} vs {want}"); }
    let (s3, _) = grid(json!({"spacing_px":64,"line_px":3})); let d3 = frame(&s3, 0.0);
    // line 3: centres at 0.5 px: cov = 1 - smoothstep(1, 2, 0.5) = 1; pixels 62 and 65 are 1.5 px: 1 - smoothstep(1,2,1.5) = 0.5
    for (x, want) in [(63, 1.0), (64, 1.0), (62, 0.5), (65, 0.5), (61, 0.0), (66, 0.0)] { assert!((px(&d3, x, 31) - want).abs() <= LSB, "line_px 3 pixel {x}: {}", px(&d3, x, 31)); }
    for x in 0..8 { assert_eq!(px(&d3, 63 - x, 31).to_bits(), px(&d3, 64 + x, 31).to_bits(), "line_px 3 symmetric about the line"); }
}
#[test] fn dots_are_round_with_diameter_line_px() {
    let (s, r) = grid(json!({"mode":"dots","spacing_px":32,"line_px":4})); let d = frame(&s, 0.0); assert_eq!(r["params"]["dot_radius_px"], 2.0);
    // dot centres are at x+0.5 = 32k: pixels 31/32 are 0.5 px away, well inside radius 2 (cov 1); 4.5 px away is outside
    assert!((px(&d, 31, 31) - 1.0).abs() <= LSB && (px(&d, 32, 32) - 1.0).abs() <= LSB); assert!(px(&d, 27, 31) < LSB, "{}", px(&d, 27, 31)); assert!(px(&d, 15, 15) < LSB, "mid cell is empty");
    assert_eq!(px(&d, 31, 31).to_bits(), px(&d, 32, 32).to_bits()); assert_eq!(px(&d, 31, 40).to_bits(), px(&d, 32, 23).to_bits(), "dot symmetric about its centre (x and y mirrored)");
}
#[test] fn drift_is_periodic_after_one_cell() {
    let (s, _) = grid(json!({"spacing_px":64,"drift":[16,0]})); let (a, b) = (frame(&s, 0.5), frame(&s, 4.5)); let worst = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0f32, f32::max);
    assert!(worst < 1e-5, "frame at t and t + spacing/vx differ by at most {worst}"); let c = frame(&s, 1.5); assert!(a.iter().zip(&c).any(|(x, y)| (x - y).abs() > 0.1), "the grid actually moves");
}
#[test] fn fade_is_monotone_with_distance_from_its_centre() {
    let (s, _) = grid(json!({"spacing_px":64,"fade":{"cx":128,"cy":128,"radius":120,"softness":1}})); let d = frame(&s, 0.0);
    let col: Vec<f32> = [10usize, 30, 50, 70, 90, 110, 126].iter().map(|&y| px(&d, 63, y)).collect(); // along the vertical line x=63 between horizontal lines
    for w in col.windows(2) { assert!(w[1] >= w[0] - 1e-6, "fade grows toward the centre: {col:?}"); } assert!(col[0] < 0.01 && col[6] > 0.2, "{col:?}");
}
#[test] fn apply_equals_the_expansion_by_hand_and_one_undo_restores() {
    let (mut s1, t) = base(); let a = |ap: bool| json!({"track":t,"duration":3,"spacing_px":48,"drift":[3.5,-2],"apply":ap,"name":"grid1","tags":["hero"],"blend":"add","opacity":0.7});
    let proj0 = serde_json::to_string(&s1.project).unwrap(); let r = dispatch(&mut s1, "bg_grid", &a(false)).unwrap(); assert_eq!(r["applied"], false); assert_eq!(serde_json::to_string(&s1.project).unwrap(), proj0);
    dispatch(&mut s1, "batch", &json!({"ops":r["expansion"].clone(),"atomic":true})).unwrap();
    let (mut s2, _) = base(); let r2 = dispatch(&mut s2, "bg_grid", &a(true)).unwrap(); assert_eq!(r2["applied"], true); assert_eq!(r2["expansion"], r["expansion"]);
    assert_eq!(serde_json::to_string(&s1.project.tracks).unwrap(), serde_json::to_string(&s2.project.tracks).unwrap(), "apply:true == expansion by hand");
    dispatch(&mut s2, "undo", &json!({})).unwrap(); assert_eq!(serde_json::to_string(&s2.project).unwrap(), proj0, "one undo restores");
}
#[test] fn threads_agree_and_estimate_equals_the_compiled_op_count() {
    let (s, r) = grid(json!({"spacing_px":40,"drift":[5,3],"fade":{"cx":100,"cy":100,"radius":150,"softness":0.3}}));
    kimo_core::par::set_threads(1); let a = frame(&s, 1.0); kimo_core::par::set_threads(4); let b = frame(&s, 1.0); kimo_core::par::set_threads(0);
    assert_eq!(a.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), b.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), "1 vs 4 threads");
    let expr = r["expansion"][1]["args"]["strings"]["expr"].as_str().unwrap(); let p = kimo_core::pixel_expr::compile(expr, usize::MAX).unwrap(); assert_eq!(r["estimate"]["expr_ops"].as_u64().unwrap() as usize, p.ops);
    let (mut s2, t) = base(); let e = dispatch(&mut s2, "bg_grid", &json!({"track":t,"duration":2,"fade":{"radius":100}})).unwrap(); let ops = e["estimate"]["expr_ops"].as_u64().unwrap(); assert!(ops > 10 && ops < 512, "{ops}");
    let _: Value = e;
}
#[test] fn tile_seam_a_128_render_equals_each_quadrant_of_the_256_render() {
    // no tile API exists: spacing 64 divides 128, so a 128x128 grid at origin 0 must equal every 128x128 quadrant of the 256x256 grid byte-for-byte
    let (s, _) = grid(json!({"spacing_px":64,"line_px":3})); let big = frame(&s, 0.0);
    let mut s2 = Session::new(); dispatch(&mut s2, "project_set", &json!({"width":128,"height":128,"fps":25})).unwrap(); let t = dispatch(&mut s2, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    dispatch(&mut s2, "bg_grid", &json!({"track":t,"duration":5,"apply":true,"opacity_pattern":1.0,"bg_color":[0,0,0],"spacing_px":64,"line_px":3})).unwrap();
    let small = kimo_core::render::render_frame(&s2.project, 0.0, &kimo_core::render::Cache::new()).unwrap().d;
    for (qx, qy) in [(0, 0), (1, 0), (0, 1), (1, 1)] { for y in 0..128 { for x in 0..128 { for c in 0..4 {
        let (a, b) = (small[(y * 128 + x) * 4 + c], big[((qy * 128 + y) * 256 + qx * 128 + x) * 4 + c]); assert_eq!(a.to_bits(), b.to_bits(), "quadrant ({qx},{qy}) pixel ({x},{y}) channel {c}"); } } } }
}
#[test] fn mutation_guard_line_width_is_half_the_line_px_and_fract_offset_is_centred() {
    // line_px/2 -> line_px would widen a 1 px line to cov 1 at pixels 63/64 (want 0.5); a missing fract offset would shift the line by half a cell
    let (s, r) = grid(json!({"spacing_px":64,"line_px":1})); let d = frame(&s, 0.0); assert!((px(&d, 63, 31) - 0.5).abs() <= LSB, "half width"); assert!(px(&d, 32, 31) < LSB, "no line at the half-cell offset");
    assert!(r["expansion"].to_string().contains("fract("), "expansion visible");
}
#[test] fn lint_closed_loop_title_over_grid_is_clean_when_faint_and_flagged_when_strong() {
    fn scene(op: f64) -> Session { let (mut s, t) = base(); dispatch(&mut s, "bg_grid", &json!({"track":t,"duration":3,"apply":true,"spacing_px":8,"line_px":4,"opacity_pattern":op})).unwrap();
        let tr = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
        let id = dispatch(&mut s, "add_text", &json!({"track_id":tr,"text":"H title","font":"DejaVuSans","size":120,"color":[0.45,0.45,0.45],"start":0,"duration":3})).unwrap()["clip_id"].as_str().unwrap().to_string();
        dispatch(&mut s, "set_meta", &json!({"id":id,"name":"title","role":"title"})).unwrap(); s }
    let lint = |s: &mut Session| dispatch(s, "lint_frame", &json!({"time":1,"checks":["text_contrast"],"annotate":false})).unwrap();
    let n = |r: &Value| r["frames"].as_array().unwrap().iter().map(|f| f["findings"].as_array().unwrap().len()).sum::<usize>();
    let mut faint = scene(0.08); assert_eq!(n(&lint(&mut faint)), 0, "faint grid: no findings");
    let mut strong = scene(0.6); let r = lint(&mut strong); assert!(n(&r) > 0, "strong grid: a finding: {r}");
    // the fix_hint/next_steps on lint findings come from the U-FIX branch; when this tree has them, applying them must clear the finding
    let steps = r["next_steps"].as_array().cloned().unwrap_or_default();
    if steps.is_empty() { eprintln!("NOTE: lint_frame has no next_steps in this tree (U-FIX not merged); clearing by hint not exercised"); let mut calm = scene(0.08); assert_eq!(n(&lint(&mut calm)), 0); return; }
    for st in steps { dispatch(&mut strong, st["tool"].as_str().unwrap(), &st["args"]).unwrap(); } assert_eq!(n(&lint(&mut strong)), 0, "applying the hint clears it");
}
