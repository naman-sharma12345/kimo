//! RV-7 wiring: relative stored paths (media, LUT, font) resolve against the PROJECT FILE's directory, not the process CWD, in every reader
//! (render, export worker threads, text). Absolute paths behave exactly as before. Output paths stay caller-relative.
//! One #[test] on purpose: it changes the process CWD.
use kimo_core::commands::{dispatch, Session};
use serde_json::json;
use std::path::Path;
use std::process::Command;

fn sh(args: &[&str]) { let o = Command::new(args[0]).args(&args[1..]).output().expect(args[0]); assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr)); }
fn framemd5(p: &str) -> String { String::from_utf8(Command::new("ffmpeg").args(["-v", "error", "-i", p, "-map", "0:v", "-f", "framemd5", "-"]).output().unwrap().stdout).unwrap() }
fn bytes(p: &str) -> Vec<u8> { std::fs::read(p).unwrap_or_else(|e| panic!("{p}: {e}")) }
fn find_font() -> Option<String> { for d in ["/usr/share/fonts", "/usr/local/share/fonts", "/Library/Fonts", "/System/Library/Fonts"] { let mut st = vec![std::path::PathBuf::from(d)]; while let Some(x) = st.pop() { if let Ok(rd) = std::fs::read_dir(&x) { for e in rd.flatten() { let p = e.path(); if p.is_dir() { st.push(p) } else if p.extension().map_or(false, |e| e == "ttf") { return Some(p.to_string_lossy().into()); } } } } } None }

#[test]
fn relative_paths_resolve_against_project_dir() {
    let root = std::env::temp_dir().join(format!("kimo_wiring_{}", std::process::id())); let _ = std::fs::remove_dir_all(&root);
    let a = root.join("A"); let other = root.join("elsewhere"); std::fs::create_dir_all(a.join("media")).unwrap(); std::fs::create_dir_all(a.join("luts")).unwrap(); std::fs::create_dir_all(a.join("fonts")).unwrap(); std::fs::create_dir_all(&other).unwrap();
    sh(&["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "testsrc=size=160x90:rate=25:duration=1", "-pix_fmt", "yuv420p", a.join("media/v.mp4").to_str().unwrap()]);
    let cube = "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n"; // identity
    let cube_inv = "LUT_3D_SIZE 2\n1 1 1\n0 1 1\n1 0 1\n0 0 1\n1 1 0\n0 1 0\n1 0 0\n0 0 0\n"; // inverts: visible in the frames
    let _ = cube; std::fs::write(a.join("luts/look.cube"), cube_inv).unwrap();
    let font = find_font(); if let Some(f) = &font { std::fs::copy(f, a.join("fonts/f.ttf")).unwrap(); }
    let ab = |p: &str| a.join(p).to_str().unwrap().to_string();

    let mut ss = Session::new(); let mut c = |ss: &mut Session, n: &str, v: serde_json::Value| dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}"));
    c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let tid = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let asset = c(&mut ss, "import_media", json!({"path": ab("media/v.mp4")}))["asset_id"].as_str().unwrap().to_string();
    let clip = c(&mut ss, "add_clip", json!({"track_id": tid, "source_type": "media", "asset_id": asset, "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_effect", json!({"clip_id": clip, "kind": "lut", "strings": {"path": ab("luts/look.cube")}}));
    if font.is_some() { let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); c(&mut ss, "add_text", json!({"track_id": t, "text": "Hello", "start": 0, "duration": 1, "size": 30, "font": ab("fonts/f.ttf")})); }

    // reference: absolute paths
    let (ref_png, ref_mp4) = (root.join("ref.png").to_str().unwrap().to_string(), root.join("ref.mp4").to_str().unwrap().to_string());
    c(&mut ss, "render_frame", json!({"time": 0.5, "path": ref_png})); c(&mut ss, "export", json!({"path": ref_mp4}));

    // rewrite every reference to a path relative to A and save the project in A
    for r in ss.project.external_refs() { if r.family { continue; } let rel = Path::new(&r.path).strip_prefix(&a).unwrap().to_str().unwrap().to_string(); ss.project.set_ref(&r.field, &rel).unwrap(); }
    assert!(ss.project.external_refs().iter().all(|r| r.family || !r.path.starts_with('/')), "paths were not made relative");
    let proj = ab("p.kimo"); c(&mut ss, "save_project", json!({"path": proj}));

    // failing row on the old behaviour: a project without a base dir reads relative paths from the CWD (here: elsewhere) and cannot render
    std::env::set_current_dir(&other).unwrap();
    { let mut raw = Session::new(); raw.project = serde_json::from_str(&std::fs::read_to_string(&proj).unwrap()).unwrap(); kimo_core::deps::set_base_dir(None);
      assert!(dispatch(&mut raw, "render_frame", &json!({"time": 0.5, "path": "no_base.png"})).is_err(), "relative media must not resolve without a base dir"); }

    // fresh session, CWD elsewhere, absolute project path: must match the absolute-path reference bit for bit
    let mut b = Session::new(); c(&mut b, "load_project", json!({"path": proj}));
    let (b_png, b_mp4) = (root.join("b.png").to_str().unwrap().to_string(), root.join("b.mp4").to_str().unwrap().to_string());
    c(&mut b, "render_frame", json!({"time": 0.5, "path": b_png})); c(&mut b, "export", json!({"path": b_mp4}));
    assert_eq!(bytes(&ref_png), bytes(&b_png), "frame differs when paths are relative");
    let (m1, m2) = (framemd5(&ref_mp4), framemd5(&b_mp4)); assert!(!m1.is_empty() && m1 == m2, "export (worker threads) differs when paths are relative");
    // output paths are NOT rebased: a relative output lands in the CWD
    c(&mut b, "render_frame", json!({"time": 0.5, "path": "out_rel.png"})); assert!(other.join("out_rel.png").exists() && !a.join("out_rel.png").exists());
    // the LUT really took effect (inverted vs the source): frame is not the plain media frame
    sh(&["ffmpeg", "-v", "error", "-ss", "0.5", "-i", a.join("media/v.mp4").to_str().unwrap(), "-frames:v", "1", root.join("plain.png").to_str().unwrap()]);
    assert_ne!(bytes(&b_png), bytes(root.join("plain.png").to_str().unwrap()));
    let _ = std::fs::remove_dir_all(&root);
}
