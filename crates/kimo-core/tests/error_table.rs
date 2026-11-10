//! Unit 5 baseline: ~40 deliberately bad calls and the EXACT error text each returns today. The baseline lives in tests/golden/error_texts.txt and must stay
//! byte-identical through the structured-error work (KIMO_UPDATE_GOLDEN=1 rewrites it). Ids are fixed by a deterministic setup (track1/track2, one shape clip).
use kimo_core::commands::{dispatch, dispatch_ex, Session};
use serde_json::{json, Value};

fn setup() -> (Session, String, String, String) {
    let mut ss = Session::new(); let c = |ss: &mut Session, n: &str, a: Value| dispatch(ss, n, &a).unwrap();
    c(&mut ss, "project_set", json!({"width": 320, "height": 180, "fps": 25}));
    let t1 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let t2 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let cl = c(&mut ss, "add_shape", json!({"track_id": t1, "shape": "rect", "width": 60, "height": 40, "fill": [1, 0.5, 0, 1], "start": 1.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    (ss, t1, t2, cl)
}
fn calls(t1: &str, c: &str) -> Vec<(&'static str, Value)> {
    vec![
        ("no_such_tool", json!({})), ("set_propertyy", json!({})),
        ("delete_clip", json!({})), ("delete_clip", json!({"clip_id": 5})), ("delete_clip", json!({"clip_id": "nope"})),
        ("move_clip", json!({"clip_id": c})), ("move_clip", json!({"clip_id": c, "start": "x"})), ("move_clip", json!({"clip_id": "nope", "start": 1.0})),
        ("trim_clip", json!({"clip_id": c, "duration": -1.0})), ("set_speed", json!({"clip_id": c, "speed": 0.0})), ("set_speed", json!({"clip_id": c, "speed": 1000.0})),
        ("set_property", json!({"clip_id": c, "property": "bogus", "value": 1.0})), ("set_property", json!({"clip_id": c, "property": "opacity"})),
        ("split_clip", json!({"clip_id": c, "at": 99.0})), ("split_clip", json!({"clip_id": c})),
        ("freeze_frame", json!({"clip_id": c, "at": 1.0, "hold_from_here": true})), ("freeze_frame", json!({"clip_id": c, "at": 99.0})),
        ("insert_gap", json!({"track_id": t1, "at": -1.0, "duration": 1.0})), ("insert_gap", json!({"track_id": t1, "at": 1.0, "duration": -1.0})), ("insert_gap", json!({"track_id": "nope", "at": 1.0, "duration": 1.0})),
        ("add_track", json!({"kind": 5})), ("add_shape", json!({"track_id": t1})), ("add_shape", json!({"track_id": "nope", "shape": "rect", "width": 1, "height": 1, "fill": [1, 1, 1, 1], "start": 0.0, "duration": 1.0})),
        ("add_shape", json!({"track_id": t1, "shape": "hexagonal", "width": 1, "height": 1, "fill": [1, 1, 1, 1], "start": 0.0, "duration": 1.0})),
        ("add_effect", json!({"clip_id": c, "kind": "no_such_effect"})), ("add_effect", json!({"clip_id": "nope", "kind": "blur"})),
        ("set_keyframe", json!({"clip_id": c, "property": "opacity", "time": 0.5})), ("set_keyframe", json!({"clip_id": c, "property": "bogus", "time": 0.5, "value": 1.0})),
        ("set_camera", json!({"cam_fov": 3.0})), ("set_camera", json!({"cam_x": "a"})),
        ("set_track", json!({"track_id": "nope", "locked": true})), ("remove_silence_cuts", json!({"track_id": "nope", "ranges": [[1.0, 2.0]]})),
        ("loop_clip", json!({"clip_id": c, "times": 0})), ("loop_clip", json!({"clip_id": c, "times": 1, "gap": -1.0})), ("swap_clips", json!({"clip_a": c, "clip_b": c})),
        ("distribute_clips", json!({"track_id": t1, "gap": -1.0})), ("get_scopes", json!({})), ("render_frame", json!({"time": "x"})),
        ("bind_shared_grade_graphs", json!({"clip_id": c, "graphs": ["missing"]})), ("save_project", json!({})), ("load_project", json!({"path": "/nonexistent/x.kimo"})), ("set_property", json!({"clip_id": c, "property": "x", "value": 1.0, "time": 0.0})), ("delete_clip", json!({"clipid": "x"})),
        ("cache_set_budget", json!({})), ("cache_set_budget", json!({"megabytes": -1.0})),
        ("collect_project", json!({})), ("collect_project", json!({"dest_dir": "/tmp"})), ("collect_project", json!({"dest_dir": "/tmp/kimo_err_row", "copy": "zip"})), ("relink_media", json!({"search_dirs": ["/tmp"], "match": "fuzzy"})),
        ("transcript_import", json!({})), ("transcript_import", json!({"path": "/nonexistent/x.srt"})), ("transcript_get", json!({"transcript_id": "nope"})), ("transcript_coverage", json!({})), ("transcript_to_captions", json!({"track_id": "nope"})), ("validate_captions", json!({})), ("transcript_edit_preview", json!({"edits": [{"op": "wipe", "from_index": 0}]})),
        ("batch", json!({"ops": [{"tool": "undo"}]})), ("batch", json!({"ops": [{"tool": "add_track", "args": {"name": "$zz.x"}}]})), ("batch", json!({"ops": [{"tool": "delete_clip", "args": {"clip_id": c}}, {"tool": "delete_clip", "args": {"clip_id": c}}]})), ("batch", json!({"ops": [{"tool": "add_track"}], "expect_revision": "deadbeef"})),
        ("audio_speech_intervals", json!({"mode":"bogus"})), ("audio_speech_intervals", json!({"aggressiveness":4})), ("audio_speech_intervals", json!({"target":{"clip_id":"missing"}})),
        ("set_keyframes", json!({"clip_id": c, "property": "opacity", "keys": [{"t": 0.0}]})), ("set_keyframes", json!({"clip_id": c, "property": "opacity", "keys": []})), ("set_keyframes", json!({"clip_id": c, "property": "nope", "keys": [{"t": 0.0, "v": 1.0}]})),
        ("sample_property", json!({"clip_id": c, "property": "nope", "times": [0.0]})), ("sample_property", json!({"clip_id": c, "property": "x", "times": []})), ("sample_property", json!({"clip_id": c, "property": "x", "times": [0.0], "space": "zzz"})),
        ("bake_property", json!({"clip_id": c, "property": "nope", "start": 0.0, "end": 1.0, "n": 3})), ("bake_property", json!({"clip_id": c, "property": "x", "start": 0.0, "end": 1.0})), ("bake_property", json!({"clip_id": c, "property": "x", "start": 1.0, "end": 1.0, "fps": 10})), ("bake_property", json!({"clip_id": c, "property": "x", "start": 0.0, "end": 1.0, "fps": 10, "drop_expression": false})),
        ("set_expression", json!({"clip_id": c, "property": "x", "op": "spring", "mass": 0.0})), ("set_expression", json!({"clip_id": c, "property": "x", "op": "bezier", "duration": 0.0})), ("set_expression", json!({"clip_id": c, "property": "x", "op": "spring", "combine": "zzz"})), ("set_expression", json!({"camera": true, "property": "nope", "op": "spring"})),
        ("set_parent", json!({"clip_id": c, "parent_id": c})), ("set_parent", json!({"clip_id": c, "parent_id": "nope"})), ("set_parent", json!({"clip_id": c, "parent_id": null, "mode": "zip"})), ("set_parent", json!({"clip_id": c, "parent_id": null, "inherit": {"time": true}})),
        ("get_transform", json!({"clip_id": c, "space": "zzz"})), ("get_transform", json!({"clip_id": "nope"})), ("add_null", json!({"track_id": "nope", "start": 0.0, "duration": 1.0})),
        ("set_meta", json!({"id": c, "name": "has space"})), ("set_meta", json!({"id": "@nope"})), ("set_meta", json!({"target": "zzz", "id": c})), ("set_meta", json!({"id": c, "tags": ["bad tag"]})), ("set_meta", json!({"target": "track", "id": t1, "role": "x"})),
        ("find", json!({"limit": 0})), ("find", json!({"kind": "zzz"})), ("find", json!({"time_range": [2.0, 1.0]})), ("find", json!({"track": "@missing"})),
        ("project_diff", json!({"a": "undo:0", "b": "current"})), ("project_diff", json!({"a": "nope", "b": "current"})), ("project_diff", json!({"a": "checkpoint:zzz", "b": "current"})), ("project_diff", json!({"a": "current"})),
    ]
}
#[test] fn error_texts_are_byte_identical_to_the_baseline() {
    let (probe, t1, _t2, c) = setup(); drop(probe);
    let calls = calls(&t1, &c);
    let mut lines = vec![];
    for (i, (tool, args)) in calls.iter().enumerate() {
        let (mut ss, _, _, _) = setup(); let line = match dispatch(&mut ss, tool, args) { Ok(_) => "OK (no error)".to_string(), Err(e) => e.replace('\n', "\\n") };
        lines.push(format!("{i:02} {tool} {}\t{line}", serde_json::to_string(args).unwrap()));
    }
    let text = lines.join("\n") + "\n"; let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/error_texts.txt");
    if std::env::var("KIMO_UPDATE_GOLDEN").map(|v| v == "1").unwrap_or(false) { std::fs::write(&p, &text).unwrap(); return; }
    let want = std::fs::read_to_string(&p).expect("run once with KIMO_UPDATE_GOLDEN=1 to record the baseline"); assert_eq!(text, want, "error text changed");
}

#[test] fn every_baseline_row_has_the_expected_code_param_and_hint() {
    let (probe, t1, _t2, c) = setup(); drop(probe); let mut lines = vec![]; let mut other = 0; let mut n = 0;
    for (i, (tool, args)) in calls(&t1, &c).iter().enumerate() {
        let (mut ss, _, _, _) = setup(); let (mut ss2, _, _, _) = setup();
        match dispatch_ex(&mut ss, tool, args) { Ok(_) => lines.push(format!("{i:02} {tool}\tOK")), Err(e) => { n += 1; if e.code == "other" { other += 1; }
            assert_eq!(Some(e.message.clone()), dispatch(&mut ss2, tool, args).err(), "dispatch_ex text must equal dispatch text");
            // the nearest-tools list depends on the whole registry, so every new tool would move this row: pin its shape (prefix and 3 names), not the names
            let hint = if e.code == "unknown_tool" { let h = e.hint.clone().unwrap_or_default(); let names = h.strip_prefix("nearest tools: ").expect("hint starts with 'nearest tools: '"); assert_eq!(names.split(", ").count(), 3, "{h}"); "nearest tools: <3 registry-dependent names>".to_string() } else { e.hint.clone().unwrap_or_default() };
            lines.push(format!("{i:02} {tool}\t{}\tparam={}\thint={}", e.code, e.param.clone().unwrap_or_default(), hint)); } }
    }
    eprintln!("classified as other: {other} of {n}");
    let text = lines.join("\n") + "\n"; let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/error_codes.txt");
    if std::env::var("KIMO_UPDATE_GOLDEN").map(|v| v == "1").unwrap_or(false) { std::fs::write(&p, &text).unwrap(); return; }
    assert_eq!(text, std::fs::read_to_string(&p).expect("record with KIMO_UPDATE_GOLDEN=1"), "codes changed");
}
#[test]fn audio_transients_structured_errors_additive(){let(mut s,t,_t2,c)=setup();for (a,code)in[(json!({"target":c}),"no_audio"),(json!({"target":t,"start":100}),"range_empty"),(json!({"target":"/nonexistent/audio_transients.wav"}),"decode_failed"),(json!({"target":"mix","end":0.01}),"insufficient_data")]{let e=dispatch_ex(&mut s,"audio_transients",&a).unwrap_err();assert_eq!(e.code,code,"{}",e.message);}}
#[test]fn visual_perception_errors_additive(){let(mut s,_,_,_)=setup();for(tool,a,code)in[("diff_frames",json!({"a":{"source":"bad"},"b":{}}),"invalid_source"),("diff_frames",json!({"a":{},"b":{},"metrics":["bad"]}),"invalid_metric"),("diff_frames",json!({"a":{},"b":{},"threshold":2}),"out_of_range"),("detect_cuts",json!({"source":"bad"}),"invalid_source"),("detect_cuts",json!({"method":"bad"}),"invalid_method"),("detect_cuts",json!({"fps":100}),"out_of_range")]{assert_eq!(dispatch_ex(&mut s,tool,&a).unwrap_err().code,code);}}

#[test]fn raw_audio_structured_errors_additive(){let(mut s,_,_,_)=setup();for(tool,a,code)in[("import_samples",json!({"rate":48000,"channels":2,"data":[[null],[0]]}),"invalid_samples"),("audio_filter_response",json!({"coeffs":[]}),"out_of_range"),("read_samples",json!({"rate":44100}),"out_of_range")]{let e=dispatch_ex(&mut s,tool,&a).unwrap_err();assert_eq!(e.code,code,"{}",e.message);}}

/// U2/RV-28/U1 prefix codes are typed (they used to fall through to "other"): the text is unchanged, only the code is new.
/// name_taken and ambiguous_name need state (two named things), which the stateless baseline rows cannot express: pinned here by exact code and text prefix.
#[test]fn name_taken_and_ambiguous_name_are_typed_and_text_stable(){let(mut s,t1,t2,c)=setup();
    dispatch(&mut s,"set_meta",&json!({"target":"track","id":t1,"name":"main"})).unwrap();
    let e=dispatch_ex(&mut s,"set_meta",&json!({"target":"track","id":t2,"name":"main"})).unwrap_err();assert_eq!(e.code,"name_taken");assert_eq!(e.message,"name_taken: 'main' is already used by another track");
    let c2=dispatch(&mut s,"add_shape",&json!({"track_id":t2,"shape":"rect","width":10,"height":10,"start":0.0,"duration":1.0})).unwrap()["clip_id"].as_str().unwrap().to_string();
    for id in [&c,&c2]{for tr in s.project.tracks.iter_mut(){for cl in tr.clips.iter_mut(){if &cl.id==id{std::sync::Arc::make_mut(cl).name=Some("dup".into());}}}}
    let e=dispatch_ex(&mut s,"set_property",&json!({"clip_id":"@dup","property":"x","value":1.0})).unwrap_err();assert_eq!(e.code,"ambiguous_name");assert!(e.message.starts_with("ambiguous_name: 2 clips are named 'dup' ("),"{}",e.message);}

#[test]fn batch_names_and_layout_prefix_codes_are_typed(){let(mut s,_,_,_)=setup();
    let one=|f:serde_json::Value|json!({"expand_only":true,"ops":[f]});
    for(tool,a,code)in[
        ("batch",one(json!({"tool":"find","args":{"limit":{"$calc":"1/0"}}})),"calc_error"),
        ("batch",one(json!({"foreach":{"var":"a","in":{"range":[0,600]},"max":1000},"ops":[{"tool":"find","args":{}}]})),"batch_expansion_too_large"),
        ("batch",one(json!({"foreach":{"var":"a","in":[1]},"ops":[{"foreach":{"var":"b","in":[1]},"ops":[{"foreach":{"var":"d","in":[1]},"ops":[{"foreach":{"var":"e","in":[1]},"ops":[{"tool":"find","args":{}}]}]}]}]})),"invalid_foreach"),
        ("set_meta",json!({"id":"@nope"}),"name_not_found"),
        ("set_meta",json!({"id":"clip3","name":"has space"}),"invalid_name")]{let e=dispatch_ex(&mut s,tool,&a).unwrap_err();assert_eq!(e.code,code,"{}",e.message);}}
