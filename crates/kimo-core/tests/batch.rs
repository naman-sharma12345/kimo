//! RV-11 `batch`: atomicity, one history step, equivalence with individual calls, references, dry_run, forbidden ops, locks, drift.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};

fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
fn fresh() -> Session { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); ss }
/// A scene of `n` shape clips: op 0 adds a track, then add_shape ops with an id each.
fn scene_ops(n: usize) -> Vec<Value> {
    let mut ops = vec![json!({"id": "t", "tool": "add_track", "args": {}})];
    for i in 0..n { ops.push(json!({"id": format!("s{i}"), "tool": "add_shape", "args": {"track_id": "$t.track_id", "shape": "rect", "width": 20 + i, "height": 10, "fill": [1, 0.5, 0, 1], "start": i as f64 * 0.1, "start": 0.0, "duration": 1.0}})); }
    ops
}
fn hash_frames(ss: &Session) -> Vec<u64> { let cache = render::Cache::new(); [0.0, 0.3, 0.7, 1.2, 2.0].iter().map(|t| { let f = render::render_frame(&ss.project, *t, &cache).unwrap(); f.d.iter().fold(1469598103934665603u64, |h, x| (h ^ x.to_bits() as u64).wrapping_mul(1099511628211)) }).collect() }

#[test] fn validation_catches_every_problem_before_anything_runs() {
    let mut ss = fresh(); let mut ops = scene_ops(29); ops[17] = json!({"id": "bad", "tool": "set_blend", "args": {"clip_id": "$s1.clip_id", "mode": "bogus"}}); ops[25] = json!({"tool": "nope_tool", "args": {}});
    let (h, u) = (pj(&ss), ss.undo.len()); let m = e(&mut ss, "batch", json!({"ops": ops}));
    assert!(m.starts_with("batch_failed:") && m.contains("ops[17]") && m.contains("Valid:") && m.contains("\"allowed\""), "{m}"); assert!(m.contains("\"index\":25"), "all problems are listed, not just the first: {m}");
    assert_eq!(pj(&ss), h, "nothing applied"); assert_eq!(ss.undo.len(), u);
}
#[test] fn apply_time_failure_rolls_back_project_ids_and_history() {
    let mut ss = fresh(); c(&mut ss, "add_track", json!({})); let (h, u) = (pj(&ss), ss.undo.len()); let next = ss.project.next_id;
    let ops = json!([{"id": "t", "tool": "add_track"}, {"id": "a", "tool": "add_shape", "args": {"track_id": "$t.track_id", "shape": "rect", "width": 10, "height": 10, "start": 0.0, "duration": 1.0}}, {"tool": "delete_clip", "args": {"clip_id": "$a.clip_id"}}, {"id": "late", "tool": "set_blend", "args": {"clip_id": "$a.clip_id", "mode": "add"}}]);
    let m = e(&mut ss, "batch", json!({"ops": ops})); assert!(m.starts_with("batch_failed:") && m.contains("ops[3]") && m.contains("\"rolled_back\":true"), "{m}");
    assert_eq!(pj(&ss), h, "project bytes identical"); assert_eq!(ss.project.next_id, next, "id counter restored"); assert_eq!(ss.undo.len(), u);
    let te = kimo_core::commands::dispatch_ex(&mut ss, "batch", &json!({"ops": ops})).unwrap_err(); assert_eq!((te.code, te.param.as_deref()), ("batch_failed", Some("ops[3]")));
}
#[test] fn one_history_step_undo_redo() {
    let mut ss = fresh(); let before = pj(&ss); let u = ss.undo.len(); let r = c(&mut ss, "batch", json!({"ops": scene_ops(9), "label": "scene"}));
    assert_eq!(r["applied"], 10); assert_eq!(ss.undo.len(), u + 1, "exactly one undo entry"); assert_eq!(ss.undo_labels.last().unwrap(), "batch: scene x10"); assert!(r.get("history_label").is_none());
    let after = pj(&ss); c(&mut ss, "undo", json!({})); assert_eq!(pj(&ss), before); c(&mut ss, "redo", json!({})); assert_eq!(pj(&ss), after);
}
#[test] fn forty_op_scene_as_batch_equals_individual_calls_bytes_and_frames() {
    let ops = scene_ops(39); assert_eq!(ops.len(), 40);
    let mut a = fresh(); let mut ids: std::collections::HashMap<String, Value> = Default::default();
    for o in &ops { let mut args = o["args"].clone(); if let Some(t) = args.get("track_id").and_then(|v| v.as_str()) { if t == "$t.track_id" { args["track_id"] = ids["t"]["track_id"].clone(); } } let r = c(&mut a, o["tool"].as_str().unwrap(), args); ids.insert(o["id"].as_str().unwrap().into(), r); }
    let mut b = fresh(); c(&mut b, "batch", json!({"ops": ops})); assert_eq!(pj(&a), pj(&b), "byte-identical project JSON, ids included"); assert_eq!(hash_frames(&a), hash_frames(&b), "identical frames at 5 times");
    assert_eq!(a.undo.len(), 41, "project_set + one undo entry per call"); assert_eq!(b.undo.len(), 2, "project_set + exactly one batch entry");
}
#[test] fn references_are_exact_typed_nested_and_escapable() {
    let mut ss = fresh();
    let r = c(&mut ss, "batch", json!({"ops": [{"id": "trk", "tool": "add_track"}, {"id": "a", "tool": "add_shape", "args": {"track_id": "$trk.track_id", "shape": "rect", "width": 10, "height": 10, "start": 0.0, "duration": 1.0}},
        {"id": "p", "tool": "get_project"}, {"id": "txt", "tool": "add_text", "args": {"track_id": "$trk.track_id", "text": "$$5 off", "start": 0.0, "duration": "$p.fps"}}, {"tool": "set_blend", "args": {"clip_id": "$a.clip_id", "mode": "normal"}}], "return": "full"}));
    assert_eq!(r["applied"], 5); let a_id = r["results"][1]["result"]["clip_id"].as_str().unwrap().to_string(); assert_eq!(r["results"][4]["result"]["clip_id"].as_str().unwrap_or(&a_id), a_id);
    let txt = r["results"][3]["result"]["clip_id"].as_str().unwrap(); let clip = ss.project.tracks.iter().flat_map(|t| t.clips.iter()).find(|c| c.id == txt).unwrap();
    assert_eq!(clip.duration, 25.0, "$p.fps stays a number (25)"); match &clip.source { kimo_core::model::Source::Text { text, .. } => assert_eq!(text, "$5 off", "$$ is a literal $"), _ => panic!() }
    let m = e(&mut ss, "batch", json!({"ops": [{"id": "x", "tool": "add_track", "args": {"name": "$y.track_id"}}, {"id": "y", "tool": "add_track"}]})); assert!(m.starts_with("invalid_reference:") && m.contains("ops[0]") && m.contains("forward"), "{m}");
    assert!(e(&mut ss, "batch", json!({"ops": [{"tool": "add_track", "args": {"name": "$ghost.x"}}]})).contains("unknown op")); assert!(e(&mut ss, "batch", json!({"ops": [{"tool": "add_track", "args": {"name": "$5"}}]})).contains("malformed"));
}
#[test] fn dry_run_leaves_no_trace_returns_the_real_ids_and_detects_drift() {
    let mut ss = fresh(); let h = pj(&ss); let u = ss.undo.len(); let ops = scene_ops(5);
    let d = c(&mut ss, "batch", json!({"ops": ops, "dry_run": true})); assert_eq!(pj(&ss), h, "project hash unchanged"); assert_eq!(ss.undo.len(), u, "no undo entry"); let rev = d["revision"].as_str().unwrap().to_string();
    let r = c(&mut ss, "batch", json!({"ops": ops, "expect_revision": rev})); assert_eq!(d["created"], r["created"], "dry run and real run create identical ids");
    let mut s2 = fresh(); let d2 = c(&mut s2, "batch", json!({"ops": scene_ops(5), "dry_run": true})); c(&mut s2, "add_track", json!({}));
    let m = e(&mut s2, "batch", json!({"ops": scene_ops(5), "expect_revision": d2["revision"]})); assert!(m.starts_with("project_drift:"), "{m}");
}
#[test] fn forbidden_ops_caps_and_locks() {
    let mut ss = fresh(); let h = pj(&ss);
    for t in ["batch", "undo", "save_project", "export_video", "render_job_start", "checkpoint", "recipe_apply"] { let m = e(&mut ss, "batch", json!({"ops": [{"tool": t, "args": {}}]})); assert!(m.starts_with("forbidden_in_batch:") || m.contains("unknown_tool") || m.contains("unknown tool"), "{t}: {m}"); }
    assert!(e(&mut ss, "batch", json!({"ops": [{"tool": "add_track"}, {"tool": "project_set", "args": {"width": 10}}]})).contains("first op"));
    assert!(e(&mut ss, "batch", json!({"ops": (0..501).map(|_| json!({"tool": "add_track"})).collect::<Vec<_>>()})).contains("limit is 500")); assert!(e(&mut ss, "batch", json!({"ops": []})).contains("at least one"));
    assert_eq!(pj(&ss), h);
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); c(&mut ss, "set_track", json!({"track_id": t, "locked": true})); let h2 = pj(&ss); let u = ss.undo.len();
    let m = e(&mut ss, "batch", json!({"ops": [{"tool": "add_track"}, {"tool": "add_shape", "args": {"track_id": t, "shape": "rect", "width": 5, "height": 5, "start": 0.0, "duration": 1.0}}]})); assert!(m.contains("is locked"), "{m}"); assert_eq!(pj(&ss), h2); assert_eq!(ss.undo.len(), u);
    // validation is conservative: a locked track is rejected up front even if an earlier op unlocks it
    assert!(e(&mut ss, "batch", json!({"ops": [{"tool": "set_track", "args": {"track_id": t, "locked": false}}, {"tool": "add_shape", "args": {"track_id": t, "shape": "rect", "width": 5, "height": 5, "start": 0.0, "duration": 1.0}}]})).contains("is locked"));
}
#[test] fn non_atomic_keeps_done_ops_as_one_entry_and_dry_run_failure_restores() {
    let mut ss = fresh(); let u = ss.undo.len(); let ops = json!([{"id": "t", "tool": "add_track"}, {"tool": "delete_clip", "args": {"clip_id": "nope"}}, {"tool": "add_track"}]);
    let r = c(&mut ss, "batch", json!({"ops": ops, "atomic": false})); assert_eq!((r["ok"].clone(), r["applied"].clone(), r["failed_op"]["index"].clone()), (json!(false), json!(1), json!(1))); assert_eq!(ss.undo.len(), u + 1); assert_eq!(ss.project.tracks.len(), 1);
    let h = pj(&ss); let m = e(&mut ss, "batch", json!({"ops": ops, "dry_run": true})); assert!(m.starts_with("batch_failed:")); assert_eq!(pj(&ss), h);
}
#[test] fn cancel_mid_batch_rolls_back_to_byte_identical_state() {
    let mut ss = fresh(); c(&mut ss, "add_track", json!({})); let (h, u, n) = (pj(&ss), ss.undo.len(), ss.project.next_id);
    // token already cancelled: the first per-op guard check panics; dispatch must restore everything
    let tok = kimo_core::guard::new_token(); tok.store(true, std::sync::atomic::Ordering::Relaxed);
    let r = kimo_core::guard::with_token(&tok, || dispatch(&mut ss, "batch", &json!({"ops": scene_ops(50)})));
    let m = r.unwrap_err(); assert!(m.contains("cancelled"), "{m}");
    assert_eq!(pj(&ss), h); assert_eq!(ss.undo.len(), u); assert_eq!(ss.project.next_id, n);
    // cancel raced against a running 500-op batch: either it completes as ONE step or it rolls back byte-identically
    let tok = kimo_core::guard::new_token(); let t2 = tok.clone(); let th = std::thread::spawn(move || { std::thread::sleep(std::time::Duration::from_micros(300)); t2.store(true, std::sync::atomic::Ordering::Relaxed); });
    let r = kimo_core::guard::with_token(&tok, || dispatch(&mut ss, "batch", &json!({"ops": scene_ops(450)}))); th.join().unwrap();
    match r { Ok(_) => assert_eq!(ss.undo.len(), u + 1), Err(m) => { assert!(m.contains("cancelled"), "{m}"); assert_eq!(pj(&ss), h); assert_eq!(ss.undo.len(), u); assert_eq!(ss.project.next_id, n); } }
}
/// Property: for random VALID op sequences, applying individually (resolving references by hand) equals one batch, byte for byte.
#[test] fn property_random_sequences_individual_calls_equal_batch() {
    let mut seed = 0x9e3779b97f4a7c15u64; let mut rnd = move || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
    for case in 0..25 {
        let mut ops = vec![json!({"id": "t", "tool": "add_track", "args": {}})]; let mut clips: Vec<String> = vec![];
        for i in 0..(3 + rnd() % 10) as usize {
            let k = rnd() % 4;
            if clips.is_empty() || k == 0 { let id = format!("c{i}"); ops.push(json!({"id": id, "tool": "add_shape", "args": {"track_id": "$t.track_id", "shape": "rect", "width": 5 + rnd() % 30, "height": 8, "start": (rnd() % 20) as f64 * 0.1, "duration": 1.0}})); clips.push(id); }
            else { let r = format!("${}.clip_id", clips[(rnd() % clips.len() as u64) as usize]); ops.push(match k { 1 => json!({"tool": "set_blend", "args": {"clip_id": r, "mode": "add"}}), 2 => json!({"tool": "move_clip", "args": {"clip_id": r, "start": (rnd() % 30) as f64 * 0.1}}), _ => json!({"tool": "set_property", "args": {"clip_id": r, "property": "opacity", "value": 0.5}}) }); }
        }
        let mut a = fresh(); let mut ids: std::collections::HashMap<String, Value> = Default::default();
        for o in &ops { let mut args = o["args"].clone(); if let Some(m) = args.as_object_mut() { for (_, v) in m.iter_mut() { if let Some(r) = v.as_str().and_then(|s| s.strip_prefix('$')) { let (id, f) = r.split_once('.').unwrap(); *v = ids[id][f].clone(); } } } let r = c(&mut a, o["tool"].as_str().unwrap(), args); if let Some(id) = o.get("id").and_then(|v| v.as_str()) { ids.insert(id.into(), r); } }
        let mut b = fresh(); c(&mut b, "batch", json!({"ops": ops})); assert_eq!(pj(&a), pj(&b), "case {case}: batch differs from individual calls");
    }
}

