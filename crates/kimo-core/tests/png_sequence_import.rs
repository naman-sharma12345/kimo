//! Naman m:48b79971: a numbered .png sequence ("f_%03d.png") must fail AT IMPORT with a clear message. On main
//! 05cfbaab (and on this branch up to a539345) it probed and imported, then failed only at render with
//! "media stat: No such file or directory". A project file that references such a pattern is refused at load the same way.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

#[test]
fn png_sequence_is_refused_at_import_and_load_not_at_render() {
    let d = std::env::temp_dir().join(format!("kimo-v51-pngseq-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let st = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=red:s=32x18:d=0.2:r=25", d.join("f_%03d.png").to_str().unwrap()]).status().unwrap();
    assert!(st.success() && d.join("f_001.png").is_file(), "ffmpeg wrote the frames");
    let pat = d.join("f_%03d.png");
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":32,"height":18,"fps":25}));
    let r = dispatch(&mut s, "import_media", &json!({"path": pat}));
    eprintln!("import_media f_%03d.png -> {}", match &r { Ok(v) => format!("Ok {v}"), Err(e) => format!("Err {e}") });
    let e = r.expect_err("a .png %d sequence must be refused AT IMPORT, not imported and failed later at render");
    assert!(e.contains("image sequences are not supported") && e.contains("f_%03d.png") && e.contains("ffmpeg -framerate"), "{e}");
    // one frame of it still imports and draws
    let a = c(&mut s, "import_media", json!({"path": d.join("f_001.png")}))["asset_id"].clone();
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id":t,"source_type":"media","asset_id":a,"start":0,"duration":1}));
    c(&mut s, "render_frame", json!({"time":0.5,"path":d.join("one.png"),"inline":"never"}));
    // a saved project whose asset points at the pattern: refused at load, not at render
    let proj = d.join("p.json");
    c(&mut s, "save_project", json!({"path": proj}));
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&proj).unwrap()).unwrap();
    let assets = v["assets"].as_object_mut().expect("saved project has assets");
    for (_, asset) in assets.iter_mut() { asset["path"] = json!(pat); }
    std::fs::write(&proj, serde_json::to_string(&v).unwrap()).unwrap();
    let mut s2 = Session::new();
    let r = dispatch(&mut s2, "load_project", &json!({"path": proj}));
    eprintln!("load_project with a f_%03d.png asset -> {}", match &r { Ok(_) => "Ok".to_string(), Err(e) => format!("Err {e}") });
    let e = r.expect_err("load_project must refuse a .png %d asset, not load it and fail at render");
    assert!(e.contains("image sequences are not supported"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}
