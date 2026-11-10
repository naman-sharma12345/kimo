//! RV-27 lane A5: transform nodes (add_null, set_parent, get_transform) and the single transform resolver.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
fn sid(v: &Value) -> String { v["clip_id"].as_str().unwrap().to_string() }
fn scene() -> (Session, String) { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); (ss, t) }
fn rect(ss: &mut Session, t: &str) -> String { sid(&c(ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "start": 0.0, "duration": 30.0}))) }
fn keys(ss: &mut Session, id: &str, p: &str, ks: Value) { c(ss, "set_keyframes", json!({"clip_id": id, "property": p, "keys": ks})); }
fn xf(ss: &mut Session, id: &str, t: f64, space: &str) -> Value { c(ss, "get_transform", json!({"clip_id": id, "t": t, "space": space})) }
fn f(v: &Value) -> f64 { v.as_f64().unwrap() }
const TIMES: [f64; 5] = [0.0, 0.7, 1.5, 2.25, 3.9];
fn lerp(a: f64, b: f64, t: f64, t1: f64) -> f64 { a + (b - a) * (t / t1) }

/// closed form of the render convention: f = c + d + R(rot) S (q - c), world = parent o local
fn closed_corners(dp: (f64, f64), rp: f64, sp: f64, dc: (f64, f64), rc: f64, sc: f64, w: f64, h: f64) -> Vec<(f64, f64)> {
    let (r, s) = (rp.to_radians(), sp); let dw = (dp.0 + s * (r.cos() * dc.0 - r.sin() * dc.1), dp.1 + s * (r.sin() * dc.0 + r.cos() * dc.1)); let (rw, sw) = ((rp + rc).to_radians(), sp * sc); let (cx, cy) = (w / 2.0, h / 2.0);
    [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].iter().map(|(x, y)| { let (qx, qy) = (x - cx, y - cy); (cx + dw.0 + sw * (rw.cos() * qx - rw.sin() * qy), cy + dw.1 + sw * (rw.sin() * qx + rw.cos() * qy)) }).collect()
}
#[test] fn child_follows_parent_analytically_at_five_times() {
    let (mut ss, t) = scene(); let p = sid(&c(&mut ss, "add_null", json!({"track_id": t, "start": 0.0, "duration": 30.0}))); let ch = rect(&mut ss, &t);
    keys(&mut ss, &p, "x", json!([{"t": 0.0, "v": 0.0, "ease": "linear"}, {"t": 4.0, "v": 40.0, "ease": "linear"}])); keys(&mut ss, &p, "y", json!([{"t": 0.0, "v": 5.0, "ease": "linear"}, {"t": 4.0, "v": -15.0, "ease": "linear"}]));
    keys(&mut ss, &p, "rotation", json!([{"t": 0.0, "v": 0.0, "ease": "linear"}, {"t": 4.0, "v": 90.0, "ease": "linear"}])); keys(&mut ss, &p, "scale", json!([{"t": 0.0, "v": 1.0, "ease": "linear"}, {"t": 4.0, "v": 2.0, "ease": "linear"}]));
    c(&mut ss, "set_property", json!({"clip_id": ch, "property": "x", "value": 12.0})); c(&mut ss, "set_property", json!({"clip_id": ch, "property": "y", "value": -7.0})); c(&mut ss, "set_property", json!({"clip_id": ch, "property": "rotation", "value": 30.0})); c(&mut ss, "set_property", json!({"clip_id": ch, "property": "scale", "value": 1.5}));
    c(&mut ss, "set_parent", json!({"clip_id": ch, "parent_id": p, "mode": "keep_local"}));
    for &tt in &TIMES { let r = xf(&mut ss, &ch, tt, "canvas_px");
        let want = closed_corners((lerp(0.0, 40.0, tt, 4.0), lerp(5.0, -15.0, tt, 4.0)), lerp(0.0, 90.0, tt, 4.0), lerp(1.0, 2.0, tt, 4.0), (12.0, -7.0), 30.0, 1.5, 160.0, 90.0);
        for (i, (wx, wy)) in want.iter().enumerate() { let g = &r["quad_canvas_px"][i]; assert!((f(&g[0]) - wx).abs() < 1e-9 && (f(&g[1]) - wy).abs() < 1e-9, "t={tt} corner {i}: {g} vs {wx},{wy}"); } }
}
#[test] fn keep_world_preserves_pose_keep_local_moves_it() {
    let (mut ss, t) = scene(); let p = sid(&c(&mut ss, "add_null", json!({"track_id": t, "start": 0.0, "duration": 30.0, "x": 30.0, "y": 10.0, "rotation": 45.0, "scale": 2.0, "opacity": 0.5}))); let ch = rect(&mut ss, &t);
    c(&mut ss, "set_property", json!({"clip_id": ch, "property": "x", "value": 8.0})); c(&mut ss, "set_property", json!({"clip_id": ch, "property": "rotation", "value": 10.0}));
    let before = xf(&mut ss, &ch, 1.0, "world"); let r = c(&mut ss, "set_parent", json!({"clip_id": ch, "parent_id": p})); assert!(r["adjusted"].is_object()); let after = xf(&mut ss, &ch, 1.0, "world");
    for k in ["x", "y", "scale", "rotation", "opacity"] { assert!((f(&before["world"][k]) - f(&after["world"][k])).abs() < 1e-9, "{k}: {before} vs {after}"); }
    let (mut s2, t2) = scene(); let p2 = sid(&c(&mut s2, "add_null", json!({"track_id": t2, "start": 0.0, "duration": 30.0, "x": 30.0, "scale": 2.0}))); let c2 = rect(&mut s2, &t2); let b = xf(&mut s2, &c2, 1.0, "world");
    c(&mut s2, "set_parent", json!({"clip_id": c2, "parent_id": p2, "mode": "keep_local"})); let a = xf(&mut s2, &c2, 1.0, "world"); assert!((f(&a["world"]["x"]) - f(&b["world"]["x"]) - 30.0).abs() < 1e-9); assert!((f(&a["world"]["scale"]) - 2.0).abs() < 1e-12);
    assert!(e(&mut s2, "set_parent", json!({"clip_id": c2, "parent_id": p2, "mode": "zip"})).contains("Valid:"));
}
#[test] fn cycles_self_parent_and_depth_cap_are_named_errors_and_change_nothing() {
    let (mut ss, t) = scene(); let a = rect(&mut ss, &t); let b = rect(&mut ss, &t); c(&mut ss, "set_parent", json!({"clip_id": b, "parent_id": a})); let h = pj(&ss); let u = ss.undo.len();
    assert!(e(&mut ss, "set_parent", json!({"clip_id": a, "parent_id": a})).contains("parent_cycle")); assert!(e(&mut ss, "set_parent", json!({"clip_id": a, "parent_id": b})).contains("parent_cycle"));
    assert!(e(&mut ss, "set_parent", json!({"clip_id": a, "parent_id": "nope"})).contains("nope")); assert!(e(&mut ss, "set_parent", json!({"clip_id": a, "parent_id": b, "inherit": {"time": true}})).len() > 0);
    assert_eq!(pj(&ss), h); assert_eq!(ss.undo.len(), u);
    let (mut s2, t2) = scene(); let ids: Vec<String> = (0..66).map(|_| rect(&mut s2, &t2)).collect();
    for i in 1..=64 { c(&mut s2, "set_parent", json!({"clip_id": ids[i], "parent_id": ids[i - 1], "mode": "keep_local"})); }
    assert!(e(&mut s2, "set_parent", json!({"clip_id": ids[65], "parent_id": ids[64], "mode": "keep_local"})).contains("parent_depth"));
}
#[test] fn unparent_restores_undo_restores_and_old_projects_stay_byte_equal() {
    let (mut ss, t) = scene(); let p = sid(&c(&mut ss, "add_null", json!({"track_id": t, "start": 0.0, "duration": 30.0, "x": 20.0, "rotation": 15.0}))); let ch = rect(&mut ss, &t);
    let h0 = pj(&ss); assert!(!h0.contains("parent") && !h0.contains("inherit"), "old shape: no new fields when unused");
    let w = xf(&mut ss, &ch, 1.0, "world"); c(&mut ss, "set_parent", json!({"clip_id": ch, "parent_id": p})); assert!(pj(&ss).contains("parent"));
    c(&mut ss, "set_parent", json!({"clip_id": ch, "parent_id": null})); let w2 = xf(&mut ss, &ch, 1.0, "world"); assert_eq!(w["world"], w2["world"]); assert!(!pj(&ss).contains("\"parent\""));
    c(&mut ss, "undo", json!({})); assert!(pj(&ss).contains("\"parent\"")); c(&mut ss, "undo", json!({})); assert_eq!(pj(&ss), h0);
}
fn hash_frame(ss: &Session, t: f64) -> u64 { let fr = render::render_frame(&ss.project, t, &render::Cache::new()).unwrap(); let mut h: u64 = 0xcbf29ce484222325; for v in &fr.d { for b in v.to_le_bytes() { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); } } h }
#[test] fn rendered_parented_child_equals_hand_flattened_child_bit_for_bit() {
    for &tt in &[0.0, 1.0, 2.5] {
        let (mut ss, t) = scene(); let p = sid(&c(&mut ss, "add_null", json!({"track_id": t, "start": 0.0, "duration": 30.0}))); let ch = rect(&mut ss, &t);
        keys(&mut ss, &p, "x", json!([{"t": 0.0, "v": 0.0, "ease": "linear"}, {"t": 4.0, "v": 40.0, "ease": "linear"}])); keys(&mut ss, &p, "rotation", json!([{"t": 0.0, "v": 0.0, "ease": "linear"}, {"t": 4.0, "v": 60.0, "ease": "linear"}])); keys(&mut ss, &p, "scale", json!([{"t": 0.0, "v": 1.0}, {"t": 4.0, "v": 1.5}]));
        c(&mut ss, "set_property", json!({"clip_id": ch, "property": "x", "value": 9.0})); c(&mut ss, "set_property", json!({"clip_id": ch, "property": "opacity", "value": 0.8})); c(&mut ss, "set_parent", json!({"clip_id": ch, "parent_id": p, "mode": "keep_local"}));
        let w = xf(&mut ss, &ch, tt, "world")["world"].clone();
        let (mut fl, t2) = scene(); let fc = rect(&mut fl, &t2); for k in ["x", "y", "scale", "rotation", "opacity"] { c(&mut fl, "set_property", json!({"clip_id": fc, "property": k, "value": w[k]})); }
        assert_ne!(hash_frame(&ss, tt), hash_frame(&scene().0, tt), "something is drawn");
        assert_eq!(hash_frame(&ss, tt), hash_frame(&fl, tt), "t={tt}: parented child must equal flattened child"); }
}
#[test] fn inherit_flags_and_time_rejection() {
    let (mut ss, t) = scene(); let p = sid(&c(&mut ss, "add_null", json!({"track_id": t, "start": 0.0, "duration": 30.0, "x": 10.0, "scale": 3.0, "rotation": 90.0, "opacity": 0.5}))); let ch = rect(&mut ss, &t);
    c(&mut ss, "set_parent", json!({"clip_id": ch, "parent_id": p, "mode": "keep_local", "inherit": {"scale": false, "rotation": false, "opacity": false}})); let w = xf(&mut ss, &ch, 1.0, "world");
    assert_eq!(f(&w["world"]["scale"]), 1.0); assert_eq!(f(&w["world"]["rotation"]), 0.0); assert_eq!(f(&w["world"]["opacity"]), 1.0); assert!((f(&w["world"]["x"]) - 10.0).abs() < 1e-9, "position still inherited (rotation/scale off in the child, parent's own R S applied to a zero offset)");
    let m = e(&mut ss, "set_parent", json!({"clip_id": ch, "parent_id": p, "inherit": {"time": true}})); assert!(m.contains("time"), "{m}");
}
#[test] fn get_transform_matrix_inverse_quaternion_and_slerp() {
    let (mut ss, t) = scene(); let ch = rect(&mut ss, &t); c(&mut ss, "set_property", json!({"clip_id": ch, "property": "rotation", "value": 33.0})); c(&mut ss, "set_property", json!({"clip_id": ch, "property": "x", "value": 7.5})); c(&mut ss, "set_property", json!({"clip_id": ch, "property": "scale", "value": 1.7}));
    for sp in ["local", "world", "canvas_px"] { let r = xf(&mut ss, &ch, 0.123, sp); let (m, i) = (&r["matrix"], &r["inverse"]);
        for a in 0..4 { for b in 0..4 { let s: f64 = (0..4).map(|k| f(&m[a][k]) * f(&i[k][b])).sum(); assert!((s - if a == b { 1.0 } else { 0.0 }).abs() < 1e-9, "{sp} {a}{b} {s}"); } }
        let q: f64 = r["quaternion_wxyz"].as_array().unwrap().iter().map(|x| f(x) * f(x)).sum(); assert!((q - 1.0).abs() < 1e-12); }
    use kimo_core::ext::xform_tools::{euler_to_quat, quat_slerp}; let (a, b) = (euler_to_quat(0.0, 0.0, 0.0), euler_to_quat(0.0, 0.0, 90.0)); let mid = quat_slerp(a, b, 0.5); let want = euler_to_quat(0.0, 0.0, 45.0); for i in 0..4 { assert!((mid[i] - want[i]).abs() < 1e-12); }
    let long = quat_slerp(a, euler_to_quat(0.0, 0.0, 350.0), 0.5); assert!((long.iter().map(|x| x * x).sum::<f64>() - 1.0).abs() < 1e-12);
    c(&mut ss, "set_property", json!({"clip_id": ch, "property": "scale", "value": 0.0})); assert_eq!(xf(&mut ss, &ch, 0.0, "world")["singular"], true);
}
#[test] fn add_null_is_invisible() {
    let (mut ss, t) = scene(); let before = hash_frame(&ss, 1.0); c(&mut ss, "add_null", json!({"track_id": t, "start": 0.0, "duration": 5.0, "x": 40.0, "scale": 5.0})); assert_eq!(hash_frame(&ss, 1.0), before);
}
/// Interleaved 1080p timing: 48 rect clips in chains of depth 8 vs a control where the same WORLD poses are set statically on unparented clips (identical pixel work, only the resolver differs).
/// Run: cargo test --profile gate --test transform_nodes -- --ignored --nocapture
#[test] #[ignore] fn timing_1080p_parented_vs_flat_interleaved() {
    let build = |parented: bool| { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 1920, "height": 1080, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let mut ids = vec![];
        for g in 0..6 { let mut prev: Option<String> = None; for d in 0..8 { let id = rect(&mut ss, &t); c(&mut ss, "set_property", json!({"clip_id": id, "property": "x", "value": (g * 40 + d * 6) as f64 - 200.0})); c(&mut ss, "set_property", json!({"clip_id": id, "property": "rotation", "value": 4.0}));
            if parented { if let Some(p) = &prev { c(&mut ss, "set_parent", json!({"clip_id": id, "parent_id": p, "mode": "keep_local"})); } } ids.push(id.clone()); prev = Some(id); } } (ss, ids) };
    let (mut b, ids) = build(true); let cache = render::Cache::new(); let (mut ta, mut tb) = (vec![], vec![]);
    let (mut a, aids) = build(false); for (bi, ai) in ids.iter().zip(&aids) { let w = c(&mut b, "get_transform", json!({"clip_id": bi, "t": 1.0, "space": "world"}))["world"].clone(); for k in ["x", "y", "scale", "rotation", "opacity"] { c(&mut a, "set_property", json!({"clip_id": ai, "property": k, "value": w[k]})); } }
    assert_eq!(hash_frame(&a, 1.0), hash_frame(&b, 1.0), "control must render identically");
    for _ in 0..11 { let s = std::time::Instant::now(); render::render_frame(&a.project, 1.0, &cache).unwrap(); ta.push(s.elapsed().as_secs_f64() * 1e3); let s = std::time::Instant::now(); render::render_frame(&b.project, 1.0, &cache).unwrap(); tb.push(s.elapsed().as_secs_f64() * 1e3); }
    ta.sort_by(|x, y| x.partial_cmp(y).unwrap()); tb.sort_by(|x, y| x.partial_cmp(y).unwrap()); eprintln!("1080p 48 clips depth-8 chains, median ms: control {:.1}  parented {:.1}  (min {:.1}/{:.1})", ta[5], tb[5], ta[0], tb[0]);
}
#[test] fn memo_never_serves_a_stale_parent_pose_across_frames_or_edits() {
    let (mut ss, t) = scene(); let p = sid(&c(&mut ss, "add_null", json!({"track_id": t, "start": 0.0, "duration": 30.0}))); let ch = rect(&mut ss, &t); c(&mut ss, "set_parent", json!({"clip_id": ch, "parent_id": p, "mode": "keep_local"}));
    let h1 = hash_frame(&ss, 1.0); c(&mut ss, "set_property", json!({"clip_id": p, "property": "x", "value": 30.0})); let h2 = hash_frame(&ss, 1.0); assert_ne!(h1, h2, "a parent edit must show in the next frame at the same t");
    c(&mut ss, "undo", json!({})); assert_eq!(hash_frame(&ss, 1.0), h1); assert_eq!(hash_frame(&ss, 1.0), h1, "repeat render is stable");
}
