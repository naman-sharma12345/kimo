//! Tool contract suite (unit 9). Every row runs one tool through `dispatch` on a fresh Session and asserts: the call succeeds, the row's own check on the
//! project JSON before/after, byte-exact undo, byte-exact redo, and that the registry `mutates` flag equals (before != after). Error rows assert the call
//! fails, the project JSON is unchanged and the undo stack length is unchanged. No media, no network.
use kimo_core::commands::{dispatch, registry_ref, Session};
use kimo_core::render;
use serde_json::{json, Value};

type Step<'a> = (&'a str, Value);
fn pj(ss: &Session) -> Value { serde_json::to_value(&ss.project).unwrap() }
fn call(ss: &mut Session, n: &str, a: Value) -> Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("setup {n}: {e}")) }
fn fresh(setup: &[Step]) -> Session { let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 320, "height": 180, "fps": 25})); for (n, a) in setup { call(&mut ss, n, a.clone()); } ss }
fn clips(p: &Value) -> Vec<Value> { p["tracks"].as_array().unwrap().iter().flat_map(|t| t["clips"].as_array().unwrap().iter().cloned()).collect() }
fn clip(p: &Value, id: &str) -> Value { clips(p).into_iter().find(|c| c["id"] == id).unwrap_or_else(|| panic!("no clip {id}")) }
fn without(v: &Value, keys: &[&str]) -> Value { let mut v = v.clone(); for k in keys { v.as_object_mut().unwrap().remove(*k); } v }

fn row(setup: &[Step], tool: &str, args: Value, check: impl Fn(&Value, &Value, &Value)) -> Session {
    let mut ss = fresh(setup); let before = pj(&ss);
    let res = dispatch(&mut ss, tool, &args).unwrap_or_else(|e| panic!("{tool} failed: {e}")); let after = pj(&ss);
    check(&res, &before, &after);
    assert!(ss.project.dangling_refs().is_empty(), "{tool}: dangling clip references after the call: {:?}", ss.project.dangling_refs());
    let mutates = registry_ref().iter().find(|c| c.name == tool).unwrap_or_else(|| panic!("{tool} not in registry")).mutates;
    assert_eq!(mutates, before != after, "{tool}: registry mutates={mutates} but project {}changed", if before != after { "" } else { "un" });
    if before != after {
        call(&mut ss, "undo", json!({})); assert_eq!(pj(&ss), before, "{tool}: undo is not byte-exact"); assert!(ss.project.dangling_refs().is_empty(), "{tool}: dangling refs after undo");
        call(&mut ss, "redo", json!({})); assert_eq!(pj(&ss), after, "{tool}: redo does not re-apply byte-exactly"); assert!(ss.project.dangling_refs().is_empty(), "{tool}: dangling refs after redo");
    }
    ss
}
fn row_err(setup: &[Step], tool: &str, args: Value, want: Option<&str>) {
    let mut ss = fresh(setup); let before = pj(&ss); let depth = ss.undo.len();
    match dispatch(&mut ss, tool, &args) { Ok(r) => panic!("{tool} {args} should have failed but returned {r}"), Err(e) => if let Some(w) = want { assert!(e.contains(w), "{tool}: error '{e}' lacks '{w}'") } }
    assert_eq!(pj(&ss), before, "{tool}: a failed call changed the project"); assert_eq!(ss.undo.len(), depth, "{tool}: a failed call left an undo step");
}
fn renders_ok(ss: &Session) { let cache = render::Cache::new(); for t in [0.1, 0.5, 1.2] { render::render_frame(&ss.project, t, &cache).unwrap_or_else(|e| panic!("render at {t} failed: {e}")); } }

fn base() -> Vec<Step<'static>> { vec![("add_track", json!({})), ("add_track", json!({}))] }
/// Build the setup list for a project with one shape clip; returns (setup, track1, track2, clip).
fn one_clip() -> (Vec<Step<'static>>, String, String, String) {
    let mut ss = fresh(&base()); let p = pj(&ss); let t1 = p["tracks"][0]["id"].as_str().unwrap().to_string(); let t2 = p["tracks"][1]["id"].as_str().unwrap().to_string();
    let c = call(&mut ss, "add_shape", json!({"track_id": t1, "shape": "rect", "width": 60, "height": 40, "fill": [1, 0.5, 0, 1], "start": 1.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    let mut s = base(); s.push(("add_shape", json!({"track_id": t1, "shape": "rect", "width": 60, "height": 40, "fill": [1, 0.5, 0, 1], "start": 1.0, "duration": 2.0}))); (s, t1, t2, c)
}

#[test] fn duplicate_clip_copies_everything_but_id_and_start() {
    let (mut s, t1, _t2, c) = one_clip(); s.push(("add_effect", json!({"clip_id": c, "kind": "blur", "params": {"radius": 4}}))); s.push(("set_keyframe", json!({"clip_id": c, "property": "scale", "time": 0.5, "value": 1.5})));
    row(&s, "duplicate_clip", json!({"clip_id": c, "start": 5.0}), |res, b, a| {
        assert_eq!(clips(a).len(), clips(b).len() + 1, "exactly one clip added"); let nid = res["clip_id"].as_str().unwrap(); assert_ne!(nid, c);
        let (src, dup) = (clip(a, &c), clip(a, nid)); assert_eq!(dup["start"], 5.0); assert_eq!(dup["track_id"].as_str().unwrap_or(&t1), t1);
        assert_eq!(without(&without(&src, &["id", "start"]), &[]), without(&dup, &["id", "start"]), "everything except id and start must match the source");
        // documented current behaviour: effect ids are copied verbatim (they are only unique within a clip)
        assert_eq!(src["effects"][0]["id"], dup["effects"][0]["id"]);
    });
}

#[test] fn duplicate_clip_onto_another_track() {
    let (s, _t1, t2, c) = one_clip();
    row(&s, "duplicate_clip", json!({"clip_id": c, "start": 0.0, "track_id": t2}), |_r, b, a| {
        assert_eq!(a["tracks"][1]["clips"].as_array().unwrap().len(), 1, "lands on the requested track"); assert_eq!(a["tracks"][0]["clips"], b["tracks"][0]["clips"], "source track untouched");
    });
}
#[test] fn duplicate_clip_errors() {
    let (s, _t1, _t2, c) = one_clip();
    row_err(&s, "duplicate_clip", json!({"clip_id": "nope", "start": 1.0}), None);
    row_err(&s, "duplicate_clip", json!({"clip_id": c, "start": 1.0, "track_id": "nope"}), None);
}
#[test] fn insert_gap_shifts_later_clips_and_splits_the_spanning_one() {
    let (mut s, t1, _t2, _c) = one_clip(); // clip 1.0..3.0
    s.push(("add_shape", json!({"track_id": t1, "shape": "rect", "width": 10, "height": 10, "fill": [1, 1, 1, 1], "start": 4.0, "duration": 1.0})));
    row(&s, "insert_gap", json!({"track_id": t1, "at": 2.0, "duration": 0.5}), |_r, b, a| {
        let (cb, ca) = (clips(b), clips(a)); assert_eq!(ca.len(), cb.len() + 1, "the spanning clip splits in two");
        let mut ds: Vec<f64> = ca.iter().map(|c| c["start"].as_f64().unwrap()).collect(); ds.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let want = [1.0, 2.5, 4.5]; for (g, w) in ds.iter().zip(want.iter()) { assert!((g - w).abs() < 1e-9, "starts {ds:?}"); }
        let spanning: f64 = ca.iter().filter(|c| c["start"].as_f64().unwrap() < 3.0).map(|c| c["duration"].as_f64().unwrap()).sum(); assert!((spanning - 2.0).abs() < 1e-9, "durations of the two halves sum to the original");
    });
}
#[test] fn insert_gap_before_everything_moves_all_clips_by_exactly_duration() {
    let (s, t1, _t2, _c) = one_clip();
    row(&s, "insert_gap", json!({"track_id": t1, "at": 0.0, "duration": 1.25}), |_r, b, a| { let (x, y) = (clips(b)[0]["start"].as_f64().unwrap(), clips(a)[0]["start"].as_f64().unwrap()); assert!((y - x - 1.25).abs() < 1e-9); assert_eq!(clips(a).len(), clips(b).len()); });
}
#[test] fn insert_gap_rejects_bad_input() {
    let (s, t1, _t2, _c) = one_clip();
    row_err(&s, "insert_gap", json!({"track_id": t1, "at": 2.0, "duration": -1.0}), None);
    row_err(&s, "insert_gap", json!({"track_id": t1, "at": 2.0, "duration": 1e18}), None);
    row_err(&s, "insert_gap", json!({"track_id": "nope", "at": 2.0, "duration": 1.0}), None);
}
#[test] fn slip_clip_moves_only_the_in_point() {
    let (mut s, _t1, _t2, c) = one_clip(); s.push(("trim_clip", json!({"clip_id": c, "in_point": 1.0})));
    row(&s, "slip_clip", json!({"clip_id": c, "delta": 0.25}), |_r, b, a| {
        let (x, y) = (clip(b, &c), clip(a, &c)); assert!((y["in_point"].as_f64().unwrap() - x["in_point"].as_f64().unwrap() - 0.25).abs() < 1e-9);
        assert_eq!(without(&x, &["in_point"]), without(&y, &["in_point"]), "nothing but the in point changes");
    });
}
#[test] fn slip_clip_cannot_go_before_the_source_start() {
    let (s, _t1, _t2, c) = one_clip(); let mut ss = fresh(&s); let id = pj(&ss)["tracks"][0]["clips"][0]["id"].as_str().unwrap().to_string(); let _ = c;
    let r = dispatch(&mut ss, "slip_clip", &json!({"clip_id": id, "delta": -50.0}));
    let after = clip(&pj(&ss), &id)["in_point"].as_f64().unwrap(); assert!(after >= 0.0, "in point {after} went negative (result {r:?})");
}
#[test] fn rate_stretch_sets_duration_and_speed_keeps_source_range() {
    let (s, _t1, _t2, c) = one_clip();
    row(&s, "rate_stretch", json!({"clip_id": c, "duration": 4.0}), |_r, b, a| {
        let (x, y) = (clip(b, &c), clip(a, &c)); assert!((y["duration"].as_f64().unwrap() - 4.0).abs() < 1e-9);
        let (rb, ra) = (x["speed"].as_f64().unwrap() * x["duration"].as_f64().unwrap(), y["speed"].as_f64().unwrap() * y["duration"].as_f64().unwrap()); assert!((rb - ra).abs() < 1e-9, "source range changed {rb} -> {ra}");
    });
    row_err(&s, "rate_stretch", json!({"clip_id": c, "duration": 0.0}), None); row_err(&s, "rate_stretch", json!({"clip_id": c, "duration": -1.0}), None); row_err(&s, "rate_stretch", json!({"clip_id": "nope", "duration": 1.0}), None);
}
#[test] fn set_text_changes_text_only_and_rejects_non_text() {
    let mut s = base(); let ss0 = fresh(&base()); let t1 = pj(&ss0)["tracks"][0]["id"].as_str().unwrap().to_string();
    s.push(("add_text", json!({"track_id": t1, "text": "HELLO", "start": 0.0, "duration": 2.0})));
    let (os, _, _, shape) = one_clip(); let _ = os;
    let probe = { let ss = fresh(&s); pj(&ss)["tracks"][0]["clips"][0]["id"].as_str().unwrap().to_string() };
    row(&s, "set_text", json!({"clip_id": probe, "text": "WORLD"}), |_r, b, a| {
        let (x, y) = (clip(b, &probe), clip(a, &probe)); assert_ne!(x["source"], y["source"]); assert_eq!(without(&x, &["source"]), without(&y, &["source"]));
        assert!(y["source"].to_string().contains("WORLD"));
    });
    let (s2, _, _, c2) = one_clip(); let _ = shape; row_err(&s2, "set_text", json!({"clip_id": c2, "text": "x"}), None);
}
#[test] fn remove_mask_leaves_no_dangling_state() {
    let (mut s, _t1, _t2, c) = one_clip(); s.push(("add_mask", json!({"clip_id": c, "kind": "ellipse"})));
    let ss = row(&s, "remove_mask", json!({"clip_id": c}), |_r, b, a| { assert!(!clip(b, &c)["mask"].is_null()); assert!(clip(a, &c)["mask"].is_null()); });
    renders_ok(&ss);
}
#[test] fn read_only_list_tools_do_not_touch_the_project() {
    let (s, _t1, _t2, c) = one_clip();
    for (t, a) in [("list_markers", json!({})), ("list_materials", json!({})), ("list_fonts", json!({})), ("list_masks", json!({"clip_id": c}))] {
        row(&s, t, a, |_r, b, a| assert_eq!(b, a)); }
}

