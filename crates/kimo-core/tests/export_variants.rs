//! v7 additions: export_variants -- batch-export several size/aspect-ratio deliverables in one call.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn e(s: &mut Session, n: &str, a: Value) -> String { dispatch(s, n, &a).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn out(tag: &str, name: &str) -> PathBuf { std::env::temp_dir().join(format!("kimo-export-variants-{tag}-{name}-{}.mp4", std::process::id())) }

fn solid_scene(tag: &str) -> (Session, (u32, u32)) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 12}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.3, 0.8], "start": 0.0, "duration": 0.2}));
    let _ = tag; (s, (64, 36))
}

fn media_scene(tag: &str) -> (Session, String) {
    let path = std::env::temp_dir().join(format!("kimo-export-variants-src-{tag}-{}.mp4", std::process::id()));
    let status = std::process::Command::new("ffmpeg").args(["-v","error","-y","-f","lavfi","-i","color=c=red:size=64x48:rate=12:duration=0.5","-c:v","mpeg4","-q:v","2", path.to_str().unwrap()]).status().unwrap();
    assert!(status.success());
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 48, "fps": 12}));
    let asset = c(&mut s, "import_media", json!({"path": path}))["asset_id"].clone();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let clip = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": asset, "start": 0.0, "duration": 0.5, "fit": "cover"}))["clip_id"].as_str().unwrap().to_string();
    (s, clip)
}

#[test] fn exports_each_variant_at_its_own_size_and_restores_the_canvas() {
    let (mut s, orig) = solid_scene("a");
    let p1 = out("a", "sq"); let p2 = out("a", "same");
    let r = c(&mut s, "export_variants", json!({"variants": [
        {"name": "sq", "width": 48, "height": 48, "path": p1.to_str().unwrap()},
        {"name": "same", "path": p2.to_str().unwrap()},
    ]}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["variants"][0]["width"], 48); assert_eq!(r["variants"][0]["height"], 48);
    assert_eq!(r["variants"][1]["width"], orig.0); assert_eq!(r["variants"][1]["height"], orig.1, "no size given -> current canvas size");
    assert!(p1.exists() && std::fs::metadata(&p1).unwrap().len() > 0);
    assert!(p2.exists() && std::fs::metadata(&p2).unwrap().len() > 0);
    assert_eq!((s.project.width, s.project.height), orig, "the project's own canvas is restored after the call");
    let _ = std::fs::remove_file(&p1); let _ = std::fs::remove_file(&p2);
}

#[test] fn a_bad_variant_fails_on_its_own_without_stopping_the_rest() {
    let (mut s, orig) = solid_scene("b");
    let p_ok = out("b", "good");
    let r = c(&mut s, "export_variants", json!({"variants": [
        {"name": "bad", "width": 0, "height": 10, "path": "/tmp/should-not-write.mp4"},
        {"name": "good", "width": 32, "height": 32, "path": p_ok.to_str().unwrap()},
    ]}));
    assert_eq!(r["ok"], false); assert_eq!(r["variants"][0]["ok"], false); assert!(r["variants"][0]["error"].as_str().unwrap().contains("1..8192"));
    assert_eq!(r["variants"][1]["ok"], true, "{r}");
    assert!(p_ok.exists());
    assert_eq!((s.project.width, s.project.height), orig, "canvas restored even when a variant failed");
    let _ = std::fs::remove_file(&p_ok);
}

#[test] fn path_template_fills_in_name_and_duplicate_names_are_rejected() {
    let (mut s, _orig) = solid_scene("c");
    let dir = std::env::temp_dir(); let tmpl = dir.join(format!("kimo-ev-tmpl-{}-{{name}}.mp4", std::process::id()));
    let r = c(&mut s, "export_variants", json!({"variants": [{"name": "x", "width": 32, "height": 32}], "path_template": tmpl.to_str().unwrap()}));
    assert_eq!(r["ok"], true, "{r}");
    let expect = dir.join(format!("kimo-ev-tmpl-{}-x.mp4", std::process::id()));
    assert!(expect.exists(), "path_template's {{name}} should have been filled with 'x'");
    let _ = std::fs::remove_file(&expect);

    assert!(e(&mut s, "export_variants", json!({"variants": [{"name": "x", "width": 1, "height": 1}, {"name": "x", "width": 2, "height": 2}]})).contains("duplicate"));
    assert!(e(&mut s, "export_variants", json!({"variants": [{"name": "x", "width": 1, "height": 1}], "path_template": "no-placeholder.mp4"})).contains("path_template"));
    assert!(e(&mut s, "export_variants", json!({"variants": []})).contains("variants"));
    // width without height is a per-variant failure, not a whole-call error (other variants in the same call still run)
    let r2 = c(&mut s, "export_variants", json!({"variants": [{"name": "x", "width": 10}]}));
    assert_eq!(r2["ok"], false); assert!(r2["variants"][0]["error"].as_str().unwrap().contains("width and height"));
}

#[test] fn unknown_aspect_is_a_typed_error_for_that_variant_only() {
    let (mut s, _orig) = solid_scene("d");
    let p = out("d", "ar");
    let r = c(&mut s, "export_variants", json!({"variants": [{"name": "ar", "aspect": "3:2", "path": p.to_str().unwrap()}]}));
    assert_eq!(r["ok"], false); assert!(r["variants"][0]["error"].as_str().unwrap().contains("Valid:"));
    assert!(!p.exists());
}

#[test] fn auto_reframe_tracks_the_subject_for_a_differing_aspect_and_leaves_nothing_behind() {
    let (mut s, clip) = media_scene("e");
    let p = out("e", "vert");
    let r = c(&mut s, "export_variants", json!({"variants": [{"name": "vert", "width": 48, "height": 64, "path": p.to_str().unwrap()}], "auto_reframe": true}));
    assert_eq!(r["ok"], true, "{r}");
    assert!(p.exists() && std::fs::metadata(&p).unwrap().len() > 0);
    let has_reframe = s.project.find_clip(&clip).unwrap().props.keys().any(|k| k.starts_with("reframe_"));
    assert!(!has_reframe, "the reframe window should be removed again after the variant that needed it");
    let _ = std::fs::remove_file(&p);
}
