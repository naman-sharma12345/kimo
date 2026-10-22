//! paste_attributes (Premiere Paste Attributes): a styled clip's motion, opacity and effects, keyframes included, land on other
//! clips so they render the same way; scale_times stretches the keys to each target's length; refusals leave the project as it was.
use kimo_core::commands::{dispatch, dispatch_ex, Session};
use kimo_core::render;
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn pj(s: &Session) -> Value { serde_json::to_value(&s.project).unwrap() }
/// Render only clip `id` (others muted by moving it to a fresh session) at time t: (centroid x, centroid y, mean alpha, edge energy).
fn look(s: &Session, id: &str, t: f64) -> (f64, f64, f64, f64) {
    let f = render::render_clip_alone(&s.project, id, t, &{ let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); cache }).unwrap();
    let (mut sx, mut sy, mut sa, mut e) = (0.0, 0.0, 0.0, 0.0);
    for y in 0..f.h { for x in 0..f.w { let a = f.d[(y * f.w + x) * 4 + 3] as f64; sx += a * x as f64; sy += a * y as f64; sa += a; if x > 0 { e += (f.d[(y * f.w + x) * 4 + 3] - f.d[(y * f.w + x - 1) * 4 + 3]).abs() as f64; } } }
    (sx / sa.max(1e-9), sy / sa.max(1e-9), sa / (f.w * f.h) as f64, e)
}
fn scene() -> (Session, String, String, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 160, "height": 90, "fps": 10}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone(); let t3 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let shape = |s: &mut Session, t: &Value, dur: f64| c(s, "add_shape", json!({"track_id": t, "shape": "rect", "width": 30, "height": 20, "fill": [1, 0.5, 0, 1], "start": 0.0, "duration": dur}))["clip_id"].as_str().unwrap().to_string();
    let (a, b, l) = (shape(&mut s, &t, 2.0), shape(&mut s, &t2, 2.0), shape(&mut s, &t3, 4.0));
    c(&mut s, "set_keyframe", json!({"clip_id": a, "property": "x", "time": 0.0, "value": -50.0})); c(&mut s, "set_keyframe", json!({"clip_id": a, "property": "x", "time": 2.0, "value": 50.0}));
    c(&mut s, "set_keyframe", json!({"clip_id": a, "property": "opacity", "time": 0.0, "value": 0.3})); c(&mut s, "set_keyframe", json!({"clip_id": a, "property": "opacity", "time": 2.0, "value": 1.0}));
    c(&mut s, "set_property", json!({"clip_id": a, "property": "y", "value": 20.0}));
    c(&mut s, "add_effect", json!({"clip_id": a, "kind": "blur", "params": {"radius": 6.0}}));
    (s, a, b, l)
}

#[test]
fn pasted_clips_render_like_the_source() {
    let (mut s, a, b, l) = scene();
    let r = c(&mut s, "paste_attributes", json!({"from_clip_id": a, "to_clip_ids": [b], "attributes": ["motion", "opacity", "effects"]})); assert_eq!(r["pasted_onto"], json!([b]), "{r}");
    for t in [0.3, 1.0, 1.7] { let (sa, sb) = (look(&s, &a, t), look(&s, &b, t));
        assert!((sa.0 - sb.0).abs() < 0.5 && (sa.1 - sb.1).abs() < 0.5 && (sa.2 - sb.2).abs() < 1e-3 && (sa.3 - sb.3).abs() < 0.02 * sa.3.max(1.0), "at {t} s the pasted clip must look like the source: source {sa:?}, pasted {sb:?}"); }
    // effect ids are fresh and the source is untouched
    let ids: Vec<String> = s.project.tracks.iter().flat_map(|t| t.clips.iter()).flat_map(|c| c.effects.iter().map(|e| e.id.clone())).collect();
    let mut u = ids.clone(); u.sort(); u.dedup(); assert_eq!(u.len(), ids.len(), "pasted effects need their own ids: {ids:?}");
    // scale_times: the 4 s clip plays the same move over its own length
    c(&mut s, "paste_attributes", json!({"from_clip_id": a, "to_clip_ids": [l], "attributes": ["motion", "opacity"], "scale_times": true}));
    for t in [0.5, 1.5] { let (sa, sl) = (look(&s, &a, t), look(&s, &l, 2.0 * t)); assert!((sa.0 - sl.0).abs() < 0.5 && (sa.2 - sl.2).abs() < 1e-3, "scale_times: the 4 s clip at {} s must match the source at {t} s: {sa:?} vs {sl:?}", 2.0 * t); }
}

#[test]
fn refusals_leave_the_project_unchanged() {
    let (mut s, a, b, _) = scene(); let before = pj(&s);
    for (args, want) in [(json!({"from_clip_id": a, "to_clip_ids": [a]}), "is the source clip"), (json!({"from_clip_id": a, "to_clip_ids": []}), "is empty"),
        (json!({"from_clip_id": a, "to_clip_ids": ["nope"]}), "no such clip nope"), (json!({"from_clip_id": a, "to_clip_ids": [b], "attributes": ["colour"]}), "unknown attribute 'colour'"),
        (json!({"from_clip_id": a, "to_clip_ids": [b], "effects_mode": "merge"}), "replace or append"), (json!({"from_clip_id": a, "to_clip_ids": [b], "times": 2}), "unknown argument 'times'")] {
        let e = dispatch_ex(&mut s, "paste_attributes", &args).unwrap_err().message; assert!(e.contains(want), "{args}: '{e}' lacks '{want}'"); assert_eq!(pj(&s), before, "{args}: a refused call changed the project"); }
}

#[test]
fn an_effect_tied_to_a_mask_layer_needs_the_mask_too() {
    let (mut s, a, b, _) = scene();
    let m = c(&mut s, "add_mask", json!({"clip_id": a, "kind": "rect", "params": {"cx": 0.5, "cy": 0.5, "w": 0.5, "h": 0.5}}));
    let mid = m["mask_id"].as_str().unwrap_or_else(|| panic!("add_mask answer has no mask_id: {m}")).to_string();
    let eid = s.project.find_clip(&a).unwrap().effects[0].id.clone();
    c(&mut s, "set_effect_mask", json!({"clip_id": a, "effect_id": eid, "mask_id": mid}));
    let before = pj(&s);
    let e = dispatch_ex(&mut s, "paste_attributes", &json!({"from_clip_id": a, "to_clip_ids": [b], "attributes": ["effects"]})).unwrap_err().message;
    assert!(e.contains("mask layer") && e.contains("add \"mask\""), "pasting a masked effect alone must be refused with the fix: {e}");
    assert_eq!(pj(&s), before, "the refusal must leave the project unchanged");
    c(&mut s, "paste_attributes", json!({"from_clip_id": a, "to_clip_ids": [b], "attributes": ["effects", "mask"]}));
    let cb = s.project.find_clip(&b).unwrap(); assert_eq!(cb.effects[0].strings.get("mask_layer"), Some(&mid)); assert!(cb.mask.as_ref().is_some_and(|m| m.layers.iter().any(|l| l.id == mid)));
}