fn keyed(prop: &str, keys: &[(f64, f64)]) -> (Vec<Step<'static>>, String) {
    let (mut s, _t1, _t2, c) = one_clip(); for (t, v) in keys { s.push(("set_keyframe", json!({"clip_id": c, "property": prop, "time": t, "value": v}))); } (s, c)
}
fn keys_of(p: &Value, c: &str, prop: &str) -> Vec<(f64, f64)> { clip(p, c)["props"][prop]["keys"].as_array().map(|a| a.iter().map(|k| (k["t"].as_f64().unwrap(), k["v"].as_f64().unwrap())).collect()).unwrap_or_default() }
fn sample(keys: &[(f64, f64)], t: f64) -> f64 { if keys.is_empty() { return 0.0; } if t <= keys[0].0 { return keys[0].1; } for w in keys.windows(2) { if t <= w[1].0 { let u = (t - w[0].0) / (w[1].0 - w[0].0).max(1e-12); return w[0].1 + (w[1].1 - w[0].1) * u; } } keys.last().unwrap().1 }

#[test] fn ungroup_restores_the_original_clips_ignoring_ids() {
    let (mut s, t1, _t2, a) = one_clip(); s.push(("add_shape", json!({"track_id": t1, "shape": "ellipse", "width": 20, "height": 20, "fill": [0, 1, 0, 1], "start": 0.5, "duration": 1.0})));
    let b = { let ss = fresh(&s); pj(&ss)["tracks"][0]["clips"][1]["id"].as_str().unwrap().to_string() };
    let mut s2 = s.clone(); s2.push(("group_clips", json!({"clip_ids": [a.clone(), b.clone()]})));
    let g = { let ss = fresh(&s2); clips(&pj(&ss)).into_iter().find(|c| c["source"]["type"] == "precomp").expect("group clip")["id"].as_str().unwrap().to_string() };
    row(&s2, "ungroup_clip", json!({"clip_id": g}), |_r, _b, after| {
        let starts: Vec<f64> = { let mut v: Vec<f64> = clips(after).iter().map(|c| c["start"].as_f64().unwrap()).collect(); v.sort_by(|x, y| x.partial_cmp(y).unwrap()); v };
        assert_eq!(starts.len(), 2, "two clips come back: {starts:?}"); assert!((starts[0] - 0.5).abs() < 1e-9 && (starts[1] - 1.0).abs() < 1e-9, "original starts {starts:?}");
    });
}
// Recorded behaviour: freeze_frame is accepted on a shape clip (it is not media-only), and an unknown clip errors.
#[test] fn freeze_frame_on_a_shape_clip_is_undoable_and_unknown_clip_errors() { let (s, _t1, _t2, c) = one_clip(); row(&s, "freeze_frame", json!({"clip_id": c, "at": 1.5}), |_r, b, a| assert_ne!(b, a)); row_err(&s, "freeze_frame", json!({"clip_id": "nope", "at": 1.5}), None); row_err(&s, "freeze_frame", json!({"clip_id": c, "at": -3.0}), Some("outside")); row_err(&s, "freeze_frame", json!({"clip_id": c, "at": 99.0}), Some("outside")); }
// Recorded behaviour: freeze_frame holds the WHOLE clip (before and after `at`), as the corrected description says.
#[test] fn freeze_frame_holds_the_whole_clip_on_one_source_time() {
    let (s, _t1, _t2, c) = one_clip(); let ss = row(&s, "freeze_frame", json!({"clip_id": c, "at": 2.0}), |_r, b, a| assert_ne!(b, a));
    let cl: kimo_core::model::Clip = serde_json::from_value(clip(&pj(&ss), &c)).unwrap(); let (a, b, d) = (cl.display_time(0.1), cl.display_time(1.0), cl.display_time(1.9));
    assert!((a - b).abs() < 1e-9 && (b - d).abs() < 1e-9, "display time must be constant over the whole clip: {a} {b} {d}"); assert!((a - 1.0).abs() < 1e-9, "held at clip-local 1.0 (timeline 2.0 - start 1.0), got {a}");
}

