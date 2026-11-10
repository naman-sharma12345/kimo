//! v5.1 debt (2026-10-18): a still image whose extension kimo cannot draw (.ppm) used to import fine and then
//! render flat with no error. Every way a file reaches the renderer must refuse it with an error that names the
//! extension as given: import_media, load_project of a project that already references one, and the decoder itself.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kimo-v51-ext-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn ppm(path: &Path, v: u8) {
    let (w, h) = (32usize, 32usize);
    let mut f = std::fs::File::create(path).unwrap();
    write!(f, "P6\n{w} {h}\n255\n").unwrap();
    let mut px = Vec::with_capacity(w * h * 3);
    for y in 0..h { for x in 0..w { let on = (x / 8 + y / 8) % 2 == 0; px.extend([if on { v } else { 0 }; 3]); } }
    f.write_all(&px).unwrap();
}
fn ffmpeg(args: &[&str]) {
    let st = std::process::Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).status().unwrap();
    assert!(st.success(), "ffmpeg {args:?}");
}
fn media_session(path: &Path) -> Result<Session, String> {
    let mut s = Session::new();
    dispatch(&mut s, "project_set", &json!({"width":32,"height":32,"fps":24}))?;
    let a = dispatch(&mut s, "import_media", &json!({"path": path}))?["asset_id"].clone();
    let t = dispatch(&mut s, "add_track", &json!({}))?["track_id"].clone();
    dispatch(&mut s, "add_clip", &json!({"track_id":t,"source_type":"media","asset_id":a,"start":0,"duration":1}))?;
    Ok(s)
}
fn render(s: &mut Session, t: f64) -> Result<Vec<f32>, String> {
    let mut c = Cache::new();
    kimo_core::render::prepare_at(&s.project, &mut c, Some(t))?;
    Ok(render_frame(&s.project, t, &c)?.d)
}
fn lit(px: &[f32]) -> usize { px.chunks_exact(4).filter(|p| p[0] > 0.5).count() }

/// Path 1: import_media of a single file.
#[test]
fn import_media_refuses_unsupported_still_and_names_it() {
    let d = dir("single");
    let bad = d.join("checker.ppm");
    ppm(&bad, 255);
    let mut s = Session::new();
    let err = dispatch(&mut s, "import_media", &json!({"path": bad})).expect_err("a .ppm still must be refused, not imported");
    eprintln!("refusal: {err}");
    assert!(err.contains("unsupported image extension .ppm"), "error names the extension: {err}");
    assert!(err.contains(".png"), "error lists a supported extension: {err}");
    assert!(s.project.assets.is_empty(), "a refused import adds no asset");
    let _ = std::fs::remove_dir_all(&d);
}

/// The error keeps the extension's case as given.
#[test]
fn error_keeps_the_extension_case() {
    let d = dir("case");
    let bad = d.join("FOO.PPM");
    ppm(&bad, 255);
    let err = dispatch(&mut Session::new(), "import_media", &json!({"path": bad})).expect_err("FOO.PPM must be refused");
    eprintln!("refusal: {err}");
    assert!(err.contains("unsupported image extension .PPM"), "case kept: {err}");
    let _ = std::fs::remove_dir_all(&d);
}

/// png, jpg and exr stills still import and draw (checker: half the pixels lit, same size).
#[test]
fn png_jpg_exr_still_import_and_draw() {
    let d = dir("ok");
    let src = d.join("checker.ppm");
    ppm(&src, 255);
    for ext in ["png", "jpg", "exr"] {
        let f = d.join(format!("checker.{ext}"));
        ffmpeg(&["-i", src.to_str().unwrap(), f.to_str().unwrap()]);
        let mut s = media_session(&f).unwrap_or_else(|e| panic!("{ext}: {e}"));
        let a = s.project.assets.values().next().unwrap().clone();
        assert_eq!((a.width, a.height), (32, 32), "{ext} size");
        let px = render(&mut s, 0.5).unwrap_or_else(|e| panic!("{ext} render: {e}"));
        eprintln!("{ext}: {} of {} px lit", lit(&px), px.len() / 4);
        assert_eq!(lit(&px), 512, "{ext}: the checker draws (512 of 1024 lit)");
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// Path 2: a project file that already references a .ppm (saved before this fix, or hand-edited).
/// load_project refuses it, and a session that reaches the renderer with one errors instead of drawing flat.
#[test]
fn project_load_and_render_of_a_ppm_reference_error() {
    let d = dir("load");
    let png = d.join("checker.png");
    let bad = d.join("checker.ppm");
    ppm(&bad, 255);
    ffmpeg(&["-i", bad.to_str().unwrap(), png.to_str().unwrap()]);
    let mut s = media_session(&png).unwrap();
    for a in s.project.assets.values_mut() { a.path = bad.to_str().unwrap().into(); }
    let file = d.join("old.kimo.json");
    dispatch(&mut s, "save_project", &json!({"path": file})).unwrap();
    let mut s2 = Session::new();
    let r = dispatch(&mut s2, "load_project", &json!({"path": file}));
    eprintln!("load_project: {r:?}");
    let err = r.expect_err("load_project must refuse a .ppm reference");
    assert!(err.contains("unsupported image extension .ppm"), "{err}");
    let r = render(&mut s, 0.5);
    eprintln!("render of the .ppm reference: {}", match &r { Ok(px) => format!("Ok, {} lit px", lit(px)), Err(e) => e.clone() });
    let err = r.expect_err("the renderer must refuse a .ppm reference, not draw it flat");
    assert!(err.contains("unsupported image extension .ppm"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

/// Path 3: a numbered .ppm sequence (import_media with a %d pattern) and a folder. The sequence is refused with the
/// same extension error; a folder is not media and import_media refuses it (probe error). Neither draws flat.
#[test]
fn ppm_sequence_and_folder_are_refused() {
    let d = dir("seq");
    for i in 0..24 { ppm(&d.join(format!("f_{i:03}.ppm")), 255); }
    let pat = d.join("f_%03d.ppm");
    let err = media_session(&pat).map(|_| ()).expect_err("a .ppm sequence must be refused");
    eprintln!("sequence: {err}");
    assert!(err.contains("unsupported image extension .ppm"), "{err}");
    let err = media_session(&d).map(|_| ()).expect_err("a folder is not media");
    eprintln!("folder: {err}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn still_ext_list_is_the_decoders() {
    for p in ["/a/b.png", "/a/b.JPG", "/a/b.jpeg", "/a/b.webp", "/a/b.bmp", "/a/b.exr", "/a/b.tif", "/a/b.TIFF", "/a/b.pfm"] {
        assert!(kimo_core::render::is_still_path(p), "{p}");
    }
    for p in ["/a/b.ppm", "/a/b.tga", "/a/b.heic", "/a/b.mp4", "/a/b.gif"] {
        assert!(!kimo_core::render::is_still_path(p), "{p}");
    }
}
