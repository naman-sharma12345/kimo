//! v7.5 feature: import_clips -- copy specific clips, with their full effect/keyframe/mask/grade state
//! (a clip serializes all of it directly), from ANOTHER project revision into the live project at a chosen
//! track and time. project_merge already reconciles two revisions of the SAME lineage by clip id
//! (add/remove/change); nothing today does a direct "grab these specific clips out of that other project
//! file and drop them into this one", independent of whether the two projects share any history at all.
//! import_clips reuses project_diff's own resolve_rev (same source forms: current | undo:<n> |
//! checkpoint:<name> | file:<path.kimo>) rather than re-implementing project loading.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn try_c(ss: &mut Session, n: &str, v: Value) -> Result<Value, String> { dispatch(ss, n, &v) }

fn new_proj() -> Session {
    let mut ss = Session::new();
    c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    ss
}
fn tmp_path(name: &str) -> String {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("kimo-importclips-{name}-{}-{n}.kimo", std::process::id())).to_str().unwrap().to_string()
}

/// A standalone source project file with one shape clip (with an effect) on one track. Returns (path, clip_id).
fn source_project_with_a_shape_clip() -> (String, String) {
    let mut src = new_proj();
    let t = c(&mut src, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let clip = c(&mut src, "add_shape", json!({"track_id": t, "shape": "rect", "start": 2.0, "duration": 3.0, "color": [0.8, 0.2, 0.1, 1.0]}))["clip_id"].as_str().unwrap().to_string();
    let eid = c(&mut src, "add_effect", json!({"clip_id": clip, "kind": "blur"}))["effect_id"].as_str().unwrap().to_string();
    c(&mut src, "set_effect_param", json!({"clip_id": clip, "effect_id": eid, "param": "radius", "value": 5.0}));
    c(&mut src, "set_keyframe", json!({"clip_id": clip, "property": "opacity", "time": 0.5, "value": 0.3}));
    let path = tmp_path("shape");
    c(&mut src, "save_project", json!({"path": path}));
    (path, clip)
}

#[test]
fn imports_a_clip_with_its_effect_and_keyframe_onto_a_new_track() {
    let (path, clip_id) = source_project_with_a_shape_clip();
    let mut ss = new_proj();
    let r = c(&mut ss, "import_clips", json!({"source": format!("file:{path}"), "clip_ids": [clip_id]}));
    assert_eq!(r["imported"].as_array().unwrap().len(), 1, "{r}");
    let new_id = r["imported"][0]["clip_id"].as_str().unwrap().to_string();
    let newc = ss.project.find_clip(&new_id).expect("imported clip should exist in the live project");
    assert_eq!(newc.start, 2.0, "with no time_offset, the clip keeps its original position");
    assert!(!newc.effects.is_empty(), "the blur effect should travel with the clip");
    assert!(newc.props.get("opacity").map(|p| p.keys.len() > 0).unwrap_or(false), "the opacity keyframe should travel with the clip");
}

#[test]
fn time_offset_shifts_every_imported_clip() {
    let (path, clip_id) = source_project_with_a_shape_clip();
    let mut ss = new_proj();
    let r = c(&mut ss, "import_clips", json!({"source": format!("file:{path}"), "clip_ids": [clip_id], "time_offset": 10.0}));
    let new_id = r["imported"][0]["clip_id"].as_str().unwrap().to_string();
    let newc = ss.project.find_clip(&new_id).unwrap();
    assert_eq!(newc.start, 12.0, "start should be the original 2.0 plus the 10.0 offset");
}

#[test]
fn importing_onto_an_existing_track_does_not_create_a_new_one() {
    let (path, clip_id) = source_project_with_a_shape_clip();
    let mut ss = new_proj();
    let dest_track = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let before_tracks = ss.project.tracks.len();
    let r = c(&mut ss, "import_clips", json!({"source": format!("file:{path}"), "clip_ids": [clip_id], "track_id": dest_track}));
    assert_eq!(ss.project.tracks.len(), before_tracks, "no new track should be created when track_id is given");
    assert_eq!(r["track_id"], json!(dest_track));
}

#[test]
fn importing_without_track_id_creates_a_new_track() {
    let (path, clip_id) = source_project_with_a_shape_clip();
    let mut ss = new_proj();
    let before_tracks = ss.project.tracks.len();
    c(&mut ss, "import_clips", json!({"source": format!("file:{path}"), "clip_ids": [clip_id]}));
    assert_eq!(ss.project.tracks.len(), before_tracks + 1, "a destination track should be created when track_id is omitted");
}

#[test]
fn a_media_clips_asset_is_imported_along_with_it() {
    let mut src = new_proj();
    let wav_path = std::env::temp_dir().join(format!("kimo-importclips-tone-{}.wav", std::process::id()));
    let status = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=1"]).arg(&wav_path).status().unwrap();
    assert!(status.success());
    let asset_id = c(&mut src, "import_media", json!({"path": wav_path.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let r = c(&mut src, "add_audio_clip", json!({"asset_id": asset_id, "start": 0.0, "duration": 1.0}));
    let clip_id = r["clip_id"].as_str().unwrap().to_string();
    let path = tmp_path("media");
    c(&mut src, "save_project", json!({"path": path}));

    let mut ss = new_proj();
    assert!(ss.project.assets.is_empty(), "sanity: the destination project starts with no assets");
    let result = c(&mut ss, "import_clips", json!({"source": format!("file:{path}"), "clip_ids": [clip_id]}));
    assert_eq!(result["assets_imported"], 1, "{result}");
    let new_id = result["imported"][0]["clip_id"].as_str().unwrap().to_string();
    let newc = ss.project.find_clip(&new_id).unwrap();
    let kimo_core::model::Source::Media { asset } = &newc.source else { panic!("expected a media source") };
    assert!(ss.project.assets.contains_key(asset), "the clip's asset id must resolve inside the destination project's own asset pool");
    assert_eq!(ss.project.assets[asset].path, wav_path.to_str().unwrap());
}

#[test]
fn importing_the_same_media_clip_twice_reuses_the_already_imported_asset() {
    let mut src = new_proj();
    let wav_path = std::env::temp_dir().join(format!("kimo-importclips-tone2-{}.wav", std::process::id()));
    let status = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=330:duration=1"]).arg(&wav_path).status().unwrap();
    assert!(status.success());
    let asset_id = c(&mut src, "import_media", json!({"path": wav_path.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let r1 = c(&mut src, "add_audio_clip", json!({"asset_id": asset_id.clone(), "start": 0.0, "duration": 1.0}));
    let r2 = c(&mut src, "add_audio_clip", json!({"asset_id": asset_id, "start": 1.0, "duration": 1.0}));
    let (c1, c2) = (r1["clip_id"].as_str().unwrap().to_string(), r2["clip_id"].as_str().unwrap().to_string());
    let path = tmp_path("media2");
    c(&mut src, "save_project", json!({"path": path}));

    let mut ss = new_proj();
    let result = c(&mut ss, "import_clips", json!({"source": format!("file:{path}"), "clip_ids": [c1, c2]}));
    assert_eq!(result["assets_imported"], 1, "{result}: both clips share one source asset, so only one new asset should be created");
    assert_eq!(result["assets_reused"], 1, "{result}: the second clip should reuse the asset the first import just created");
    assert_eq!(ss.project.assets.len(), 1, "exactly one asset should exist in the destination project");
}

#[test]
fn importing_a_media_clip_reuses_an_asset_already_present_by_path_in_the_destination() {
    let wav_path = std::env::temp_dir().join(format!("kimo-importclips-tone3-{}.wav", std::process::id()));
    let status = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=550:duration=1"]).arg(&wav_path).status().unwrap();
    assert!(status.success());

    let mut src = new_proj();
    let src_asset = c(&mut src, "import_media", json!({"path": wav_path.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let clip_id = c(&mut src, "add_audio_clip", json!({"asset_id": src_asset, "start": 0.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    let path = tmp_path("media3");
    c(&mut src, "save_project", json!({"path": path}));

    let mut ss = new_proj();
    // The destination project ALREADY imported the same media file under its own asset id.
    c(&mut ss, "import_media", json!({"path": wav_path.to_str().unwrap()}));
    let result = c(&mut ss, "import_clips", json!({"source": format!("file:{path}"), "clip_ids": [clip_id]}));
    assert_eq!(result["assets_imported"], 0, "{result}: the destination already has this exact media file imported, by path");
    assert_eq!(result["assets_reused"], 1, "{result}");
    assert_eq!(ss.project.assets.len(), 1, "no duplicate asset should be created for the same file path");
}

#[test]
fn errors_on_an_unknown_clip_id_in_the_source() {
    let (path, _clip_id) = source_project_with_a_shape_clip();
    let mut ss = new_proj();
    let err = try_c(&mut ss, "import_clips", json!({"source": format!("file:{path}"), "clip_ids": ["nope"]})).unwrap_err();
    assert!(err.contains("nope"), "{err}");
}

#[test]
fn errors_on_a_missing_source_file() {
    let mut ss = new_proj();
    let err = try_c(&mut ss, "import_clips", json!({"source": "file:/tmp/does-not-exist-kimo-xyz.kimo", "clip_ids": ["c1"]})).unwrap_err();
    assert!(err.to_lowercase().contains("cannot read") || err.to_lowercase().contains("no such file"), "{err}");
}

#[test]
fn errors_on_an_unknown_destination_track() {
    let (path, clip_id) = source_project_with_a_shape_clip();
    let mut ss = new_proj();
    let err = try_c(&mut ss, "import_clips", json!({"source": format!("file:{path}"), "clip_ids": [clip_id], "track_id": "nope"})).unwrap_err();
    assert!(err.to_lowercase().contains("track"), "{err}");
}

#[test]
fn import_clips_is_undoable_in_one_step() {
    let (path, clip_id) = source_project_with_a_shape_clip();
    let mut ss = new_proj();
    let before_tracks = ss.project.tracks.len();
    c(&mut ss, "import_clips", json!({"source": format!("file:{path}"), "clip_ids": [clip_id]}));
    assert_eq!(ss.project.tracks.len(), before_tracks + 1);
    c(&mut ss, "undo", json!({}));
    assert_eq!(ss.project.tracks.len(), before_tracks, "undo should remove the whole import in one step");
}

#[test]
fn imports_multiple_clips_in_one_call() {
    let mut src = new_proj();
    let t = c(&mut src, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c1 = c(&mut src, "add_shape", json!({"track_id": t, "shape": "rect", "start": 0.0, "duration": 1.0, "color": [1.0, 0.0, 0.0, 1.0]}))["clip_id"].as_str().unwrap().to_string();
    let c2 = c(&mut src, "add_shape", json!({"track_id": t, "shape": "ellipse", "start": 2.0, "duration": 1.0, "color": [0.0, 1.0, 0.0, 1.0]}))["clip_id"].as_str().unwrap().to_string();
    let path = tmp_path("multi");
    c(&mut src, "save_project", json!({"path": path}));

    let mut ss = new_proj();
    let r = c(&mut ss, "import_clips", json!({"source": format!("file:{path}"), "clip_ids": [c1, c2]}));
    assert_eq!(r["imported"].as_array().unwrap().len(), 2, "{r}");
    let dest_track = r["track_id"].as_str().unwrap().to_string();
    let tr = ss.project.tracks.iter().find(|t| t.id == dest_track).unwrap();
    assert_eq!(tr.clips.len(), 2, "both imported clips should land on the one destination track");
}

#[test]
fn a_destination_clip_already_using_the_source_clips_id_is_not_overwritten() {
    let (path, clip_id) = source_project_with_a_shape_clip();
    let mut ss = new_proj();
    // Force a real id collision: give the destination a clip whose id is EXACTLY the source clip's id,
    // something two independently-built project files can do by coincidence (both start their own id
    // counter at "clip1", "clip2", ...).
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let existing = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "start": 50.0, "duration": 1.0, "color": [0.0, 0.0, 0.0, 1.0]}))["clip_id"].as_str().unwrap().to_string();
    // Can't force add_shape to hand out a specific id, so fabricate the collision directly on the model.
    { let cl = ss.project.find_clip_mut(&existing).unwrap(); cl.id = clip_id.clone(); }
    assert_eq!(ss.project.find_clip(&clip_id).unwrap().start, 50.0, "sanity: the destination's own clip now holds the colliding id");

    let r = c(&mut ss, "import_clips", json!({"source": format!("file:{path}"), "clip_ids": [clip_id.clone()]}));
    let new_id = r["imported"][0]["clip_id"].as_str().unwrap().to_string();
    assert_ne!(new_id, clip_id, "the newly imported clip must not collide with the id already in use in the destination");
    // Both clips must still exist, independently.
    assert_eq!(ss.project.find_clip(&clip_id).unwrap().start, 50.0, "the destination's pre-existing clip must be untouched");
    let imported_clip = ss.project.find_clip(&new_id).expect("the imported clip should exist under its own fresh id");
    assert_eq!(imported_clip.start, 2.0, "the imported clip should carry the source's own position");
}