#[test] fn marker_add_list_remove_round_trip() {
    let mut ss = fresh(&base()); let r = call(&mut ss, "add_marker", json!({"time": 1.5, "name": "m1"})); let id = r["marker_id"].as_str().or(r["id"].as_str()).unwrap_or_else(|| panic!("no marker id in {r}")).to_string();
    let listed = call(&mut ss, "list_markers", json!({})).to_string(); assert!(listed.contains("m1"));
    let before = pj(&ss); call(&mut ss, "remove_marker", json!({"marker_id": id})); assert!(!call(&mut ss, "list_markers", json!({})).to_string().contains("m1"), "still listed after removal");
    call(&mut ss, "undo", json!({})); assert_eq!(pj(&ss), before, "undo restores byte-exactly"); renders_ok(&ss);
    row_err(&[], "remove_marker", json!({"marker_id": "nope"}), None);
}
#[test] fn remove_light_with_a_relit_clip_does_not_break_rendering() {
    let (mut s, _t1, _t2, c) = one_clip(); s.push(("set_light", json!({"id": "key", "kind": "point", "pos": [100, 50, 80], "intensity": 1.0}))); s.push(("set_relight", json!({"clip_id": c})));
    let ss = row(&s, "remove_light", json!({"id": "key"}), |_r, b, a| { assert!(b["lights"].to_string().contains("key")); assert!(!a["lights"].to_string().contains("\"key\"")); });
    renders_ok(&ss); assert!(!call(&mut { ss }, "list_lights", json!({})).to_string().contains("\"key\""));
    row_err(&s, "remove_light", json!({"id": "nope"}), None);
}
#[test] fn remove_material_with_a_relit_clip_does_not_break_rendering() {
    let (mut s, _t1, _t2, c) = one_clip(); s.push(("set_material", json!({"name": "gold", "metal": 1.0, "albedo": [1, 0.8, 0.3]}))); s.push(("set_relight", json!({"clip_id": c, "strings": {"material": "gold"}})));
    let ss = row(&s, "remove_material", json!({"name": "gold"}), |_r, b, a| { assert!(b["materials"].to_string().contains("gold")); assert!(!a["materials"].to_string().contains("gold")); });
    renders_ok(&ss);
}
#[test] fn remove_motion_path_and_track_matte_leave_renderable_projects() {
    let (mut s, _t1, _t2, c) = one_clip(); s.push(("set_motion_path", json!({"clip_id": c, "shape": "circle", "cx": 160, "cy": 90, "rx": 40, "ry": 40})));
    renders_ok(&row(&s, "remove_motion_path", json!({"clip_id": c}), |_r, b, a| assert_ne!(b, a)));
    let (mut s2, t1, _t2, c2) = one_clip(); s2.push(("add_shape", json!({"track_id": t1, "shape": "ellipse", "width": 50, "height": 50, "fill": [1, 1, 1, 1], "start": 1.0, "duration": 2.0})));
    let m = { let ss = fresh(&s2); pj(&ss)["tracks"][0]["clips"][1]["id"].as_str().unwrap().to_string() }; s2.push(("set_track_matte", json!({"clip_id": c2, "matte_clip_id": m})));
    renders_ok(&row(&s2, "remove_track_matte", json!({"clip_id": c2}), |_r, b, a| assert_ne!(b, a)));
}
#[test] fn curve_tools_keep_endpoints_and_exactness() {
    let ks: Vec<(f64, f64)> = (0..11).map(|i| (i as f64 * 0.2, (i as f64 * 0.7).sin())).collect();
    let (s, c) = keyed("x", &ks);
    row(&s, "remove_key", json!({"clip_id": c, "property": "x", "time": 0.4}), |_r, b, a| { assert_eq!(keys_of(a, &c, "x").len(), keys_of(b, &c, "x").len() - 1); assert!(!keys_of(a, &c, "x").iter().any(|k| (k.0 - 0.4).abs() < 1e-9)); });
    row(&s, "move_key", json!({"clip_id": c, "property": "x", "time": 0.4, "new_time": 0.45}), |_r, b, a| {
        let (kb, ka) = (keys_of(b, &c, "x"), keys_of(a, &c, "x")); assert_eq!(kb.len(), ka.len()); let diff: Vec<_> = kb.iter().zip(ka.iter()).filter(|(x, y)| x != y).collect(); assert_eq!(diff.len(), 1, "only one key changes: {diff:?}"); assert!((diff[0].1 .0 - 0.45).abs() < 1e-9);
    });
    let tol = 0.05;
    row(&s, "simplify_curve", json!({"clip_id": c, "property": "x", "tolerance": tol}), |_r, b, a| {
        let (kb, ka) = (keys_of(b, &c, "x"), keys_of(a, &c, "x")); assert!(ka.len() < kb.len(), "simplify removed nothing"); assert_eq!(ka.first(), kb.first()); assert_eq!(ka.last(), kb.last());
        let worst = (0..=200).map(|i| { let t = 2.0 * i as f64 / 200.0; (sample(&kb, t) - sample(&ka, t)).abs() }).fold(0.0, f64::max); assert!(worst <= tol * 1.0001 + 1e-9, "max deviation {worst} > tolerance {tol}");
    });
    // recorded behaviour: smooth_curve keeps the key count and key times; endpoint VALUES move (see the ignored test below)
    row(&s, "smooth_curve", json!({"clip_id": c, "property": "x", "sigma": 1.0}), |_r, b, a| { let (kb, ka) = (keys_of(b, &c, "x"), keys_of(a, &c, "x")); assert_eq!(kb.len(), ka.len()); for (x, y) in kb.iter().zip(ka.iter()) { assert_eq!(x.0, y.0, "key times must not move"); } });
    row_err(&s, "remove_key", json!({"clip_id": c, "property": "x", "time": 9.0}), None);
}

fn tv(k: &[(f64, f64)]) -> f64 { k.windows(2).map(|w| (w[1].1 - w[0].1).abs()).sum() }
#[test] fn smooth_curve_rows() {
    let noisy: Vec<(f64, f64)> = (0..21).map(|i| (i as f64 * 0.1, if i % 2 == 0 { 1.0 } else { -1.0 } + i as f64 * 0.05)).collect(); let (s, c) = keyed("x", &noisy);
    row(&s, "smooth_curve", json!({"clip_id": c, "property": "x", "sigma": 2.0}), |_r, b, a| { let (kb, ka) = (keys_of(b, &c, "x"), keys_of(a, &c, "x")); assert_eq!(kb.len(), ka.len()); for (x, y) in kb.iter().zip(ka.iter()) { assert_eq!(x.0, y.0); } assert!(tv(&ka) < tv(&kb) * 0.5, "total variation {} -> {}", tv(&kb), tv(&ka)); });
    row(&s, "smooth_curve", json!({"clip_id": c, "property": "x", "sigma": 2.0, "pin_ends": true}), |_r, b, a| { let (kb, ka) = (keys_of(b, &c, "x"), keys_of(a, &c, "x")); assert_eq!(kb[0].1, ka[0].1); assert_eq!(kb.last().unwrap().1, ka.last().unwrap().1); assert!(tv(&ka) < tv(&kb)); });
    row(&s, "smooth_curve", json!({"clip_id": c, "property": "x", "sigma": 2.0}), |_r, b, a| { assert_ne!(keys_of(b, &c, "x")[0].1, keys_of(a, &c, "x")[0].1, "without pin_ends the first value moves (edge-replicating Gaussian)"); });
    let flat: Vec<(f64, f64)> = (0..8).map(|i| (i as f64 * 0.2, 0.7)).collect(); let (s2, c2) = keyed("x", &flat);
    let mut ss = fresh(&s2); let before = keys_of(&pj(&ss), &c2, "x"); call(&mut ss, "smooth_curve", json!({"clip_id": c2, "property": "x", "sigma": 3.0})); for (x, y) in before.iter().zip(keys_of(&pj(&ss), &c2, "x").iter()) { assert!((x.1 - y.1).abs() < 1e-12, "a constant curve must not change"); }
    let ramp: Vec<(f64, f64)> = (0..11).map(|i| (i as f64 * 0.2, i as f64)).collect(); let (s3, c3) = keyed("x", &ramp);
    let mut ss = fresh(&s3); call(&mut ss, "smooth_curve", json!({"clip_id": c3, "property": "x", "sigma": 300.0})); let k = keys_of(&pj(&ss), &c3, "x"); let (lo, hi) = k.iter().fold((f64::MAX, f64::MIN), |(l, h), x| (l.min(x.1), h.max(x.1)));
    assert!(hi - lo < 0.02 * 10.0, "huge sigma should flatten a 0..10 ramp to under 2% of its range, spread {}", hi - lo); eprintln!("huge sigma on 0..10 ramp -> {:.3} (end-average 5, edge replication, not the mean of keys)", k[0].1);
    row_err(&s3, "smooth_curve", json!({"clip_id": c3, "property": "nope"}), None);
}
#[test] fn set_keyframe_with_effect_id_on_a_duplicate_edits_only_the_duplicate() {
    let (mut s, _t1, _t2, c) = one_clip(); s.push(("add_effect", json!({"clip_id": c, "kind": "blur", "params": {"radius": 4}})));
    let mut ss = fresh(&s); let eid = clip(&pj(&ss), &c)["effects"][0]["id"].as_str().unwrap().to_string();
    let nid = call(&mut ss, "duplicate_clip", json!({"clip_id": c, "start": 5.0}))["clip_id"].as_str().unwrap().to_string(); let before = pj(&ss);
    call(&mut ss, "set_keyframe", json!({"clip_id": nid, "effect_id": eid, "property": "radius", "time": 0.5, "value": 9.0})); let after = pj(&ss);
    assert_eq!(clip(&before, &c), clip(&after, &c), "the source clip must not change"); assert_ne!(clip(&before, &nid), clip(&after, &nid));
}

fn matte_setup() -> (Vec<Step<'static>>, String, String) {
    let (mut s, t1, _t2, c) = one_clip(); s.push(("add_shape", json!({"track_id": t1, "shape": "ellipse", "width": 50, "height": 50, "fill": [1, 1, 1, 1], "start": 1.0, "duration": 2.0})));
    let m = { let ss = fresh(&s); pj(&ss)["tracks"][0]["clips"][1]["id"].as_str().unwrap().to_string() }; s.push(("set_track_matte", json!({"clip_id": c, "matte_clip_id": m}))); (s, c, m)
}
#[test] fn duplicate_of_a_matte_source_and_of_a_matted_clip_still_render() {
    let (s, c, m) = matte_setup();
    renders_ok(&row(&s, "duplicate_clip", json!({"clip_id": m, "start": 6.0}), |res, b, a| { assert_eq!(clips(a).len(), clips(b).len() + 1); assert_eq!(clip(a, &c), clip(b, &c), "the matted clip keeps pointing at its own matte"); let _ = res; }));
    renders_ok(&row(&s, "duplicate_clip", json!({"clip_id": c, "start": 6.0}), |res, b, a| {
        let dup = clip(a, res["clip_id"].as_str().unwrap()); let src = clip(b, &c);
        assert!(!src["mask"]["matte"].is_null(), "setup: the matte reference lives under mask.matte"); eprintln!("duplicate of matted clip: mask.matte = {}", dup["mask"]["matte"]);
        assert_eq!(dup["mask"]["matte"], src["mask"]["matte"], "the copy keeps the same matte reference");
    }));
}
#[test] fn duplicate_of_a_clip_with_a_mask_layer_that_references_another_clip_renders() {
    let (mut s, _t1, _t2, c) = one_clip(); let (_s2, t1b) = (0, { let ss = fresh(&s); pj(&ss)["tracks"][0]["id"].as_str().unwrap().to_string() });
    s.push(("add_shape", json!({"track_id": t1b, "shape": "ellipse", "width": 30, "height": 30, "fill": [1, 1, 1, 1], "start": 1.0, "duration": 2.0})));
    let r = { let ss = fresh(&s); pj(&ss)["tracks"][0]["clips"][1]["id"].as_str().unwrap().to_string() }; s.push(("add_mask", json!({"clip_id": c, "kind": "ellipse", "ref_clip": r})));
    renders_ok(&row(&s, "duplicate_clip", json!({"clip_id": c, "start": 6.0}), |_r, b, a| assert_eq!(clips(a).len(), clips(b).len() + 1)));
}

