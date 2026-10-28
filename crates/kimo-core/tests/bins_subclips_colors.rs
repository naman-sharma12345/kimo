//! Subclips, clip colour labels, bins, metadata and search (FEATURES.md L85).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn err(s: &mut Session, n: &str, a: Value) -> String { dispatch(s, n, &a).unwrap_err() }

fn fixture(name: &str, dur: f64) -> String {
    let p = format!("/tmp/kimo-binsub-{}-{name}.mp4", std::process::id());
    assert!(std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("testsrc=size=64x48:rate=25:duration={dur}"), "-pix_fmt", "yuv420p", &p])
        .status().unwrap().success());
    p
}

// ---------- bins ----------

#[test]
fn create_list_rename_delete_bin_round_trip() {
    let mut s = Session::new();
    c(&mut s, "create_bin", json!({"name": "B-roll"}));
    assert!(err(&mut s, "create_bin", json!({"name": "B-roll"})).contains("bin_exists"));
    let bins = c(&mut s, "list_bins", json!({}))["bins"].as_array().unwrap().clone();
    assert_eq!(bins.len(), 1);
    assert_eq!(bins[0]["name"], "B-roll");
    assert_eq!(bins[0]["assets"], 0);

    c(&mut s, "rename_bin", json!({"name": "B-roll", "new_name": "Archive"}));
    assert!(err(&mut s, "rename_bin", json!({"name": "B-roll", "new_name": "x"})).contains("no bin"));
    let bins = c(&mut s, "list_bins", json!({}))["bins"].as_array().unwrap().clone();
    assert_eq!(bins[0]["name"], "Archive");

    c(&mut s, "delete_bin", json!({"name": "Archive"}));
    assert_eq!(c(&mut s, "list_bins", json!({}))["bins"].as_array().unwrap().len(), 0);
    assert!(err(&mut s, "delete_bin", json!({"name": "Archive"})).contains("no bin"));
}

#[test]
fn move_asset_to_bin_and_find_assets() {
    let mut s = Session::new();
    let p1 = fixture("a", 1.0);
    let p2 = fixture("b", 1.0);
    let a1 = c(&mut s, "import_media", json!({"path": p1}))["asset_id"].as_str().unwrap().to_string();
    let a2 = c(&mut s, "import_media", json!({"path": p2}))["asset_id"].as_str().unwrap().to_string();

    // moving into a bin that doesn't exist yet creates it
    c(&mut s, "move_asset_to_bin", json!({"asset_id": a1, "bin": "Interviews"}));
    let bins = c(&mut s, "list_bins", json!({}))["bins"].as_array().unwrap().clone();
    assert_eq!(bins.len(), 1);
    assert_eq!(bins[0]["name"], "Interviews");
    assert_eq!(bins[0]["assets"], 1);

    let unfiled = c(&mut s, "find_assets", json!({"bin": Value::Null}))["matches"].as_array().unwrap().clone();
    assert_eq!(unfiled.len(), 1);
    assert_eq!(unfiled[0]["asset_id"], a2);

    let filed = c(&mut s, "find_assets", json!({"bin": "Interviews"}))["matches"].as_array().unwrap().clone();
    assert_eq!(filed.len(), 1);
    assert_eq!(filed[0]["asset_id"], a1);

    // clear back to unfiled
    c(&mut s, "move_asset_to_bin", json!({"asset_id": a1, "bin": Value::Null}));
    assert_eq!(c(&mut s, "find_assets", json!({"bin": Value::Null}))["matches"].as_array().unwrap().len(), 2);

    assert!(err(&mut s, "move_asset_to_bin", json!({"asset_id": "nope", "bin": "X"})).contains("no asset"));
    std::fs::remove_file(&p1).ok();
    std::fs::remove_file(&p2).ok();
}

#[test]
fn delete_bin_reassigns_its_assets() {
    let mut s = Session::new();
    let p = fixture("c", 1.0);
    let a = c(&mut s, "import_media", json!({"path": p}))["asset_id"].as_str().unwrap().to_string();
    c(&mut s, "move_asset_to_bin", json!({"asset_id": a, "bin": "Old"}));
    c(&mut s, "delete_bin", json!({"name": "Old", "reassign_to": "New"}));
    let bins = c(&mut s, "list_bins", json!({}))["bins"].as_array().unwrap().clone();
    assert_eq!(bins.len(), 1);
    assert_eq!(bins[0]["name"], "New");
    assert_eq!(bins[0]["assets"], 1);
    std::fs::remove_file(&p).ok();
}

