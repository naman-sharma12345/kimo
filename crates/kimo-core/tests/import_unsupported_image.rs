//! v5.1 debt (2026-10-18): a still image whose extension kimo cannot draw (.ppm) used to import fine and then
//! render flat with no error. import_media must refuse it, name the extension, and leave the project unchanged.
use kimo_core::commands::{dispatch, Session};
use serde_json::json;
use std::io::Write;

fn ppm(path: &std::path::Path) {
    let (w, h) = (32usize, 32usize);
    let mut f = std::fs::File::create(path).unwrap();
    write!(f, "P6\n{w} {h}\n255\n").unwrap();
    let mut px = Vec::with_capacity(w * h * 3);
    for y in 0..h { for x in 0..w { let v = if (x / 8 + y / 8) % 2 == 0 { 255 } else { 0 }; px.extend([v, v, v]); } }
    f.write_all(&px).unwrap();
}

#[test]
fn import_media_refuses_unsupported_still_and_names_it() {
    let dir = std::env::temp_dir().join(format!("kimo-v51-ext-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("checker.ppm");
    ppm(&bad);
    let mut s = Session::new();
    let before = s.project.assets.len();
    let err = dispatch(&mut s, "import_media", &json!({"path": bad})).expect_err("a .ppm still must be refused, not imported");
    eprintln!("refusal: {err}");
    assert!(err.contains("unsupported image format: ppm"), "error names the format: {err}");
    assert!(err.contains("png"), "error lists a supported format: {err}");
    assert_eq!(s.project.assets.len(), before, "a refused import adds no asset");

    // Control: the same pixels as .png (converted by ffmpeg) still import, with the probed size.
    let good = dir.join("checker.png");
    let st = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-i"]).arg(&bad).arg(&good).status().unwrap();
    assert!(st.success(), "ffmpeg ppm -> png");
    let r = dispatch(&mut s, "import_media", &json!({"path": good})).expect("a .png still imports");
    assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(32), Some(32)));
    assert_eq!(s.project.assets.len(), before + 1);
    let _ = std::fs::remove_dir_all(&dir);
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