#[test] fn freeze_frame_hold_from_here_plays_then_holds() {
    let (s, _t1, _t2, c) = one_clip(); // clip 1.0..3.0
    let ss = row(&s, "freeze_frame", json!({"clip_id": c, "at": 2.0, "hold_from_here": true}), |r, b, a| {
        assert_eq!(clips(a).len(), clips(b).len() + 1, "split adds exactly one clip"); let rid = r["right_clip_id"].as_str().unwrap().to_string();
        assert!(clip(a, &rid)["start"].as_f64().unwrap() - 2.0 < 1e-9);
    });
    let p = pj(&ss); let mut cs = clips(&p); cs.sort_by(|x, y| x["start"].as_f64().unwrap().partial_cmp(&y["start"].as_f64().unwrap()).unwrap());
    let first: kimo_core::model::Clip = serde_json::from_value(cs[0].clone()).unwrap(); let second: kimo_core::model::Clip = serde_json::from_value(cs[1].clone()).unwrap();
    assert!(first.display_time(0.5) > first.display_time(0.1) + 0.3, "first half plays normally"); assert!((second.display_time(0.1) - second.display_time(0.9)).abs() < 1e-9, "second half is constant");
    row_err(&s, "freeze_frame", json!({"clip_id": c, "at": 1.0, "hold_from_here": true}), Some("strictly inside"));
}

