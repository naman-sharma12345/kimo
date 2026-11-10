//! v7 additions: mark_scene_cuts -- one call drops a timeline marker at every detected scene cut.
//! Fixture style matches scene_split.rs (a generated red/green/blue/white 1s-per-shot video).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn e(s: &mut Session, n: &str, a: Value) -> String { dispatch(s, n, &a).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn shots_file(tag: &str, cols: &[&str]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("kimo-mark-cuts-{tag}-{}.mp4", std::process::id()));
    let mut args: Vec<String> = vec!["-v".into(), "error".into(), "-y".into()];
    for col in cols { args.extend(["-f", "lavfi", "-i"].iter().map(|x| x.to_string())); args.push(format!("color=c={col}:size=64x48:rate=24:duration=1")); }
    let ins: String = (0..cols.len()).map(|i| format!("[{i}:v]")).collect();
    args.extend(["-filter_complex".to_string(), format!("{ins}concat=n={}:v=1:a=0[v]", cols.len()), "-map".into(), "[v]".into(), "-c:v".into(), "mpeg4".into(), "-q:v".into(), "2".into(), path.to_str().unwrap().into()]);
    assert!(std::process::Command::new("ffmpeg").args(&args).status().unwrap().success());
    path
}
fn setup(tag: &str, cols: &[&str]) -> (Session, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 48, "fps": 24}));
    let asset = c(&mut s, "import_media", json!({"path": shots_file(tag, cols)}))["asset_id"].clone();
    let tr = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let clip = c(&mut s, "add_clip", json!({"track_id": tr, "source_type": "media", "asset_id": asset, "start": 2.0, "duration": cols.len() as f64, "fit": "stretch"}))["clip_id"].as_str().unwrap().to_string();
    (s, clip)
}
fn marker_times(s: &Session) -> Vec<f64> { s.project.markers.iter().map(|m| m.time).collect() }

#[test] fn drops_one_marker_per_cut_named_in_order() {
    let (mut s, clip) = setup("a", &["red", "green", "blue", "white"]);
    let r = c(&mut s, "mark_scene_cuts", json!({"source": format!("clip:{clip}")}));
    assert_eq!(r["created"].as_array().unwrap().len(), 3, "4 shots -> 3 internal cuts: {r}");
    let times = marker_times(&s); assert_eq!(times.len(), 3);
    let names: Vec<String> = s.project.markers.iter().map(|m| m.name.clone()).collect();
    assert_eq!(names, vec!["cut 1", "cut 2", "cut 3"]);
    for m in &s.project.markers { assert_eq!(m.color, "red"); }
    // cuts land on the shot boundaries (2.0 + 1, +2, +3), within a couple of frames
    let mut sorted = times.clone(); sorted.sort_by(|a, b| a.total_cmp(b));
    for (i, t) in sorted.iter().enumerate() { assert!((t - (3.0 + i as f64)).abs() <= 2.0/24.0, "{sorted:?}"); }
}

#[test] fn dry_run_changes_nothing_and_custom_prefix_and_color_apply() {
    let (mut s, clip) = setup("b", &["red", "green", "blue"]);
    let r = c(&mut s, "mark_scene_cuts", json!({"source": format!("clip:{clip}"), "dry_run": true}));
    assert_eq!(r["would_place"], 2); assert_eq!(s.project.markers.len(), 0, "dry_run creates no markers");
    let r2 = c(&mut s, "mark_scene_cuts", json!({"source": format!("clip:{clip}"), "name_prefix": "scene", "color": "yellow"}));
    assert_eq!(r2["created"].as_array().unwrap().len(), 2);
    assert_eq!(s.project.markers[0].name, "scene 1"); assert_eq!(s.project.markers[0].color, "yellow");
}

#[test] fn skip_existing_avoids_duplicate_markers_on_a_second_call() {
    let (mut s, clip) = setup("c", &["red", "green", "blue"]);
    c(&mut s, "mark_scene_cuts", json!({"source": format!("clip:{clip}")}));
    assert_eq!(s.project.markers.len(), 2);
    let r2 = c(&mut s, "mark_scene_cuts", json!({"source": format!("clip:{clip}")}));
    assert_eq!(r2["created"].as_array().unwrap().len(), 0, "every cut already has a marker: {r2}");
    assert_eq!(r2["skipped_existing"], 2);
    assert_eq!(s.project.markers.len(), 2, "no duplicates piled up");
    // skip_existing:false forces fresh markers even on the same cuts
    let r3 = c(&mut s, "mark_scene_cuts", json!({"source": format!("clip:{clip}"), "skip_existing": false}));
    assert_eq!(r3["created"].as_array().unwrap().len(), 2);
    assert_eq!(s.project.markers.len(), 4);
}

#[test] fn a_single_shot_places_nothing_and_bad_calls_are_typed_errors() {
    let (mut s, clip) = setup("d", &["red", "red"]);
    let r = c(&mut s, "mark_scene_cuts", json!({"source": format!("clip:{clip}")}));
    assert_eq!(r["created"].as_array().unwrap().len(), 0, "no cut detected -> no markers: {r}");
    assert_eq!(s.project.markers.len(), 0);
    assert!(e(&mut s, "mark_scene_cuts", json!({"sensitivity": 2.0})).contains("sensitivity"));
    assert!(e(&mut s, "mark_scene_cuts", json!({"source": "clip:nope"})).len() > 0);
}