fn shape_batch(floor: Option<Value>, dur: f64, atomic: bool) -> (Session, Result<Value, String>) {
    let mut ss = fresh(); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "fill": [1, 1, 1, 1], "start": 0.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    let mut a = json!({"ops": [{"tool": "trim_clip", "args": {"clip_id": id, "duration": dur}}], "atomic": atomic}); if let Some(f) = floor { a["min_clip_frames"] = f; }
    let r = dispatch(&mut ss, "batch", &a); (ss, r)
}
#[test] fn min_clip_frames_floor_blocks_degenerate_clips_and_rolls_back() {
    let (ss0, _) = shape_batch(None, 2.0, true); let base = pj(&ss0);
    let (ss, r) = shape_batch(None, 0.01, true); let m = r.unwrap_err(); assert!(m.starts_with("batch_failed:") && m.contains("clip_too_short") && m.contains("min_clip_frames"), "{m}"); assert_eq!(pj(&ss), base, "rolled back exactly");
    let (_, r) = shape_batch(None, 0.04, true); assert!(r.unwrap_err().contains("clip_too_short"), "1 frame is under the default floor of 2");
    let (_, r) = shape_batch(None, 0.08, true); assert!(r.is_ok(), "exactly 2 frames passes: {r:?}");
    let (_, r) = shape_batch(Some(json!(1)), 0.04, true); assert!(r.is_ok(), "floor 1 allows a 1-frame clip");
    let (_, r) = shape_batch(Some(json!(4)), 0.12, true); assert!(r.unwrap_err().contains("clip_too_short"), "floor 4 rejects 3 frames");
    let (_, r) = shape_batch(None, 0.01, true); let _ = r;
    let (_, r) = shape_batch(Some(json!(0)), 0.001, true); assert!(r.is_ok(), "0 disables the floor: {r:?}");
    for bad in [json!(-1), json!(5), json!("2"), json!(2.5)] { let (_, r) = shape_batch(Some(bad), 1.0, true); assert!(r.unwrap_err().contains("0 to 4")); }
}
#[test] fn min_clip_frames_dry_run_fails_and_non_atomic_warns_and_untouched_clips_are_ignored() {
    let mut ss = fresh(); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let tiny = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "fill": [1, 1, 1, 1], "start": 0.0, "duration": 0.01}))["clip_id"].as_str().unwrap().to_string();
    let ok = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "fill": [1, 1, 1, 1], "start": 1.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    let r = c(&mut ss, "batch", json!({"ops": [{"tool": "set_property", "args": {"clip_id": ok, "property": "opacity", "value": 0.5}}]})); assert_eq!(r["ok"], true, "a pre-existing tiny clip the batch did not touch is not flagged");
    let before = pj(&ss);
    let m = e(&mut ss, "batch", json!({"dry_run": true, "ops": [{"tool": "trim_clip", "args": {"clip_id": ok, "duration": 0.01}}]})); assert!(m.contains("clip_too_short")); assert_eq!(pj(&ss), before);
    let r = c(&mut ss, "batch", json!({"atomic": false, "ops": [{"tool": "trim_clip", "args": {"clip_id": ok, "duration": 0.01}}]})); assert_eq!(r["ok"], true); assert!(r["warnings"].to_string().contains("clip_too_short"), "{r}");
    assert!(ss.project.find_clip(&ok).unwrap().duration < 0.02, "non-atomic keeps the ops"); let _ = tiny;
}