fn frame_bytes(ss: &Session, t: f64) -> Vec<f32> { render::render_frame(&ss.project, t, &render::Cache::new()).unwrap().d.clone() }
#[test] fn render_after_remove_light_equals_a_project_that_never_had_the_light() {
    let (mut with, _t1, _t2, c) = one_clip(); with.push(("set_relight", json!({"clip_id": c})));
    let never = fresh(&with); let mut had = { let mut w = with.clone(); w.push(("set_light", json!({"id": "key", "kind": "point", "pos": [100, 50, 80], "intensity": 2.0}))); fresh(&w) };
    call(&mut had, "remove_light", json!({"id": "key"}));
    for t in [0.1, 1.5, 2.5] { assert_eq!(frame_bytes(&had, t), frame_bytes(&never, t), "stale light state visible at t={t}"); }
}
#[test] fn deleting_a_matte_source_clears_the_reference_reports_it_and_renders_as_unmatted() {
    let (s, c, m) = matte_setup(); let mut ss = fresh(&s); let orig = pj(&ss); let orig_frames: Vec<_> = [0.1, 1.5, 2.5].iter().map(|t| frame_bytes(&ss, *t)).collect();
    let res = call(&mut ss, "delete_clip", json!({"clip_id": m}));
    assert_eq!(res["cleared_refs"], json!([{"clip_id": c, "field": "mask.matte.source", "removed_id": m}]));
    assert!(clip(&pj(&ss), &c)["mask"]["matte"].is_null()); assert!(ss.project.dangling_refs().is_empty());
    // same project built without ever having a matte (the matte clip is gone in both)
    let (mut s_nm, t1, _t2, _c2) = one_clip(); let _ = t1; s_nm.push(("noop_marker", json!({}))); s_nm.pop();
    let mut plain = fresh(&s); call(&mut plain, "remove_track_matte", json!({"clip_id": c})); call(&mut plain, "delete_clip", json!({"clip_id": m}));
    for t in [0.1, 1.5, 2.5] { assert_eq!(frame_bytes(&ss, t), frame_bytes(&plain, t), "cleared matte must render exactly like a clip with no matte (t={t})"); }
    call(&mut ss, "undo", json!({})); assert_eq!(pj(&ss), orig, "undo restores the matte and the source clip"); assert!(ss.project.dangling_refs().is_empty());
    for (i, t) in [0.1, 1.5, 2.5].iter().enumerate() { assert_eq!(frame_bytes(&ss, *t), orig_frames[i], "render after undo equals the original"); }
}
#[test] fn deleting_a_mask_reference_clip_clears_ref_clip_and_ripple_delete_does_too() {
    let (mut s2, _t1, _t2, c2) = one_clip(); let t1b = { let ss = fresh(&s2); pj(&ss)["tracks"][0]["id"].as_str().unwrap().to_string() };
    s2.push(("add_shape", json!({"track_id": t1b, "shape": "ellipse", "width": 30, "height": 30, "fill": [1, 1, 1, 1], "start": 1.0, "duration": 2.0})));
    let r = { let ss = fresh(&s2); pj(&ss)["tracks"][0]["clips"][1]["id"].as_str().unwrap().to_string() }; s2.push(("add_mask", json!({"clip_id": c2, "kind": "ellipse", "ref_clip": r})));
    for tool in ["delete_clip", "ripple_delete"] {
        let mut ss = fresh(&s2); let orig = pj(&ss); let res = call(&mut ss, tool, json!({"clip_id": r}));
        let cr = res["cleared_refs"].as_array().unwrap_or_else(|| panic!("{tool}: no cleared_refs in {res}")); assert_eq!(cr.len(), 1); assert_eq!(cr[0]["removed_id"], r.as_str()); assert!(cr[0]["field"].as_str().unwrap().ends_with(".ref_clip"));
        assert!(ss.project.dangling_refs().is_empty()); renders_ok(&ss);
        call(&mut ss, "undo", json!({})); assert_eq!(pj(&ss), orig); assert!(ss.project.dangling_refs().is_empty());
    }
}
#[test] fn deleting_the_clip_an_emitter_spawns_on_or_collides_with_clears_those_fields() {
    let (mut s, t1, _t2, _c) = one_clip();
    s.push(("add_shape", json!({"track_id": t1, "shape": "ellipse", "width": 30, "height": 30, "fill": [1, 1, 1, 1], "start": 1.0, "duration": 2.0})));
    let r = { let ss = fresh(&s); pj(&ss)["tracks"][0]["clips"][1]["id"].as_str().unwrap().to_string() };
    s.push(("add_emitter", json!({"track_id": t1, "start": 1.0, "duration": 2.0, "config": {"shape": "layer", "emit_clip": r, "colliders": [{"kind": "layer", "clip_id": r}]}})));
    let mut ss = fresh(&s); let orig = pj(&ss); assert!(ss.project.dangling_refs().is_empty());
    let res = call(&mut ss, "delete_clip", json!({"clip_id": r})); let cr = res["cleared_refs"].as_array().unwrap(); assert_eq!(cr.len(), 2, "{res}");
    let fields: Vec<&str> = cr.iter().map(|x| x["field"].as_str().unwrap()).collect(); assert!(fields.contains(&"source.emit_clip") && fields.contains(&"source.colliders.0.clip_id"), "{fields:?}");
    assert!(ss.project.dangling_refs().is_empty()); renders_ok(&ss);
    call(&mut ss, "undo", json!({})); assert_eq!(pj(&ss), orig);
}
#[test] fn refs_inside_a_precomp_are_found_and_cleared() {
    let (s, c, m) = matte_setup(); let mut ss = fresh(&s);
    // move the matted clip into a precomp that sits on the same track; its matte source stays at the top level
    let mut g = ss.project.find_clip(&c).unwrap().clone(); g.id = "clip9000".into(); let inner = ss.project.tracks[0].clone();
    let mut it = inner.clone(); it.clips = vec![std::sync::Arc::new(ss.project.find_clip(&c).unwrap().clone())]; g.mask = None; g.source = kimo_core::model::Source::Precomp { tracks: vec![it] };
    ss.project.tracks[0].clips.retain(|x| x.id != c); ss.project.tracks[0].clips.push(std::sync::Arc::new(g));
    assert!(ss.project.dangling_refs().is_empty(), "nested ref to a live top-level clip is fine");
    let res = call(&mut ss, "delete_clip", json!({"clip_id": m})); assert_eq!(res["cleared_refs"].as_array().unwrap().len(), 1, "{res}"); assert!(ss.project.dangling_refs().is_empty());
}
fn scoped_setup() -> (Vec<Step<'static>>, String) {
    let (mut s, _t1, _t2, c) = one_clip(); s.push(("create_shared_grade_graph", json!({"id": "warm"}))); s.push(("bind_shared_grade_graphs", json!({"clip_id": c, "graphs": ["warm"]}))); (s, c)
}
#[test] fn deleting_a_clip_with_a_shared_grade_binding_succeeds_and_clears_it() {
    let (s, c) = scoped_setup(); let mut ss = fresh(&s); let orig = pj(&ss);
    let res = dispatch(&mut ss, "delete_clip", &json!({"clip_id": c})).unwrap_or_else(|e| panic!("delete_clip of a grade-bound clip failed: {e}"));
    assert!(res["cleared_refs"].as_array().unwrap().iter().any(|x| x["field"] == "grade_scopes.clip_bindings"), "{res}");
    assert!(ss.project.dangling_refs().is_empty()); assert!(ss.project.grade_scopes.graphs.contains_key("warm"), "the shared graph itself stays");
    call(&mut ss, "undo", json!({})); assert_eq!(pj(&ss), orig);
}
#[test] fn duplicate_of_a_grade_bound_clip_keeps_the_binding() {
    let (s, c) = scoped_setup();
    row(&s, "duplicate_clip", json!({"clip_id": c, "start": 6.0}), |res, _b, a| { let nid = res["clip_id"].as_str().unwrap(); assert_eq!(a["grade_scopes"]["clip_bindings"][nid], json!(["warm"])); });
}
#[test] fn v1_project_has_no_dangling_refs() {
    let mut ss = Session::new(); call(&mut ss, "load_project", json!({"path": concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/data/v1_project.kimo")}));
    assert!(ss.project.dangling_refs().is_empty(), "{:?}", ss.project.dangling_refs());
}
#[test] fn find_clip_does_not_reach_precomp_children_while_flat_clips_does() {
    // PINNED CURRENT BEHAVIOUR (a finding): lookups by id used by tools see only top-level clips; render and Scopes::validate see nested ones too.
    let (s, c, _m) = matte_setup(); let mut ss = fresh(&s);
    let mut g = ss.project.find_clip(&c).unwrap().clone(); g.id = "clip9000".into(); let mut it = ss.project.tracks[0].clone(); it.clips = vec![std::sync::Arc::new(ss.project.find_clip(&c).unwrap().clone())];
    g.source = kimo_core::model::Source::Precomp { tracks: vec![it] }; ss.project.tracks[0].clips.retain(|x| x.id != c); ss.project.tracks[0].clips.push(std::sync::Arc::new(g));
    assert!(ss.project.find_clip(&c).is_none() && ss.project.find_clip_mut(&c).is_none(), "tool lookups do not reach nested clips");
    assert!(render::flat_clips(&ss.project.tracks, 0.0).iter().any(|x| x.id == c), "render/validation do see them");
}
#[test] fn remove_mask_layer_with_a_ref_clip_leaves_no_dangling_ref_and_renders_like_never_had_it() {
    let (mut s, t1, _t2, c) = one_clip();
    s.push(("add_shape", json!({"track_id": t1, "shape": "ellipse", "width": 30, "height": 30, "fill": [1, 1, 1, 1], "start": 1.0, "duration": 2.0})));
    let r = { let ss = fresh(&s); pj(&ss)["tracks"][0]["clips"][1]["id"].as_str().unwrap().to_string() };
    let never = fresh(&s); let mut s2 = s.clone(); s2.push(("add_mask", json!({"clip_id": c, "kind": "ellipse", "ref_clip": r})));
    let mid = { let ss = fresh(&s2); clip(&pj(&ss), &c)["mask"]["layers"][0]["id"].as_str().unwrap().to_string() };
    let ss = row(&s2, "remove_mask_layer", json!({"clip_id": c, "mask_id": mid}), |_r, _b, a| assert!(clip(a, &c)["mask"]["layers"].as_array().map(|l| l.is_empty()).unwrap_or(true)));
    for t in [0.1, 1.5, 2.5] { assert_eq!(frame_bytes(&ss, t), frame_bytes(&never, t), "stale mask layer visible at t={t}"); }
}
#[test] fn set_track_locks_names_and_reorders() {
    let (s, t1, t2, c) = one_clip();
    row(&s, "set_track", json!({"track_id": t1, "name": "Main"}), |_r, b, a| { assert_ne!(b, a); assert_eq!(a["tracks"][0]["name"], "Main"); });
    row(&s, "set_track", json!({"track_id": t1, "locked": true}), |_r, _b, a| assert_eq!(a["tracks"][0]["locked"], true));
    let mut ss = fresh(&s); call(&mut ss, "set_track", json!({"track_id": t1, "locked": true}));
    assert!(dispatch(&mut ss, "ripple_delete", &json!({"clip_id": c})).unwrap_err().contains("locked"), "the edit.rs tools honour the lock");

    row(&s, "set_track", json!({"track_id": t2, "index": 0}), |_r, _b, a| assert_eq!(a["tracks"][0]["id"], t2.as_str()));
    row_err(&s, "set_track", json!({"track_id": "nope", "locked": true}), Some("no such track"));
}
#[test] fn remove_silence_cuts_extracts_ranges_and_closes_the_gap() {
    let (s, t1, _t2, _c) = one_clip(); // one clip 1.0..3.0
    let ss = row(&s, "remove_silence_cuts", json!({"track_id": t1, "ranges": [[1.5, 2.0]]}), |_r, b, a| { assert_ne!(b, a); assert!(clips(a).len() >= 2, "the clip is cut around the range"); });
    let d = ss.project.duration(); assert!((d - 2.5).abs() < 1e-6, "0.5 s extracted from a clip ending at 3.0 leaves 2.5, got {d}");
    row_err(&s, "remove_silence_cuts", json!({"track_id": "nope", "ranges": [[1.0, 1.5]]}), None);
}
#[test] fn set_camera_sets_base_values_and_validates() {
    let (s, _t1, _t2, _c) = one_clip();
    row(&s, "set_camera", json!({"cam_z": 120.0, "cam_fov": 60.0}), |_r, b, a| { assert_ne!(b, a); assert_eq!(a["camera"]["cam_z"]["base"], 120.0); assert_eq!(a["camera"]["cam_fov"]["base"], 60.0); });
    row_err(&s, "set_camera", json!({"cam_fov": 3.0}), Some("5..170")); row_err(&s, "set_camera", json!({"cam_fov": 171.0}), Some("5..170"));
}
#[test] fn add_counter_and_prompt_box_add_clips_and_survive_undo() {
    let (s, t1, _t2, _c) = one_clip(); std::env::set_var("KIMO_FONTS", concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts")); assert!(std::path::Path::new(&kimo_core::text::resolve_font("Montserrat")).ends_with("tests/fonts/Montserrat-Var.ttf"), "text test needs tests/fonts/Montserrat-Var.ttf, no silent system-font fallback");
    let ss = row(&s, "add_counter", json!({"track_id": t1, "start": 0.0, "duration": 1.0, "from": 0, "to": 100, "prefix": "$"}), |_r, b, a| assert!(clips(a).len() > clips(b).len(), "counter adds clips"));
    renders_ok(&ss);
    let ss = row(&s, "add_prompt_box", json!({"track_id": t1, "start": 0.0, "duration": 2.0, "text": "make it pop"}), |_r, b, a| assert!(clips(a).len() > clips(b).len(), "prompt box adds clips"));
    renders_ok(&ss);
    row_err(&s, "add_counter", json!({"track_id": "nope", "start": 0.0, "duration": 1.0}), None);
}
#[test] fn get_scopes_is_read_only_and_reports_stats() {
    let (s, _t1, _t2, _c) = one_clip(); let mut ss = fresh(&s); let before = pj(&ss);
    let r = call(&mut ss, "get_scopes", json!({"time": 1.5})); assert_eq!(pj(&ss), before, "read-only"); assert!(r.is_object() && !r.as_object().unwrap().is_empty(), "{r}");
    eprintln!("get_scopes keys: {:?}", r.as_object().unwrap().keys().collect::<Vec<_>>());
}
#[test] fn a_locked_track_blocks_every_tool_that_would_change_its_clips() {
    let (s, t1, t2, c) = one_clip();
    let cases: Vec<(&str, Value)> = vec![
        ("move_clip", json!({"clip_id": c, "start": 0.5})), ("delete_clip", json!({"clip_id": c})), ("trim_clip", json!({"clip_id": c, "duration": 1.0})),
        ("set_property", json!({"clip_id": c, "property": "opacity", "value": 0.5})), ("add_effect", json!({"clip_id": c, "kind": "blur", "params": {"radius": 3}})),
        ("set_keyframe", json!({"clip_id": c, "property": "opacity", "time": 0.5, "value": 0.2})), ("duplicate_clip", json!({"clip_id": c, "start": 5.0})),
        ("split_clip", json!({"clip_id": c, "at": 2.0})), ("add_shape", json!({"track_id": t1, "shape": "rect", "width": 10, "height": 10, "fill": [1, 1, 1, 1], "start": 4.0, "duration": 1.0})),
        ("add_mask", json!({"clip_id": c, "kind": "ellipse"})),
    ];
    for (tool, args) in cases {
        let mut ss = fresh(&s); call(&mut ss, "set_track", json!({"track_id": t1, "locked": true})); let before = pj(&ss); let depth = ss.undo.len();
        let e = dispatch(&mut ss, tool, &args).expect_err(&format!("{tool} must be blocked on a locked track")); assert!(e.contains("locked"), "{tool}: {e}");
        assert_eq!(pj(&ss), before, "{tool}: project must be byte-identical after a blocked call"); assert_eq!(ss.undo.len(), depth, "{tool}: undo stack unchanged");
        call(&mut ss, "set_track", json!({"track_id": t1, "locked": false})); dispatch(&mut ss, tool, &args).unwrap_or_else(|e| panic!("{tool} after unlock: {e}"));
    }
    // other tracks are unaffected, reads and renders on locked tracks work
    let mut ss = fresh(&s); call(&mut ss, "set_track", json!({"track_id": t1, "locked": true}));
    call(&mut ss, "add_shape", json!({"track_id": t2, "shape": "rect", "width": 10, "height": 10, "fill": [1, 1, 1, 1], "start": 0.0, "duration": 1.0})); call(&mut ss, "get_scopes", json!({"time": 1.5})); renders_ok(&ss);
    call(&mut ss, "add_track", json!({}));
}
#[test] fn loop_clip_repeats_with_gap_and_validates() {
    let (s, t1, _t2, c) = one_clip(); // clip 1.0..3.0 (duration 2)
    let ss = row(&s, "loop_clip", json!({"clip_id": c, "times": 3, "gap": 0.5}), |r, b, a| {
        assert_eq!(clips(a).len(), clips(b).len() + 3); let ids: Vec<String> = r["clip_ids"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
        for (k, id) in ids.iter().enumerate() { let cc = clip(a, id); assert!((cc["start"].as_f64().unwrap() - (1.0 + (k as f64 + 1.0) * 2.5)).abs() < 1e-9); assert_eq!(cc["duration"], clip(a, &c)["duration"]); }
    });
    renders_ok(&ss); let _ = t1;
    row_err(&s, "loop_clip", json!({"clip_id": c, "times": 0}), Some("times")); row_err(&s, "loop_clip", json!({"clip_id": c, "times": 2.5}), Some("times")); row_err(&s, "loop_clip", json!({"clip_id": c, "times": 101}), Some("times"));
    row_err(&s, "loop_clip", json!({"clip_id": c, "times": 1, "gap": -1.0}), Some("gap")); row_err(&s, "loop_clip", json!({"clip_id": "nope", "times": 1}), None);
    let (sg, cg) = scoped_setup(); row(&sg, "loop_clip", json!({"clip_id": cg, "times": 1}), |r, _b, a| { let nid = r["clip_ids"][0].as_str().unwrap(); assert_eq!(a["grade_scopes"]["clip_bindings"][nid], json!(["warm"])); });
}
#[test] fn distribute_clips_lays_clips_end_to_end_in_start_order() {
    let (mut s, t1, _t2, c) = one_clip(); s.push(("add_shape", json!({"track_id": t1, "shape": "rect", "width": 10, "height": 10, "fill": [1, 1, 1, 1], "start": 7.0, "duration": 1.0}))); s.push(("add_shape", json!({"track_id": t1, "shape": "rect", "width": 10, "height": 10, "fill": [1, 1, 1, 1], "start": 4.0, "duration": 0.5})));
    let ss = row(&s, "distribute_clips", json!({"track_id": t1, "gap": 0.25}), |r, _b, a| { assert_eq!(r["moved"], 2); assert_eq!(clip(a, &c)["start"], 1.0, "first clip keeps its start"); });
    let mut st: Vec<(f64, f64)> = clips(&pj(&ss)).iter().map(|x| (x["start"].as_f64().unwrap(), x["duration"].as_f64().unwrap())).collect(); st.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(st, vec![(1.0, 2.0), (3.25, 0.5), (4.0, 1.0)]);
    row(&s, "distribute_clips", json!({"track_id": t1, "start": 0.0}), |_r, _b, a| assert_eq!(clip(a, &c)["start"], 0.0));
    // already in place: nothing changes and registry mutates flag holds via row()
    let (s1, t, _t2, _c1) = one_clip(); let mut ss2 = fresh(&s1); let before = pj(&ss2); let r = call(&mut ss2, "distribute_clips", json!({"track_id": t})); assert_eq!(r["moved"], 0); assert_eq!(pj(&ss2), before);
    row_err(&s, "distribute_clips", json!({"track_id": t1, "gap": -1.0}), Some("gap")); row_err(&s, "distribute_clips", json!({"track_id": "nope"}), Some("no such track"));
}
#[test] fn swap_clips_exchanges_order_within_the_same_span() {
    let (mut s, t1, t2, c) = one_clip(); // c: 1.0..3.0
    s.push(("add_shape", json!({"track_id": t1, "shape": "rect", "width": 10, "height": 10, "fill": [1, 1, 1, 1], "start": 3.5, "duration": 1.0})));
    let d = { let ss = fresh(&s); pj(&ss)["tracks"][0]["clips"][1]["id"].as_str().unwrap().to_string() };
    row(&s, "swap_clips", json!({"clip_a": c, "clip_b": d}), |r, _b, a| {
        assert_eq!(r["first"], d.as_str()); assert!((clip(a, &d)["start"].as_f64().unwrap() - 1.0).abs() < 1e-9); assert!((clip(a, &c)["start"].as_f64().unwrap() - 2.5).abs() < 1e-9, "span 1.0..4.5 kept, gap 0.5 kept");
    });
    row(&s, "swap_clips", json!({"clip_a": d, "clip_b": c}), |_r, _b, a| assert!((clip(a, &d)["start"].as_f64().unwrap() - 1.0).abs() < 1e-9));
    row_err(&s, "swap_clips", json!({"clip_a": c, "clip_b": c}), Some("differ")); row_err(&s, "swap_clips", json!({"clip_a": c, "clip_b": "nope"}), None);
    let mut s2 = s.clone(); s2.push(("add_shape", json!({"track_id": t2, "shape": "rect", "width": 10, "height": 10, "fill": [1, 1, 1, 1], "start": 1.0, "duration": 1.0})));
    let e = { let ss = fresh(&s2); pj(&ss)["tracks"][1]["clips"][0]["id"].as_str().unwrap().to_string() }; row_err(&s2, "swap_clips", json!({"clip_a": c, "clip_b": e}), Some("same track"));
    let mut s3 = s.clone(); s3.push(("add_shape", json!({"track_id": t1, "shape": "rect", "width": 10, "height": 10, "fill": [1, 1, 1, 1], "start": 3.1, "duration": 0.3}))); row_err(&s3, "swap_clips", json!({"clip_a": c, "clip_b": d}), Some("within the span"));
}
#[test] fn dangling_refs_validator_can_fail() {
    let (s, _c, m) = matte_setup(); let mut ss = fresh(&s); ss.project.tracks.iter_mut().for_each(|t| t.clips.retain(|c| c.id != m));
    assert_eq!(ss.project.dangling_refs().len(), 1, "raw removal bypassing the tools must be detected");
}

