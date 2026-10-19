//! v5.1 debt (lint_frame text_contrast over media, read-only check b72a06c, 2026-10-18): over a video or image
//! background text_contrast read 21.0/21.0/21.0 (perfect contrast) where validate_captions measured about 1-7,
//! because the background was rendered from a fresh cache with no prepare_at, so the media drew black.
//! This test fails if text_contrast ever reports 21.0 over a media background again.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::Path;

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn ffmpeg(args: &[&str]) {
    let st = std::process::Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).status().unwrap();
    assert!(st.success(), "ffmpeg {args:?}");
}
/// White 24 px text over a media clip (the background file), both 0-3 s on a 640x360 project.
fn scene(bg: &Path) -> Session {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":640,"height":360,"fps":25}));
    let a = c(&mut s, "import_media", json!({"path": bg}))["asset_id"].clone();
    let t0 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id":t0,"source_type":"media","asset_id":a,"start":0,"duration":3}));
    let t1 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_text", json!({"track_id":t1,"text":"H title","font":"DejaVuSans","size":24,"color":[1,1,1],"start":0,"duration":3}))["clip_id"].clone();
    c(&mut s, "set_meta", json!({"id":id,"name":"title","role":"title"}));
    s
}
/// min_ratio of every text_contrast finding; contrast_normal 22 forces a finding whatever the ratio.
fn min_ratios(s: &mut Session) -> Vec<f64> {
    let r = c(s, "lint_frame", json!({"time":1,"checks":["text_contrast"],"params":{"contrast_normal":22,"large_cap_px":10000},"annotate":false}));
    r["frames"].as_array().unwrap().iter().flat_map(|f| f["findings"].as_array().unwrap().iter())
        .filter(|f| f["code"] == "text_contrast" || f["check"] == "text_contrast")
        .map(|f| f["measured"]["min_ratio"].as_f64().unwrap()).collect()
}

#[test]
fn text_contrast_over_media_reads_the_real_background_never_21() {
    let d = std::env::temp_dir().join(format!("kimo-v51-contrast-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    // white text over a white still: true ratio 1.0
    let png = d.join("white.png");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=white:s=640x360", "-frames:v", "1", png.to_str().unwrap()]);
    // white text over a #777777 video: true ratio about 4.48 (the solid-background test reads 4.478089 for #777)
    let mp4 = d.join("grey.mp4");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=0x777777:s=640x360:d=3:r=25", "-c:v", "libx264", "-pix_fmt", "yuv420p", mp4.to_str().unwrap()]);
    for (bg, lo, hi) in [(&png, 1.0, 1.05), (&mp4, 4.2, 4.8)] {
        let mut s = scene(bg);
        let r = min_ratios(&mut s);
        eprintln!("{}: text_contrast min_ratio {r:?} (want {lo}..{hi})", bg.display());
        assert!(!r.is_empty(), "{}: a forced text_contrast finding exists", bg.display());
        for v in &r {
            assert!((v - 21.0).abs() > 1e-6, "{}: text_contrast printed 21.0 over a media background", bg.display());
            assert!(*v >= lo && *v <= hi, "{}: min_ratio {v} outside {lo}..{hi}", bg.display());
        }
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// Not an assertion: prints lint_frame's full output for text over SOLID backgrounds (no media), timing keys removed,
/// so the bytes can be compared between main and this fix (they must be equal: the fix only matters over media).
#[test]
fn dump_solid_background_output() {
    fn strip(v: &mut Value) { match v { Value::Object(m) => { m.retain(|k, _| !k.contains("elapsed") && !k.ends_with("_ms")); for x in m.values_mut() { strip(x) } } Value::Array(a) => for x in a { strip(x) }, _ => {} } }
    for (bgc, tc) in [([1.0, 1.0, 1.0], [119.0 / 255.0; 3]), ([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]), ([0.2, 0.4, 0.8], [1.0, 0.9, 0.1])] {
        let mut s = Session::new();
        c(&mut s, "project_set", json!({"width":640,"height":360,"fps":25}));
        let t0 = c(&mut s, "add_track", json!({}))["track_id"].clone();
        c(&mut s, "add_clip", json!({"track_id":t0,"source_type":"solid","color":bgc,"start":0,"duration":3}));
        let t1 = c(&mut s, "add_track", json!({}))["track_id"].clone();
        let id = c(&mut s, "add_text", json!({"track_id":t1,"text":"H title","font":"DejaVuSans","size":24,"color":tc,"start":0,"duration":3}))["clip_id"].clone();
        c(&mut s, "set_meta", json!({"id":id,"name":"title","role":"title"}));
        for a in [json!({"time":1,"checks":["text_contrast"],"params":{"contrast_normal":22,"large_cap_px":10000},"annotate":false}), json!({"times":[0.5,1,2],"annotate":false})] {
            let mut r = c(&mut s, "lint_frame", a);
            strip(&mut r);
            eprintln!("DUMP: {}", serde_json::to_string(&r).unwrap());
        }
    }
}
