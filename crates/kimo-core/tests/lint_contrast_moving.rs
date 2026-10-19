//! Naman m:796d7bd1: text_contrast over a REAL moving video, not a still. The background is an H.264 clip that fades
//! from black to white over 4 s; white text sits on it the whole time. The lint must pass the early (dark) frames and
//! flag the late (light) ones, with a ratio that falls frame by frame. On main 05cfbaab the media under the text drew
//! black, so every frame read 21.0 and nothing was ever flagged.
//!
//! What the code does (src/ext/lint_frame.rs): WCAG 2.x contrast of the text colour against EVERY background pixel in
//! the text's box; the frame fails when the worst (minimum) pixel ratio is under the threshold. Threshold:
//! params.contrast_normal 4.5, or params.contrast_large 3.0 when the capital-H ink height scaled to 1080p is
//! >= params.large_cap_px 24. Frames: only the times the caller passes (time / times / range, range step default 0.5 s,
//! max 120); there is no automatic sampling.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::Path;

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

/// White DejaVuSans text of `size` px over the fading clip, both 0-4 s, 640x360 at 25 fps.
fn scene(bg: &Path, size: f64) -> Session {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":640,"height":360,"fps":25}));
    let a = c(&mut s, "import_media", json!({"path": bg}))["asset_id"].clone();
    let t0 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id":t0,"source_type":"media","asset_id":a,"start":0,"duration":4}));
    let t1 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_text", json!({"track_id":t1,"text":"H title","font":"DejaVuSans","size":size,"color":[1,1,1],"start":0,"duration":4}))["clip_id"].clone();
    c(&mut s, "set_meta", json!({"id":id,"name":"title","role":"title"}));
    s
}

const TIMES: [f64; 8] = [0.25, 0.75, 1.25, 1.75, 2.25, 2.75, 3.25, 3.75];

/// Per frame: (time, min_ratio measured with forced thresholds, threshold the default run used or None, default run flagged?).
fn per_frame(s: &mut Session) -> Vec<(f64, f64, f64, bool)> {
    let tc = |r: &Value, i: usize| -> Option<Value> {
        r["frames"][i]["findings"].as_array().unwrap().iter().find(|f| f["code"] == "text_contrast").cloned()
    };
    // default thresholds: what a user gets
    let d = c(s, "lint_frame", json!({"times":TIMES,"checks":["text_contrast"],"annotate":false}));
    // thresholds forced to 22 so every frame reports its ratio (and the threshold the default rule picks is read back)
    let f = c(s, "lint_frame", json!({"times":TIMES,"checks":["text_contrast"],"params":{"contrast_normal":22,"contrast_large":22},"annotate":false}));
    assert_eq!(d["summary"]["frames_checked"], json!(TIMES.len()));
    (0..TIMES.len()).map(|i| {
        let forced = tc(&f, i).unwrap_or_else(|| panic!("t={}: forced finding missing", TIMES[i]));
        let thr = if let Some(x) = tc(&d, i) { x["measured"]["threshold"].as_f64().unwrap() } else { f64::NAN };
        (TIMES[i], forced["measured"]["min_ratio"].as_f64().unwrap(), thr, tc(&d, i).is_some())
    }).collect()
}