fn write_wav(path: &std::path::Path, hz: f64, amp: f64, secs: f64) {
    let sr = 48000u32; let n = (sr as f64 * secs) as usize; let mut b = Vec::new(); let data = (n * 2) as u32;
    b.extend(b"RIFF"); b.extend((36 + data).to_le_bytes()); b.extend(b"WAVEfmt "); b.extend(16u32.to_le_bytes()); b.extend(1u16.to_le_bytes()); b.extend(1u16.to_le_bytes()); b.extend(sr.to_le_bytes()); b.extend((sr * 2).to_le_bytes()); b.extend(2u16.to_le_bytes()); b.extend(16u16.to_le_bytes()); b.extend(b"data"); b.extend(data.to_le_bytes());
    for i in 0..n { let v = (amp * (2.0 * std::f64::consts::PI * hz * i as f64 / sr as f64).sin() * 32767.0) as i16; b.extend(v.to_le_bytes()); }
    std::fs::write(path, b).unwrap();
}
/// Needs ffmpeg/ffprobe (audio decode), so it is ignored by default: run with `cargo test --release -p kimo-core --test tool_contracts -- --ignored`.
#[test] #[ignore = "needs ffmpeg for audio decode"]
fn auto_duck_leaves_clips_outside_the_dialogue_shared_with_the_undo_snapshot() {
    use std::sync::Arc;
    let dir = std::env::temp_dir().join(format!("kimo-duck-{}", std::process::id())); std::fs::create_dir_all(&dir).unwrap(); let (mw, vw) = (dir.join("music.wav"), dir.join("voice.wav")); write_wav(&mw, 220.0, 0.3, 1.0); write_wav(&vw, 440.0, 0.6, 1.0);
    let mut ss = fresh(&[]); let tm = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let td = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let ma = call(&mut ss, "import_media", json!({"path": mw.to_str().unwrap()})); let va = call(&mut ss, "import_media", json!({"path": vw.to_str().unwrap()}));
    let (ma, va) = (ma["asset_id"].as_str().unwrap().to_string(), va["asset_id"].as_str().unwrap().to_string());
    for i in 0..4 { call(&mut ss, "add_audio_clip", json!({"track_id": tm, "asset_id": ma, "start": i as f64, "duration": 1.0})); }
    call(&mut ss, "add_audio_clip", json!({"track_id": td, "asset_id": va, "start": 1.0, "duration": 1.0}));
    let before = pj(&ss); call(&mut ss, "auto_duck", json!({"track": tm, "under": td, "amount_db": -12.0}));
    let (cur, prev) = (&ss.project.tracks[0].clips, &ss.undo.last().unwrap().tracks[0].clips); assert_eq!(cur.len(), 4);
    let shared: Vec<bool> = cur.iter().zip(prev.iter()).map(|(a, b)| Arc::ptr_eq(a, b)).collect();
    // the clip ending exactly where the voice starts (0..1 s) legitimately gets an envelope key at t=1.0 (inclusive end sample), so it is copied; recorded behaviour
    assert_eq!(&shared[1..], &[false, true, true], "the clip under the voice is copied; the two clips after it stay shared: {shared:?}"); eprintln!("auto_duck shared flags per clip: {shared:?}");
    call(&mut ss, "undo", json!({})); assert_eq!(pj(&ss), before); let _ = std::fs::remove_dir_all(&dir);
}

