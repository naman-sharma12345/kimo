//! Director-mode open items before the Oct 26 hackathon (DIRECTOR_DRYRUN items 11-12, SMALL_MODEL_PREP): each test was written
//! red against the behaviour a director agent or the demo machine hit.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;

fn dir() -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_director_gaps_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}")) }
fn sid(v: &Value, k: &str) -> String { v[k].as_str().unwrap_or_else(|| panic!("no {k} in {v}")).to_string() }

/// DIRECTOR_DRYRUN item 11: export returned only path and duration, so an agent delivering to a 16 MB cap had no way to
/// check the file it just wrote. The result now carries the file's size in bytes beside its duration.
#[test] fn export_reports_file_size_and_duration() {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 360, "height": 640, "fps": 30}));
    let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.4, 0.6], "start": 0, "duration": 1.5}));
    let out = dir().join("size.mp4"); let r = call(&mut ss, "export", json!({"path": out, "crf": 30}));
    let on_disk = std::fs::metadata(&out).unwrap().len();
    assert!(on_disk > 0);
    assert_eq!(r["bytes"].as_u64(), Some(on_disk), "export must report the written file's size in bytes: {r}");
    assert!((r["duration"].as_f64().unwrap() - 1.5).abs() < 0.05, "duration: {r}");
}

fn off_safe(v: &Value) -> Vec<Value> { let mut o: Vec<Value> = v["groups"].as_array().unwrap().iter().flat_map(|g| g["findings"].as_array().unwrap().clone()).collect();
    if let Some(t) = v.get("titles") { o.extend(t["findings"].as_array().unwrap().clone()); } o.retain(|f| f["code"] == "OFF_SAFE_AREA"); o }
fn apply_fix(ss: &mut Session, f: &Value) { let fx = &f["fix"]; call(ss, fx["tool"].as_str().unwrap_or_else(|| panic!("finding has no fix call: {f}")), fx["args"].clone()); }
/// DIRECTOR_DRYRUN item 12: validate_captions listed OFF_SAFE_AREA under skipped, so on 9:16 a caption under the Reels/Shorts
/// buttons (bottom 20%) or the top bar (top 12%) passed as clean. It must flag the cues, say how far to move them, and its fix
/// call must actually clear the finding. Titles are checked too when asked (include_titles).
#[test] fn validate_captions_flags_the_9x16_safe_area() {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 1080, "height": 1920, "fps": 30}));
    let ct = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let cap = sid(&call(&mut ss, "add_caption_cues", json!({"track_id": ct, "style": "boxed", "cues": [{"start": 0.0, "end": 1.6, "text": "three phone clips in"}, {"start": 1.8, "end": 3.4, "text": "one short out"}]})), "caption_id");
    let v = call(&mut ss, "validate_captions", json!({"group_id": cap, "profile": "social_9x16"}));
    assert!(off_safe(&v).is_empty(), "the default 9:16 caption position is inside the safe area: {v}");
    assert!(!v["skipped"].as_array().unwrap().iter().any(|s| s["code"] == "OFF_SAFE_AREA"), "OFF_SAFE_AREA must be checked on 9:16, not skipped: {v}");
    for (y, way) in [(860.0, "up"), (-900.0, "down")] {
        call(&mut ss, "set_caption_style", json!({"caption_id": cap, "style": "boxed", "y": y}));
        let v = call(&mut ss, "validate_captions", json!({"group_id": cap, "profile": "social_9x16"}));
        let f = off_safe(&v); assert_eq!(f.len(), 1, "captions at y {y} sit under the platform UI and must be flagged: {v}");
        assert_eq!(f[0]["severity"], "error"); assert_eq!(f[0]["cue_ids"].as_array().unwrap().len(), 2, "{f:?}");
        let hint = f[0]["fix_hint"].as_str().unwrap(); assert!(hint.contains(way) && hint.contains("set_caption_style"), "hint must say which way to move and how: {hint}");
        assert!(!f[0]["message"].as_str().unwrap().contains("phone"), "findings never quote cue text");
        apply_fix(&mut ss, &f[0]);
        let v = call(&mut ss, "validate_captions", json!({"group_id": cap, "profile": "social_9x16"}));
        assert!(off_safe(&v).is_empty(), "applying the fix must clear the finding (y {y}): {v}");
    }
    // a portrait canvas is checked whatever the profile; a title under the top bar is flagged when titles are included
    let tt = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let title = sid(&call(&mut ss, "add_text", json!({"track_id": tt, "text": "MY FAVS", "start": 0, "duration": 2, "size": 140})), "clip_id");
    call(&mut ss, "set_property", json!({"clip_id": title, "property": "y", "value": -900}));
    let v = call(&mut ss, "validate_captions", json!({"all": true, "include_titles": true}));
    let f = off_safe(&v); assert_eq!(f.len(), 1, "the title under the top bar must be flagged: {v}");
    assert_eq!(f[0]["cue_ids"], json!([title])); assert!(f[0]["fix_hint"].as_str().unwrap().contains("down"));
    assert_eq!(v["clean"], false);
    apply_fix(&mut ss, &f[0]);
    let v = call(&mut ss, "validate_captions", json!({"all": true, "include_titles": true}));
    assert!(off_safe(&v).is_empty(), "the title fix must clear it: {v}");
    // landscape keeps the old contract: the 9:16 band does not apply there
    let mut l = Session::new(); let lt = sid(&call(&mut l, "add_track", json!({})), "track_id");
    let lc = sid(&call(&mut l, "add_caption_cues", json!({"track_id": lt, "cues": [{"start": 0.0, "end": 1.5, "text": "wide frame"}]})), "caption_id");
    let v = call(&mut l, "validate_captions", json!({"group_id": lc}));
    assert!(v["skipped"].as_array().unwrap().iter().any(|s| s["code"] == "OFF_SAFE_AREA"), "{v}");
}