#[test]
fn text_contrast_over_a_fading_video_passes_dark_frames_and_flags_light_ones() {
    let d = std::env::temp_dir().join(format!("kimo-v51-fade-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let mp4 = d.join("fade_black_to_white.mp4");
    let st = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=white:s=640x360:d=4:r=25,fade=t=in:st=0:d=4", "-c:v", "libx264", "-pix_fmt", "yuv420p", mp4.to_str().unwrap()]).status().unwrap();
    assert!(st.success(), "ffmpeg fade clip");
    // size 24 at 360p: capital H 16 px = 48 px at 1080p -> "large" (3.0). size 8: under 24 px at 1080p -> "normal" (4.5).
    // (size 10 lands exactly on 24.0 px, which the rule counts as large.)
    for (size, want_thr) in [(24.0, 3.0), (8.0, 4.5)] {
        let mut s = scene(&mp4, size);
        // the rule's own size reading: capital-H ink height scaled to 1080p (min_text_size, forced to report)
        let m = c(&mut s, "lint_frame", json!({"time":1,"checks":["min_text_size"],"params":{"min_cap_px":100000},"annotate":false}));
        let cap1080 = m["frames"][0]["findings"].as_array().unwrap().iter().find(|f| f["code"] == "min_text_size")
            .map(|f| f["measured"]["cap_height_1080_px"].as_f64().unwrap()).expect("cap height reading");
        eprintln!("size {size}: capital H = {cap1080:.1} px at 1080p -> {} text", if cap1080 >= 24.0 { "large" } else { "normal" });
        assert_eq!(cap1080 >= 24.0, want_thr == 3.0, "size {size}: large/normal split as planned (cap {cap1080:.1} px at 1080p)");
        let rows = per_frame(&mut s);
        for (t, r, thr, flagged) in &rows {
            eprintln!("size {size}: t={t:.2}s min_ratio {r:.3} -> {} (threshold {want_thr}{})", if *flagged { "FAIL" } else { "pass" }, if thr.is_nan() { String::new() } else { format!(", reported {thr}") });
        }
        // Naman m:4b69a6b4 / m:48b79971: the 2.25 s split, asserted by name, both sides. At t=2.25 the ratio is ~3.23:
        // size 24 is large text (threshold 3.0) and PASSES because 3.23 >= 3.0; size 8 is normal text (threshold 4.5)
        // and FAILS because 3.23 < 4.5.
        let split = *rows.iter().find(|x| (x.0 - 2.25).abs() < 1e-9).expect("t=2.25 is sampled");
        if size == 24.0 {
            assert!(split.1 >= 3.0 && !split.3, "t=2.25 size 24 (large, 3.0): ratio {:.3} must be >= 3.0 and PASS (flagged: {})", split.1, split.3);
        } else {
            assert!(split.1 < 4.5 && split.3, "t=2.25 size 8 (normal, 4.5): ratio {:.3} must be < 4.5 and FAIL (flagged: {})", split.1, split.3);
            assert_eq!(split.2, 4.5, "t=2.25 size 8: the threshold used must be 4.5");
        }
        assert!((split.1 - 3.23).abs() < 0.05, "t=2.25 size {size}: ratio {:.3} must be ~3.23, between 3.0 and 4.5", split.1);
        for (t, r, thr, flagged) in &rows {
            assert!((r - 21.0).abs() > 1e-6, "size {size} t={t}: text_contrast printed 21.0 over a moving media background");
            assert_eq!(*flagged, *r < want_thr, "size {size} t={t}: ratio {r:.3} vs threshold {want_thr} must decide pass/fail");
            if *flagged { assert_eq!(*thr, want_thr, "size {size} t={t}: threshold used"); }
        }
        let (first, last) = (rows[0], rows[rows.len() - 1]);
        assert!(!first.3 && first.1 > 12.0, "size {size}: the dark first frame (t={}) must pass, ratio {:.3}", first.0, first.1);
        assert!(last.3 && last.1 < 1.6, "size {size}: the near-white last frame (t={}) must FAIL, ratio {:.3}", last.0, last.1);
        for w in rows.windows(2) {
            assert!(w[1].1 <= w[0].1 + 0.05, "size {size}: ratio must fall as the clip brightens: t={} {:.3} -> t={} {:.3}", w[0].0, w[0].1, w[1].0, w[1].1);
        }
        assert!(rows.iter().any(|x| !x.3) && rows.iter().any(|x| x.3), "size {size}: at least one passing and one failing frame");
        // the frames themselves, boxed, when asked (KIMO_V51_FRAMES=dir): one passing (first) and one failing (last)
        if let Ok(out) = std::env::var("KIMO_V51_FRAMES") {
            for (tag, t) in [("pass", first.0), ("fail", last.0)] {
                let r = c(&mut s, "lint_frame", json!({"time":t,"checks":["text_contrast"],"params":{"contrast_normal":22,"contrast_large":22}}));
                let p = r["frames"][0]["annotation"]["path"].as_str().unwrap().to_string();
                let dst = Path::new(&out).join(format!("size{size}_{tag}_t{t}.png"));
                std::fs::copy(&p, &dst).unwrap();
                eprintln!("frame {tag} t={t}: {}", dst.display());
            }
        }
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// Naman m:83105feb: prove the lint reads the RIGHT frame, not a prepared first frame. Each sampled time t must read
/// video frame floor(t * 25): its ratio is checked against that frame decoded independently by ffmpeg (the brightest
/// pixel under white text), and must sit closer to it than to the frames either side.
#[test]
fn text_contrast_reads_the_exact_sampled_frame_of_a_moving_video() {
    let d = std::env::temp_dir().join(format!("kimo-v51-fadeidx-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let mp4 = d.join("fade_black_to_white.mp4");
    let st = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=white:s=640x360:d=4:r=25,fade=t=in:st=0:d=4", "-c:v", "libx264", "-pix_fmt", "yuv420p", mp4.to_str().unwrap()]).status().unwrap();
    assert!(st.success(), "ffmpeg fade clip");
    // independent decode: every frame as RGBA, ratio of white against its brightest pixel
    let out = std::process::Command::new("ffmpeg").args(["-v", "error", "-i", mp4.to_str().unwrap(), "-f", "rawvideo", "-pix_fmt", "rgba", "-"]).output().unwrap();
    let px = 640 * 360 * 4;
    assert!(out.status.success() && out.stdout.len() % px == 0 && out.stdout.len() / px >= 99, "decoded {} bytes", out.stdout.len());
    let lin = |v: u8| { let c = v as f64 / 255.0; if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } };
    let truth: Vec<f64> = out.stdout.chunks_exact(px).map(|f| {
        let l = f.chunks_exact(4).map(|p| 0.2126 * lin(p[0]) + 0.7152 * lin(p[1]) + 0.0722 * lin(p[2])).fold(0.0, f64::max);
        1.05 / (l + 0.05)
    }).collect();
    let mut s = scene(&mp4, 24.0);
    let rows = per_frame(&mut s);
    let mut seen = vec![];
    for (t, r, _, _) in &rows {
        let n = (t * 25.0).floor() as usize;
        eprintln!("t={t:.2}s -> frame {n}: lint {r:.3}, ffmpeg frame {} {:.3} | {n} {:.3} | {} {:.3}", n - 1, truth[n - 1], truth[n], n + 1, truth[n + 1]);
        assert!((r - truth[n]).abs() < 0.01, "t={t}: lint {r:.4} must equal frame {n}'s ratio {:.4}", truth[n]);
        assert!((r - truth[n]).abs() < (r - truth[n - 1]).abs() && (r - truth[n]).abs() < (r - truth[n + 1]).abs(), "t={t}: closest to frame {n}, not its neighbours");
        seen.push(n);
    }
    assert_eq!(seen, vec![6, 18, 31, 43, 56, 68, 81, 93], "8 sampled frames, by index");
    let _ = std::fs::remove_dir_all(&d);
}