// ---- RV-7 delivery tools (apply/undo of relink_media is covered in tests/delivery.rs; its contract here is argument validation)
#[test] fn list_dependencies_and_collect_project_leave_the_project_unchanged() {
    let (s, _t1, _t2, _c) = one_clip();
    row(&s, "list_dependencies", json!({}), |res, _b, _a| { assert_eq!(res["summary"]["missing"], 0); assert!(res["items"].is_array()); });
    let d = std::env::temp_dir().join(format!("kimo_tc_collect_{}", std::process::id())); let _ = std::fs::remove_dir_all(&d);
    row(&s, "collect_project", json!({"dest_dir": d.to_str().unwrap()}), |res, _b, _a| { assert_eq!(res["missing"], 0); assert!(d.join("project.kimo").exists() && d.join("collect_manifest.json").exists()); assert!(!d.join(".collect-tmp").exists()); });
    let _ = std::fs::remove_dir_all(&d);
}
#[test] fn collect_and_relink_reject_bad_arguments_without_touching_the_project() {
    let (s, _t1, _t2, _c) = one_clip();
    row_err(&s, "collect_project", json!({"dest_dir": "/tmp", "copy": "zip"}), Some("copy must be"));
    row_err(&s, "collect_project", json!({"dest_dir": "/tmp"}), Some("dest_exists"));
    row_err(&s, "relink_media", json!({"search_dirs": []}), Some("search_dirs"));
    row_err(&s, "relink_media", json!({"search_dirs": ["/nonexistent_kimo_dir"]}), Some("not a directory"));
    row_err(&s, "relink_media", json!({"search_dirs": ["/tmp"], "match": "x"}), Some("match must be"));
}

// ---- RV-8 transcript tools: row() enforces apply/undo/redo byte-exactness and that the registry `mutates` flag matches whether the project changed.
fn tr_csv() -> String {
    let p = std::env::temp_dir().join(format!("kimo_tc_tr_{}.csv", std::process::id())); let mut t = 0.0; let mut s = String::from("word,start,end,confidence\n");
    for (i, w) in "Cold tyres read lower than warm ones because the air inside contracts. Check them before you drive.".split_whitespace().enumerate() { s.push_str(&format!("{},{:.2},{:.2},{}\n", w, t, t + 0.4, if i == 1 { 0.2 } else { 0.9 })); t += 0.4; }
    std::fs::write(&p, s).unwrap(); p.to_str().unwrap().to_string()
}
#[test] fn transcript_tools_contracts() {
    let csv = tr_csv(); let t1 = pj(&fresh(&base()))["tracks"][0]["id"].as_str().unwrap().to_string();
    let imp: Step = ("transcript_import", json!({"path": csv})); let mut with_tr = base(); with_tr.push(imp.clone()); let mut with_caps = with_tr.clone(); with_caps.push(("transcript_to_captions", json!({"track_id": t1})));
    row(&base(), "transcript_import", imp.1.clone(), |r, b, a| { assert_eq!(r["words_added"], 17); assert!(b["editorial"].get("transcripts").is_none() && a["editorial"]["transcripts"]["tr1"].is_object()); });
    row(&with_tr, "transcript_get", json!({"include_words": true}), |r, _b, _a| { assert_eq!(r["untrusted"], true); assert_eq!(r["word_count"], 17); });
    row(&with_tr, "transcript_edit_word", json!({"from_index": 1, "to_index": 1, "display": "tires"}), |r, _b, a| { assert!(r.is_object()); assert_eq!(a["editorial"]["transcripts"]["tr1"]["words"][1]["display"], "tires"); });
    row(&with_tr, "transcript_set_take", json!({"take_id": "take1", "speed": 2.0}), |r, _b, _a| assert!(r["words_moved"].as_u64().unwrap() >= 16));
    row(&with_tr, "transcript_coverage", json!({"script_text": "Cold tyres read lower than warm ones"}), |r, _b, _a| { assert!(r["coverage_pct"].as_f64().unwrap() >= 99.0); assert!(r["timeline"]["takes"].is_array()); });
    row(&with_tr, "transcript_to_captions", json!({"track_id": t1}), |r, _b, a| { assert_eq!(r["overlaps"], 0); assert!(a["editorial"]["captions"].as_object().unwrap().len() == 1); });
    row(&with_caps, "validate_captions", json!({"all": true}), |r, _b, _a| { assert_eq!(r["groups"].as_array().unwrap().len(), 1); assert_eq!(r["skipped"].as_array().unwrap().len(), 3); });
    row(&with_caps, "transcript_edit_preview", json!({"edits": [{"op": "delete", "from_index": 2, "to_index": 3}]}), |r, _b, _a| { assert_eq!(r["read_only"], true); assert_eq!(r["audio_cut_ranges"].as_array().unwrap().len(), 1); });
    row_err(&with_tr, "transcript_import", json!({}), Some("path"));
    row_err(&base(), "transcript_import", json!({"path": "/nonexistent/x.srt"}), Some("cannot read"));
    row_err(&with_tr, "transcript_get", json!({"transcript_id": "nope"}), Some("no such transcript"));
    row_err(&with_tr, "transcript_edit_word", json!({"word_id": "w1"}), Some("display"));
    row_err(&with_tr, "transcript_set_take", json!({"take_id": "take1", "speed": 0.0}), Some("speed"));
    row_err(&with_tr, "transcript_coverage", json!({}), Some("script_text"));
    row_err(&with_tr, "transcript_to_captions", json!({"track_id": "nope"}), Some("track"));
    row_err(&with_caps, "validate_captions", json!({}), Some("group_id"));
    row_err(&with_tr, "transcript_edit_preview", json!({"edits": [{"op": "delete", "from_index": 999}]}), Some("outside"));
}
#[test] fn batch_rows_success_dry_run_and_structured_failures() {
    let (s, t1, _t2, c) = one_clip();
    let ops = json!([{"id": "a", "tool": "add_shape", "args": {"track_id": t1, "shape": "rect", "width": 8, "height": 8, "start": 0.0, "duration": 1.0}}, {"tool": "set_blend", "args": {"clip_id": "$a.clip_id", "mode": "add"}}]);
    row(&s, "batch", json!({"ops": ops}), |r, b, a| { assert_eq!(r["applied"], 2); assert_eq!(r["created"]["clip_ids"].as_array().unwrap().len(), 1); assert_ne!(b, a); });
    { let mut ss = fresh(&s); let b = pj(&ss); let r = call(&mut ss, "batch", json!({"ops": ops, "dry_run": true})); assert!(r["revision"].is_string()); assert_eq!(pj(&ss), b, "dry_run changes nothing"); }
    row_err(&s, "batch", json!({"ops": [{"tool": "undo"}]}), Some("forbidden_in_batch"));
    row_err(&s, "batch", json!({"ops": [{"tool": "add_track", "args": {"name": "$zz.x"}}]}), Some("invalid_reference"));
    row_err(&s, "batch", json!({"ops": [{"tool": "delete_clip", "args": {"clip_id": c}}, {"tool": "delete_clip", "args": {"clip_id": c}}]}), Some("batch_failed"));
    row_err(&s, "batch", json!({"ops": [{"tool": "add_track"}], "expect_revision": "deadbeef", "dry_run": false}), Some("project_drift"));
}
// RV-12 explicit-window auto_duck contracts: project state and history remain byte-exact.
#[test] fn auto_duck_explicit_windows_overlap_and_under_ignored_contracts() {
 let(s,t,_under,c)=one_clip();
 for windows in [json!([[1.2,2.4]]),json!([[1.2,2.0],[1.8,2.4]])] {
  row(&s,"auto_duck",json!({"track":t,"speech_windows":windows,"amount_db":-18}),|r,_,a|{assert_eq!(r["mode"],"explicit_windows");let keys=clip(a,&c)["audio"]["duck_gain"]["keys"].as_array().unwrap().clone();assert!(keys.iter().any(|k|(k["v"].as_f64().unwrap()-10f64.powf(-18.0/20.0)).abs()<1e-12));});
 }
 row(&s,"auto_duck",json!({"track":t,"under":"absent ignored","speech_windows":[[1.2,2.4]]}),|r,_,_|assert_eq!(r["under_ignored"],true));
 row_err(&s,"auto_duck",json!({"track":t}),Some("under"));
}
#[test] fn auto_duck_explicit_empty_clears_previous_envelope_contract() {
 let(mut s,t,_,c)=one_clip();s.push(("auto_duck",json!({"track":t,"speech_windows":[[1.2,2.4]]})));
 row(&s,"auto_duck",json!({"track":t,"speech_windows":[]}),|r,_,a|{assert_eq!(r["envelope_cleared"],true);assert!(clip(a,&c)["audio"]["duck_gain"]["keys"].as_array().unwrap().is_empty());});
}
#[test] fn recipe_tool_rows_list_describe_apply_and_structured_failures() {
    let (s, t1, _t2, _c) = one_clip();
    row(&s, "recipe_list", json!({"query": "title"}), |r, _, _| { assert!(r["count"].as_u64().unwrap() >= 2); });
    row(&s, "recipe_describe", json!({"name": "title_card"}), |r, _, _| { assert_eq!(r["recipe_version"], 1); assert!(r["params"]["properties"]["easing"]["enum"].is_array()); });
    row(&s, "recipe_apply", json!({"name": "title_card", "start": 0.5, "params": {"title": "Hi", "subtitle": "there"}, "track_id": t1}), |r, b, a| { assert_eq!(r["ok"], true); assert_ne!(b, a); assert!(r["roles"]["title_card/title"].is_string()); assert_eq!(r["anchors"]["unit"], "timeline seconds"); });
    row_err(&s, "recipe_describe", json!({"name": "nope"}), Some("Valid:"));
    row_err(&s, "recipe_apply", json!({"name": "title_card", "start": 0.0, "params": {"title": "x", "easing": "wobble"}}), Some("Valid:"));
    row_err(&s, "recipe_apply", json!({"name": "title_card", "start": -1.0, "params": {"title": "x"}}), Some("start"));
    row_err(&s, "recipe_apply", json!({"name": "title_card", "params": {"title": "x"}}), None);
}
#[test] fn audio_speech_intervals_silent_contract_and_errors(){
 let mut s=Session::new();let t=call(&mut s,"add_track",json!({}))["track_id"].clone();call(&mut s,"add_clip",json!({"track_id":t,"source_type":"solid","start":0,"duration":1}));
