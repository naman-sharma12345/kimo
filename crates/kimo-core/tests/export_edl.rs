//! v7.4: export_edl -- the write-side counterpart to v7.3's import_edl. Tiny real media fixtures (ffmpeg
//! color clips), no network. Round-trips through import_edl itself where the format supports it (cmx3600,
//! otio) to prove the exported file is not just well-formed but actually faithful.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn tmp(tag: &str) -> std::path::PathBuf { let d = std::env::temp_dir().join(format!("kimo_expedl_{tag}_{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap(); d }
fn make_media(dir: &std::path::Path, name: &str, dur: u32) -> std::path::PathBuf {
    let p = dir.join(name);
    let status = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("color=c=blue:s=32x18:d={dur}:r=25"), "-pix_fmt", "yuv420p"]).arg(&p).status().unwrap();
    assert!(status.success()); p
}

fn two_clip_project(dir: &std::path::Path) -> (Session, String, String, String) {
    let a = make_media(dir, "clip_a.mp4", 10); let b = make_media(dir, "clip_b.mp4", 10);
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let aid = c(&mut ss, "import_media", json!({"path": a.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let bid = c(&mut ss, "import_media", json!({"path": b.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let tr = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": tr, "source_type": "media", "asset_id": aid, "start": 0.0, "duration": 5.0}));
    c(&mut ss, "add_clip", json!({"track_id": tr, "source_type": "media", "asset_id": bid, "start": 5.0, "duration": 3.0, "in_point": 1.0}));
    (ss, tr, aid, bid)
}

#[test] fn cmx3600_export_has_two_events_with_correct_timecodes_and_from_clip_name() {
    let dir = tmp("cmx_basic"); let (mut ss, _tr, _a, _b) = two_clip_project(&dir);
    let r = c(&mut ss, "export_edl", json!({"format": "cmx3600"}));
    assert_eq!(r["events"], 2, "{r}");
    let content = r["content"].as_str().unwrap();
    assert!(content.contains("TITLE:"), "{content}");
    assert!(content.contains("00:00:00:00 00:00:05:00 00:00:00:00 00:00:05:00"), "{content}");
    assert!(content.contains("00:00:01:00 00:00:04:00 00:00:05:00 00:00:08:00"), "{content}");
    assert!(content.contains("* FROM CLIP NAME:") && content.contains("clip_a.mp4") && content.contains("clip_b.mp4"), "{content}");
}

#[test] fn cmx3600_export_round_trips_through_import_edl_dry_run() {
    let dir = tmp("cmx_roundtrip"); let (mut ss, _tr, _a, _b) = two_clip_project(&dir);
    let r = c(&mut ss, "export_edl", json!({"format": "cmx3600"}));
    let edl_path = dir.join("out.edl"); std::fs::write(&edl_path, r["content"].as_str().unwrap()).unwrap();
    let mut fresh = Session::new(); c(&mut fresh, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let back = c(&mut fresh, "import_edl", json!({"path": edl_path.to_str().unwrap(), "dry_run": true}));
    assert_eq!(back["events_total"], 2, "{back}"); assert_eq!(back["unresolved"].as_array().unwrap().len(), 0, "{back}: from-clip-name paths must resolve directly");
}

#[test] fn otio_export_round_trips_through_import_edl() {
    let dir = tmp("otio_roundtrip"); let (mut ss, _tr, _a, _b) = two_clip_project(&dir);
    let r = c(&mut ss, "export_edl", json!({"format": "otio"}));
    assert_eq!(r["format"], "otio");
    let v: Value = serde_json::from_str(r["content"].as_str().unwrap()).expect("export_edl otio content must be valid JSON");
    assert_eq!(v["OTIO_SCHEMA"], "Timeline.1");
    let otio_path = dir.join("out.otio"); std::fs::write(&otio_path, r["content"].as_str().unwrap()).unwrap();
    let mut fresh = Session::new(); c(&mut fresh, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    // export_edl's OTIO "name" field is the human-readable label (clip_a.mp4), same as a real OTIO file's
    // clip name would be; resolving that by filename needs media_dir, exactly like a CMX3600 reel name would
    // (the CMX3600 round trip above resolves without media_dir only because EDL's FROM CLIP NAME comment
    // carries the full path -- OTIO's own "name" field, per import_edl's own documented priority, is tried
    // before media_reference.target_url, so it is resolved the same way a bare reel/clip name is: by dir scan).
    let back = c(&mut fresh, "import_edl", json!({"path": otio_path.to_str().unwrap(), "format": "otio", "media_dir": dir.to_str().unwrap(), "dry_run": true}));
    assert_eq!(back["events_total"], 2, "{back}"); assert_eq!(back["unresolved"].as_array().unwrap().len(), 0, "{back}");
}

#[test] fn otio_export_preserves_a_gap_between_clips() {
    let dir = tmp("otio_gap"); let a = make_media(&dir, "clip_a.mp4", 10);
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let aid = c(&mut ss, "import_media", json!({"path": a.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let tr = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": tr, "source_type": "media", "asset_id": aid, "start": 2.0, "duration": 3.0}));
    let r = c(&mut ss, "export_edl", json!({"format": "otio"}));
    let v: Value = serde_json::from_str(r["content"].as_str().unwrap()).unwrap();
    let children = v["tracks"]["children"][0]["children"].as_array().unwrap();
    assert_eq!(children.len(), 2, "{children:?}"); // a leading Gap, then the clip
    assert_eq!(children[0]["OTIO_SCHEMA"], "Gap.1");
    assert!((children[0]["source_range"]["duration"]["value"].as_f64().unwrap() - 2.0 * 25.0).abs() < 1e-6);
}

#[test] fn fcpxml_export_is_well_formed_xml_with_frame_exact_rational_times() {
    let dir = tmp("fcpxml_basic"); let (mut ss, _tr, _a, _b) = two_clip_project(&dir);
    let r = c(&mut ss, "export_edl", json!({"format": "fcpxml"}));
    let content = r["content"].as_str().unwrap();
    let doc = roxmltree::Document::parse(content).unwrap_or_else(|e| panic!("export_edl fcpxml must be well-formed XML: {e}\n{content}"));
    let root = doc.root_element(); assert_eq!(root.tag_name().name(), "fcpxml");
    let clips: Vec<_> = doc.descendants().filter(|n| n.tag_name().name() == "asset-clip").collect();
    assert_eq!(clips.len(), 2, "{content}");
    assert_eq!(clips[0].attribute("offset"), Some("0/25s"));
    assert_eq!(clips[0].attribute("duration"), Some("125/25s")); // 5s at 25fps
    assert_eq!(clips[1].attribute("offset"), Some("125/25s")); // starts at 5s
    assert_eq!(clips[1].attribute("start"), Some("25/25s")); // in_point 1.0s
    let assets: Vec<_> = doc.descendants().filter(|n| n.tag_name().name() == "asset").collect();
    assert_eq!(assets.len(), 2, "{content}");
}

#[test] fn export_edl_can_write_straight_to_a_file() {
    let dir = tmp("write_file"); let (mut ss, _tr, _a, _b) = two_clip_project(&dir);
    let out = dir.join("seq.edl");
    let r = c(&mut ss, "export_edl", json!({"format": "cmx3600", "path": out.to_str().unwrap()}));
    assert_eq!(r["path"], out.to_str().unwrap()); assert!(r["content"].is_null(), "when writing to a path, content should not also be duplicated inline: {r}");
    let on_disk = std::fs::read_to_string(&out).unwrap();
    assert!(on_disk.contains("TITLE:")); assert!(on_disk.contains("clip_a.mp4"));
}

#[test] fn non_media_clips_are_skipped_and_reported_not_silently_dropped() {
    let dir = tmp("skip_nonmedia"); let (mut ss, tr, _a, _b) = two_clip_project(&dir);
    c(&mut ss, "add_clip", json!({"track_id": tr, "source_type": "solid", "color": [1.0, 0.0, 0.0], "start": 8.0, "duration": 1.0}));
    let r = c(&mut ss, "export_edl", json!({"format": "cmx3600"}));
    assert_eq!(r["events"], 2, "{r}");
    let skipped = r["skipped_non_media"].as_array().unwrap();
    assert_eq!(skipped.len(), 1, "{r}");
}

#[test] fn a_sped_up_clip_is_reported_under_approximated_speed() {
    let dir = tmp("speed_approx"); let a = make_media(&dir, "clip_a.mp4", 10);
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let aid = c(&mut ss, "import_media", json!({"path": a.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let tr = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": tr, "source_type": "media", "asset_id": aid, "start": 0.0, "duration": 2.0, "speed": 2.0}));
    let r = c(&mut ss, "export_edl", json!({"format": "cmx3600"}));
    let approx = r["approximated_speed"].as_array().unwrap();
    assert_eq!(approx.len(), 1, "{r}");
}

#[test] fn ambiguous_multi_track_media_asks_for_track_id_by_name() {
    let dir = tmp("ambiguous"); let a = make_media(&dir, "clip_a.mp4", 5); let b = make_media(&dir, "clip_b.mp4", 5);
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let aid = c(&mut ss, "import_media", json!({"path": a.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let bid = c(&mut ss, "import_media", json!({"path": b.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let t1 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let t2 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": t1, "source_type": "media", "asset_id": aid, "start": 0.0, "duration": 2.0}));
    c(&mut ss, "add_clip", json!({"track_id": t2, "source_type": "media", "asset_id": bid, "start": 0.0, "duration": 2.0}));
    let e = dispatch(&mut ss, "export_edl", &json!({"format": "cmx3600"})).unwrap_err();
    assert!(e.contains("more than one track has media clips"), "{e}"); assert!(e.contains(&t1) && e.contains(&t2), "{e}");
}

#[test] fn unknown_format_is_a_clean_error() {
    let dir = tmp("bad_format"); let (mut ss, _tr, _a, _b) = two_clip_project(&dir);
    let e = dispatch(&mut ss, "export_edl", &json!({"format": "mogrt"})).unwrap_err();
    assert!(e.contains("format must be cmx3600, otio or fcpxml"), "{e}");
}

