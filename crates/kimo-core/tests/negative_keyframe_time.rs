//! Showcase finding: keyframe tools silently accepted negative clip-local times. They must reject them with code out_of_range, param "time", and leave the project unchanged.
use kimo_core::commands::{dispatch, dispatch_ex, Session};
use serde_json::json;
const TOOLS: [&str; 8] = ["set_keyframe", "set_keyframe_bezier", "set_mask_keyframe", "set_mask_prop_key", "set_mask_path_key", "set_animator_keyframe", "set_camera_keyframe", "set_audio_volume_key"];
#[test] fn set_keyframe_rejects_negative_time_and_keeps_project() {
    let mut s = Session::new(); let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    let c = dispatch(&mut s, "add_clip", &json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.3, 0.5], "start": 0, "duration": 2})).unwrap()["clip_id"].clone();
    let before = serde_json::to_value(&s.project).unwrap();
    let e = dispatch_ex(&mut s, "set_keyframe", &json!({"clip_id": c, "property": "opacity", "time": -0.5, "value": 1.0})).expect_err("negative time must be rejected");
    assert_eq!(e.code, "out_of_range"); assert_eq!(e.param.as_deref(), Some("time")); assert!(e.message.contains("time must be >= 0"), "{}", e.message);
    assert_eq!(before, serde_json::to_value(&s.project).unwrap(), "a rejected call must not change the project");
    dispatch(&mut s, "set_keyframe", &json!({"clip_id": c, "property": "opacity", "time": 0.0, "value": 1.0})).expect("time 0 stays valid");
}
#[test] fn every_keyframe_tool_rejects_negative_time() {
    let mut s = Session::new();
    for tool in TOOLS { let e = dispatch_ex(&mut s, tool, &json!({"clip_id": "x", "property": "opacity", "time": -1.0, "value": 1.0})).expect_err(tool); assert_eq!((e.code, e.param.as_deref()), ("out_of_range", Some("time")), "{tool}: {}", e.message); }
}