let before=pj(&s);let r=call(&mut s,"audio_speech_intervals",json!({"target":"mix"}));assert_eq!(r["speech"],json!([]));assert_eq!(r["count"],0);assert_eq!(r["speech_ratio"],0.);assert_eq!(r["truncated"],false);assert_eq!(r["params"]["pad_ms"],120.);assert_eq!(r["method"],"energy_zcr_spectral_vad_v1");assert_eq!(pj(&s),before);assert_eq!(r,call(&mut s,"audio_speech_intervals",json!({"target":"mix"})));
 let a=call(&mut s,"audio_probe",json!({"target":{"mix":true}}));let b=call(&mut s,"audio_speech_intervals",json!({"target":"mix","mode":"silence"}));assert_eq!(a["silence"],b["silence"]);
 let wave=call(&mut s,"audio_speech_intervals",json!({"target":"mix","image":"waveform"}));assert!(std::path::Path::new(wave["waveform"]["path"].as_str().unwrap()).exists());assert_eq!(pj(&s),before);assert!(dispatch(&mut s,"audio_speech_intervals",&json!({"target":{"clip_id":"missing"}})).is_err());assert!(dispatch(&mut s,"audio_speech_intervals",&json!({"aggressiveness":4})).is_err());
}
#[test]fn audio_transients_contract_read_only_silence_and_errors(){
 let(s,_,_,_)=one_clip();row(&s,"audio_transients",json!({"target":"mix"}),|r,b,a|{assert_eq!(r["count"],0);assert_eq!(a,b);});
 row_err(&s,"audio_transients",json!({"target":"mix","start":100}),Some("range_empty"));row_err(&s,"audio_transients",json!({"mode":"bogus"}),Some("Valid:"));
}
#[test] fn set_keyframes_rows_write_atomically_and_reject_bad_keys() {
    let (s, _t1, _t2, c) = one_clip();
    row(&s, "set_keyframes", json!({"clip_id": c, "property": "opacity", "keys": [{"t": 2.0, "v": 3.0}, {"t": 0.0, "v": 0.0, "ease": "out"}]}), |r, b, a| { assert_eq!(r["written"], 2); assert_ne!(b, a); });
    row_err(&s, "set_keyframes", json!({"clip_id": c, "property": "opacity", "keys": [{"t": 0.0, "v": 1.0}, {"t": 1.0}]}), Some("keys[1]"));
    row_err(&s, "set_keyframes", json!({"clip_id": c, "property": "opacity", "keys": [{"t": 0.0, "v": 1.0, "ease": "wobble"}]}), Some("Valid:"));
    row_err(&s, "set_keyframes", json!({"clip_id": c, "property": "nope", "keys": [{"t": 0.0, "v": 1.0}]}), Some("Valid:"));
}
#[test] fn set_expression_generator_rows() {
    let (s, _t1, _t2, c) = one_clip();
    row(&s, "set_expression", json!({"clip_id": c, "property": "opacity", "op": "spring", "from": 0.0, "to": 1.0}), |r, b, a| { assert_ne!(b, a); assert_eq!(r["ops"], 1); });
    row(&s, "set_expression", json!({"clip_id": c, "property": "x", "op": "noise_impulse", "seed": 3, "combine": "add"}), |r, _, _| { assert_eq!(r["ops"], 1); });
    row(&s, "set_expression", json!({"clip_id": c, "property": "x", "op": "bezier", "duration": 2.0}), |r, _, _| { assert_eq!(r["ops"], 1); });
    row_err(&s, "set_expression", json!({"clip_id": c, "property": "x", "op": "spring", "mass": -1.0}), Some("mass"));
    row_err(&s, "set_expression", json!({"clip_id": c, "property": "x", "op": "bezier", "duration": 0.0}), Some("duration"));
}
#[test] fn bake_property_rows_replace_keys_and_drop_expressions() {
    let (s, _t1, _t2, c) = one_clip();
    row(&s, "bake_property", json!({"clip_id": c, "property": "opacity", "start": 0.0, "end": 1.0, "n": 5}), |r, b, a| { assert_ne!(b, a); assert_eq!(r["keys"], 5); assert_eq!(r["expressions_dropped"], 0); });
    row(&s, "bake_property", json!({"clip_id": c, "property": "x", "start": 0.0, "end": 2.0, "fps": 10}), |r, _, _| { assert_eq!(r["keys"], 21); });
    row_err(&s, "bake_property", json!({"clip_id": c, "property": "nope", "start": 0.0, "end": 1.0, "n": 3}), Some("Properties on this clip"));
    row_err(&s, "bake_property", json!({"clip_id": c, "property": "x", "start": 0.0, "end": 1.0}), Some("fps"));
}
#[test] fn sample_property_rows_read_without_changing_anything() {
    let (s, _t1, _t2, c) = one_clip();
    row(&s, "sample_property", json!({"clip_id": c, "property": "opacity", "times": [0.0, 1.5, 2.5]}), |r, b, a| { assert_eq!(b, a); assert_eq!(r["values"].as_array().unwrap().len(), 3); assert_eq!(r["active"].as_array().unwrap().len(), 3); });
    row(&s, "sample_property", json!({"clip_id": c, "property": "x", "times": {"start": 0.0, "end": 3.0, "n": 4}, "space": "clip"}), |r, _, _| { assert_eq!(r["space"], "clip"); });
    row_err(&s, "sample_property", json!({"clip_id": c, "property": "nope", "times": [0.0]}), Some("Properties on this clip"));
    row_err(&s, "sample_property", json!({"clip_id": c, "property": "x", "times": []}), Some("1 to"));
}
#[test]fn shader_resource_readonly_rows(){for tool in ["get_limits","set_limits"]{row(&[],tool,json!({}),|_,a,b|assert_eq!(a,b));}row(&[],"effect_describe",json!({"kind":"pixel_expr"}),|r,a,b|{assert_eq!(a,b);assert!(r["formula"].as_str().unwrap().contains("expression"));});row(&[],"read_pixels",json!({"time":0,"region":{"x":0,"y":0,"w":1,"h":1}}),|r,a,b|{assert_eq!(a,b);assert_eq!(r["count"],1);});}

#[test]fn raw_audio_key_mutation_and_response_readonly_contracts(){let(s,_,_,_)=one_clip();row(&s,"audio_filter_response",json!({"coeffs":[1],"freqs":[100,1000]}),|r,b,a|{assert_eq!(a,b);assert_eq!(r["response"][0]["magnitude_db"],0.);});row(&s,"audio_effect_describe",json!({"kind":"biquad"}),|r,b,a|{assert_eq!(a,b);assert!(r["formula"].as_str().unwrap().contains("DF-II"));});row_err(&s,"read_samples",json!({"rate":44100}),Some("fixed 48000"));row_err(&s,"import_samples",json!({"rate":0,"channels":2,"data":[]}),Some("rate"));}

#[test] fn transform_node_rows() {
    let (s, t1, _t2, c) = one_clip();
    row(&s, "add_null", json!({"track_id": t1, "start": 0.0, "duration": 2.0, "x": 5.0}), |r, b, a| { assert_ne!(b, a); assert_eq!(r["null"], true); });
    let nul = json!({"track_id": t1, "start": 0.0, "duration": 2.0, "x": 5.0}); let mut s2 = s.clone(); s2.push(("add_null", nul.clone())); let mut probe = fresh(&s); let pid = call(&mut probe, "add_null", nul)["clip_id"].as_str().unwrap().to_string();
    row(&s2, "set_parent", json!({"clip_id": c, "parent_id": pid}), |r, b, a| { assert_ne!(b, a); assert!(r["adjusted"].is_object()); });
    row(&s, "get_transform", json!({"clip_id": c, "t": 0.5, "space": "canvas_px"}), |r, b, a| { assert_eq!(b, a); assert_eq!(r["singular"], false); });
    row_err(&s, "set_parent", json!({"clip_id": c, "parent_id": c}), Some("parent_cycle"));
    row_err(&s, "get_transform", json!({"clip_id": c, "space": "zzz"}), Some("Valid:"));
}

#[test] fn name_meta_rows() {
    let (s, t1, _t2, c) = one_clip();
    row(&s, "set_meta", json!({"id": c, "name": "hero", "role": "bg", "add_tags": ["lint:allow_bleed"]}), |r, b, a| { assert_ne!(b, a); assert_eq!(r["name"], "hero"); });
    row(&s, "set_meta", json!({"target": "track", "id": t1, "name": "main"}), |_, b, a| { assert_ne!(b, a); });
    row(&s, "find", json!({"name_glob": "*", "limit": 5}), |r, b, a| { assert_eq!(b, a); assert!(r["total"].as_u64().unwrap() >= 1); });
    row_err(&s, "set_meta", json!({"id": c, "name": "bad name"}), Some("invalid_name"));
    row_err(&s, "find", json!({"limit": 0}), Some("limit"));
}

#[test] fn project_diff_rows() {
    let (s, _t1, _t2, _c) = one_clip();
    row(&s, "project_diff", json!({"a": "current", "b": "current"}), |r, b, a| { assert_eq!(b, a); assert_eq!(r["identical"], true); });
    row_err(&s, "project_diff", json!({"a": "undo:0", "b": "current"}), Some("n >= 1"));
}