// ---------- subclips ----------

#[test]
fn create_subclip_validates_range_against_asset_duration() {
    let mut s = Session::new();
    let p = fixture("d", 4.0);
    let a = c(&mut s, "import_media", json!({"path": p}))["asset_id"].as_str().unwrap().to_string();
    assert!(err(&mut s, "create_subclip", json!({"asset_id": a, "in_point": -1.0, "out_point": 1.0})).contains("in_point"));
    assert!(err(&mut s, "create_subclip", json!({"asset_id": a, "in_point": 2.0, "out_point": 1.0})).contains("out_point"));
    assert!(err(&mut s, "create_subclip", json!({"asset_id": a, "in_point": 0.0, "out_point": 999.0})).contains("exceeds"));
    assert!(err(&mut s, "create_subclip", json!({"asset_id": "nope", "in_point": 0.0, "out_point": 1.0})).contains("no asset"));
    std::fs::remove_file(&p).ok();
}

#[test]
fn create_find_list_delete_subclip_and_place_it_on_the_timeline() {
    let mut s = Session::new();
    let p = fixture("e", 4.0);
    let a = c(&mut s, "import_media", json!({"path": p}))["asset_id"].as_str().unwrap().to_string();
    let sc = c(&mut s, "create_subclip", json!({"asset_id": a, "in_point": 1.0, "out_point": 3.0, "name": "good-take"}));
    let subclip_id = sc["subclip_id"].as_str().unwrap().to_string();
    assert_eq!(sc["in_point"], 1.0);
    assert_eq!(sc["out_point"], 3.0);

    let subs = c(&mut s, "list_subclips", json!({"asset_id": a}))["subclips"].as_array().unwrap().clone();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0]["name"], "good-take");

    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let placed = c(&mut s, "add_clip_from_subclip", json!({"subclip_id": subclip_id, "track_id": t, "start": 5.0}));
    let clip_id = placed["clip_id"].as_str().unwrap().to_string();
    let clip = s.project.find_clip(&clip_id).unwrap();
    assert_eq!(clip.in_point, 1.0);
    assert_eq!(clip.duration, 2.0); // full span: out - in
    assert_eq!(clip.start, 5.0);

    // placing with a shorter duration trims from in_point; longer than the span is rejected
    let placed2 = c(&mut s, "add_clip_from_subclip", json!({"subclip_id": subclip_id, "track_id": t, "start": 10.0, "duration": 1.0}));
    let clip2 = s.project.find_clip(placed2["clip_id"].as_str().unwrap()).unwrap();
    assert_eq!(clip2.duration, 1.0);
    assert_eq!(clip2.in_point, 1.0);
    assert!(err(&mut s, "add_clip_from_subclip", json!({"subclip_id": subclip_id, "track_id": t, "start": 20.0, "duration": 99.0})).contains("exceeds"));

    c(&mut s, "delete_subclip", json!({"subclip_id": subclip_id}));
    assert!(err(&mut s, "delete_subclip", json!({"subclip_id": subclip_id})).contains("no subclip"));
    // the already-placed clip is unaffected by deleting the bookmark
    assert!(s.project.find_clip(&clip_id).is_some());
    std::fs::remove_file(&p).ok();
}

