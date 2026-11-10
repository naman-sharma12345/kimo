use kimo_core::{
    commands::{dispatch, Session},
    perception::analysis::Image,
};
use serde_json::{json, Value};
fn image(w: usize, h: usize, v: f64) -> Image {
    let mut a = Image::new(w, h);
    for c in 0..3 {
        a.rgb[c].fill(v);
    }
    a
}
fn cmp(a: &Image, b: &Image, t: f64) -> Value {
    kimo_core::ext::diff_frames::compare(a, b, t, 4, 4)
}
#[test]
fn identical_and_black_white() {
    let a = image(32, 24, 0.);
    let b = image(32, 24, 1.);
    let r = cmp(&a, &a, 0.04);
    assert_eq!(r["metrics"]["mse"], 0.);
    assert_eq!(r["metrics"]["psnr_db"], 99.);
    assert_eq!(r["metrics"]["ssim"], 1.);
    assert_eq!(r["verdict_hint"], "identical");
    let r = cmp(&a, &b, 0.04);
    for k in ["mse", "max_abs", "changed_ratio", "hist_distance"] {
        assert_eq!(r["metrics"][k], 1.);
    }
    assert!(r["metrics"]["ssim"].as_f64().unwrap() < 0.01);
}
#[test]
fn gradient_exact_and_threshold() {
    let mut a = image(64, 32, 0.);
    let mut b = a.clone();
    for i in 0..64 * 32 {
        for c in 0..3 {
            a.rgb[c][i] = (i % 64) as f64 / 255.;
            b.rgb[c][i] = a.rgb[c][i] + 16. / 255.;
        }
    }
    let r = cmp(&a, &b, 0.04);
    assert!((r["metrics"]["mse"].as_f64().unwrap() - (16f64 / 255.).powi(2)).abs() < 1e-9);
    assert_eq!(r["metrics"]["changed_ratio"], 1.);
    assert_eq!(cmp(&a, &b, 0.1)["metrics"]["changed_ratio"], 0.);
    let mut blue = a.clone();
    blue.rgb[2].fill(1.);
    assert!(
        cmp(&a, &blue, 0.04)["metrics"]["luma_delta"]
            .as_f64()
            .unwrap()
            < 0.08
    );
}
#[test]
fn square_grid_region() {
    let mut a = image(400, 400, 0.);
    let mut b = a.clone();
    for c in 0..3 {
        for y in 0..100 {
            for x in 0..100 {
                a.rgb[c][y * 400 + x] = 1.;
                b.rgb[c][y * 400 + x + 100] = 1.;
            }
        }
    }
    let r = cmp(&a, &b, 0.04);
    assert_eq!(r["metrics"]["changed_ratio"], 0.125);
    let grid = r["grid"]["mean_abs_delta"].as_array().unwrap();
    assert_eq!(grid[0], 1.);
    assert_eq!(grid[1], 1.);
    assert_eq!(grid[15], 0.);
    assert_eq!(
        cmp(
            &a.crop([200, 200, 100, 100]),
            &b.crop([200, 200, 100, 100]),
            0.04
        )["verdict_hint"],
        "identical"
    );
}
fn session() -> Session {
    let mut ss = Session::new();
    dispatch(
        &mut ss,
        "project_set",
        &json!({"width":32,"height":24,"fps":24}),
    )
    .unwrap();
    let tr = dispatch(&mut ss, "add_track", &json!({})).unwrap()["track_id"].clone();
    for (i, c) in [[0., 0., 0.], [1., 1., 1.]].iter().enumerate() {
        dispatch(
            &mut ss,
            "add_clip",
            &json!({"track_id":tr,"source_type":"solid","color":c,"start":i,"duration":1}),
        )
        .unwrap();
    }
    ss
}
#[test]
fn wrapper_region_heatmap_purity_repeat_threads() {
    let mut ss = session();
    let before = serde_json::to_string(&ss.project).unwrap();
    let a = json!({"a":{"time":0.5},"b":{"frame":36},"metrics":["mse","ssim"],"heatmap":true,"scale":1,"region":[0,0,16,16]});
    let mut first = None;
    for threads in [1, 4, 1] {
        kimo_core::par::set_threads(threads);
        let r = dispatch(&mut ss, "diff_frames", &a).unwrap();
        assert_eq!(r["metrics"]["mse"], 1.);
        assert_eq!(r["compared"]["w"], 16);
        assert_eq!(ss.media.len(), 1);
        assert!(std::path::Path::new(r["heatmap"]["path"].as_str().unwrap()).exists());
        let bytes = serde_json::to_string(&r).unwrap();
        if let Some(f) = &first {
            assert_eq!(&bytes, f);
        } else {
            first = Some(bytes);
        }
        assert_eq!(before, serde_json::to_string(&ss.project).unwrap());
    }
}
#[test]
fn errors_and_fuzz() {
    let mut ss = session();
    for (a, code) in [
        (json!({"a":{"source":"bad"},"b":{}}), "invalid_source"),
        (json!({"a":{},"b":{},"metrics":["bad"]}), "invalid_metric"),
        (json!({"a":{"time":-1},"b":{}}), "out_of_range"),
        (json!({"a":{},"b":{},"region":[31,0,2,2]}), "out_of_range"),
    ] {
        assert!(dispatch(&mut ss, "diff_frames", &a)
            .unwrap_err()
            .starts_with(code));
    }
    for v in [
        json!(null),
        json!([]),
        json!("wrong"),
        json!(true),
        json!(-1),
    ] {
        let a = json!({"a":{"time":v},"b":{}});
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dispatch(
                &mut ss,
                "diff_frames",
                &a
            )))
            .is_ok()
        );
    }
}
#[test]
fn real_video_adjacent_far_and_size_mismatch() {
    let path = std::env::temp_dir().join(format!("kimo-diff-real-{}.mp4", std::process::id()));
    let st = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x48:rate=24:duration=4",
            "-c:v",
            "mpeg4",
            path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(st.success());
    let mut s = session();
    let src = format!("file:{}", path.display());
    let near = dispatch(
        &mut s,
        "diff_frames",
        &json!({"a":{"source":src,"time":0},"b":{"source":src,"time":1./24.}}),
    )
    .unwrap();
    let far = dispatch(
        &mut s,
        "diff_frames",
        &json!({"a":{"source":src,"time":0},"b":{"source":src,"time":2.5}}),
    )
    .unwrap();
    assert!(far["metrics"]["mse"].as_f64().unwrap() > near["metrics"]["mse"].as_f64().unwrap());
    let e = kimo_core::commands::dispatch_ex(
        &mut s,
        "diff_frames",
        &json!({"a":{},"b":{"source":src}}),
    )
    .unwrap_err();
    assert_eq!(e.code, "size_mismatch");
    assert!(e.message.contains("32x24") && e.message.contains("64x48"));
    std::fs::remove_file(path).unwrap();
}