#[test]
fn subclip_placed_clip_is_byte_identical_to_a_hand_placed_one() {
    let mut s1 = Session::new();
    let p = fixture("f", 4.0);
    let a = c(&mut s1, "import_media", json!({"path": p.clone()}))["asset_id"].as_str().unwrap().to_string();
    let sc = c(&mut s1, "create_subclip", json!({"asset_id": a, "in_point": 0.5, "out_point": 2.5}))["subclip_id"].as_str().unwrap().to_string();
    let t1 = c(&mut s1, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let via_sub = c(&mut s1, "add_clip_from_subclip", json!({"subclip_id": sc, "track_id": t1, "start": 0.0}));
    let clip_a = s1.project.find_clip(via_sub["clip_id"].as_str().unwrap()).unwrap().clone();

    let mut s2 = Session::new();
    let a2 = c(&mut s2, "import_media", json!({"path": p.clone()}))["asset_id"].as_str().unwrap().to_string();
    let t2 = c(&mut s2, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let by_hand = c(&mut s2, "add_clip", json!({"track_id": t2, "source_type": "media", "asset_id": a2, "start": 0.0, "duration": 2.0, "in_point": 0.5}));
    let clip_b = s2.project.find_clip(by_hand["clip_id"].as_str().unwrap()).unwrap().clone();

    assert_eq!(clip_a.in_point, clip_b.in_point);
    assert_eq!(clip_a.duration, clip_b.duration);
    assert_eq!(clip_a.start, clip_b.start);
    assert_eq!(serde_json::to_value(&clip_a.props).unwrap(), serde_json::to_value(&clip_b.props).unwrap());
    assert_eq!(clip_a.audio.volume.base, clip_b.audio.volume.base);
    std::fs::remove_file(&p).ok();
}

// ---------- clip colour labels ----------

#[test]
fn set_meta_color_validates_and_clears() {
    let mut s = Session::new();
    let t = c(&mut s, "add_track", json!({}));
    let tid = t["track_id"].as_str().unwrap().to_string();
    let cl = c(&mut s, "add_clip", json!({"track_id": tid, "source_type": "solid", "color": [0.5,0.5,0.5], "start": 0.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();

    assert!(err(&mut s, "set_meta", json!({"id": cl, "color": "chartreuse"})).contains("invalid_value"));

    let r = c(&mut s, "set_meta", json!({"id": cl, "color": "red"}));
    assert_eq!(r["color"], "red");
    let clip = s.project.find_clip(&cl).unwrap();
    assert!(clip.tags.contains(&"color:red".to_string()));

    // changing color replaces, doesn't stack
    c(&mut s, "set_meta", json!({"id": cl, "color": "blue"}));
    let clip = s.project.find_clip(&cl).unwrap();
    assert_eq!(clip.tags.iter().filter(|t| t.starts_with("color:")).count(), 1);
    assert!(clip.tags.contains(&"color:blue".to_string()));

    // clearing
    let r = c(&mut s, "set_meta", json!({"id": cl, "color": Value::Null}));
    assert_eq!(r["color"], Value::Null);
    let clip = s.project.find_clip(&cl).unwrap();
    assert!(!clip.tags.iter().any(|t| t.starts_with("color:")));

    // a plain custom tag with a different prefix is untouched by color management
    c(&mut s, "set_meta", json!({"id": cl, "add_tags": ["lint:allow_bleed"]}));
    c(&mut s, "set_meta", json!({"id": cl, "color": "green"}));
    let clip = s.project.find_clip(&cl).unwrap();
    assert!(clip.tags.contains(&"lint:allow_bleed".to_string()));
    assert!(clip.tags.contains(&"color:green".to_string()));
}

#[test]
fn find_filters_and_reports_clip_color() {
    let mut s = Session::new();
    let tid = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let red = c(&mut s, "add_clip", json!({"track_id": tid, "source_type": "solid", "color": [1.0,0.0,0.0], "start": 0.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    let blue = c(&mut s, "add_clip", json!({"track_id": tid, "source_type": "solid", "color": [0.0,0.0,1.0], "start": 2.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    c(&mut s, "set_meta", json!({"id": red, "color": "red"}));
    c(&mut s, "set_meta", json!({"id": blue, "color": "blue"}));

    let reds = c(&mut s, "find", json!({"color": "red"}))["matches"].as_array().unwrap().clone();
    assert_eq!(reds.len(), 1);
    assert_eq!(reds[0]["id"], red);
    assert_eq!(reds[0]["color"], "red");

    let all = c(&mut s, "find", json!({}))["matches"].as_array().unwrap().clone();
    assert_eq!(all.len(), 2);
    assert!(err(&mut s, "find", json!({"color": "not_a_color"})).contains("invalid_value"));
}
