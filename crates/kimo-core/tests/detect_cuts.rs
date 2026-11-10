use kimo_core::{
    commands::{dispatch, Session},
    perception::{analysis::Image, cuts},
};
use serde_json::{json, Value};
fn image(v: f64) -> Image {
    let mut im = Image::new(16, 16);
    for c in 0..3 {
        im.rgb[c].fill(v);
    }
    im
}
fn fixture() -> Session {
    let mut s = Session::new();
    dispatch(
        &mut s,
        "project_set",
        &json!({"width":32,"height":24,"fps":24}),
    )
    .unwrap();
    let tr = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    for (i, c) in [[0., 0., 0.], [1., 1., 1.], [1., 0., 0.]]
        .iter()
        .enumerate()
    {
        dispatch(
            &mut s,
            "add_clip",
            &json!({"track_id":tr,"source_type":"solid","color":c,"start":i*2,"duration":2}),
        )
        .unwrap();
    }
    s
}
#[test]
fn solids_refine_exact_static_scores_purity_threads() {
    let mut s = fixture();
    let before = serde_json::to_string(&s.project).unwrap();
    let mut first = None;
    for threads in [1, 4, 1] {
        kimo_core::par::set_threads(threads);
        let r = dispatch(&mut s, "detect_cuts", &json!({"return_scores":true})).unwrap();
        assert_eq!(r["count"], 2);
        for (i, t) in [2., 4.].iter().enumerate() {
            assert_eq!(r["cuts"][i]["time"], *t);
            assert_eq!(r["cuts"][i]["kind"], "hard_cut");
            assert!(r["cuts"][i]["confidence"].as_f64().unwrap() > 0.8);
        }
        assert_eq!(r["scores"][24]["score"], 1.);
        let b = serde_json::to_string(&r).unwrap();
        if let Some(f) = &first {
            assert_eq!(&b, f);
        } else {
            first = Some(b);
        }
        assert_eq!(before, serde_json::to_string(&s.project).unwrap());
    }
    assert_eq!(
        dispatch(&mut s, "detect_cuts", &json!({"start":0,"end":1})).unwrap()["count"],
        0
    );
}
#[test]
fn dissolve_flash_pan_gap() {
    let mut frames = vec![image(0.); 12];
    for i in 1..=12 {
        frames.push(image(i as f64 / 12.));
    }
    frames.extend(vec![image(1.); 12]);
    let (c, _) = cuts::detect(&frames, "combined", 0.35, 3, true);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].kind, "dissolve");
    assert!(c[0].end.unwrap() > c[0].start.unwrap());
    let mut f = vec![image(0.); 20];
    f[8] = image(1.);
    let (c, _) = cuts::detect(&f, "combined", 0.35, 3, true);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].kind, "flash");
    let f: Vec<_> = (0..30).map(|i| image(i as f64 / 300.)).collect();
    assert_eq!(cuts::detect(&f, "luma", 0.35, 3, true).0.len(), 0);
    let f = vec![image(0.), image(0.), image(0.2), image(1.), image(1.)];
    let (c, _) = cuts::detect(&f, "luma", 0.1, 4, false);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].index, 3);
}
#[test]
fn override_errors_fuzz_cap() {
    let mut s = fixture();
    let r = dispatch(
        &mut s,
        "detect_cuts",
        &json!({"threshold":0.99,"sensitivity":1}),
    )
    .unwrap();
    assert_eq!(r["params_used"]["threshold"], 0.99);
    for (a, code) in [
        (json!({"source":"bad"}), "invalid_source"),
        (json!({"method":"bad"}), "invalid_method"),
        (json!({"fps":25}), "out_of_range"),
    ] {
        assert!(dispatch(&mut s, "detect_cuts", &a)
            .unwrap_err()
            .starts_with(code));
    }
    s.project.tracks[0].clips[0] = std::sync::Arc::new({
        let mut c = (*s.project.tracks[0].clips[0]).clone();
        c.duration = 2000.;
        c
    });
    assert!(
        dispatch(&mut s, "detect_cuts", &json!({"end":2000,"fps":24}))
            .unwrap_err()
            .starts_with("too_many_frames")
    );
    for i in 0..1000 {
        let a = json!({"fps":if i%2==0{Value::Null}else{json!(-1)}});
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dispatch(
                &mut s,
                "detect_cuts",
                &a
            )))
            .is_ok()
        );
    }
}
#[test]
fn real_media_four_boundaries() {
    let path = std::env::temp_dir().join(format!("kimo-cuts-real-{}.mp4", std::process::id()));
    let st = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x48:rate=24:duration=5",
            "-c:v",
            "mpeg4",
            path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(st.success());
    let mut s = Session::new();
    dispatch(
        &mut s,
        "project_set",
        &json!({"width":64,"height":48,"fps":24}),
    )
    .unwrap();
    let asset =
        dispatch(&mut s, "import_media", &json!({"path":path})).unwrap()["asset_id"].clone();
    let tr = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    for i in 0..5 {
        let c=dispatch(&mut s,"add_clip",&json!({"track_id":tr,"source_type":"media","asset_id":asset,"start":i,"duration":1,"in_point":i,"fit":"stretch"})).unwrap()["clip_id"].clone();
        dispatch(
            &mut s,
            "set_property",
            &json!({"clip_id":c,"property":"opacity","value":if i%2==0{0.1}else{1.}}),
        )
        .unwrap();
    }
    let r = dispatch(
        &mut s,
        "detect_cuts",
        &json!({"method":"luma","threshold":0.2}),
    )
    .unwrap();
    assert_eq!(r["count"], 4, "{r}");
    for (i, c) in r["cuts"].as_array().unwrap().iter().enumerate() {
        assert!((c["time"].as_f64().unwrap() - (i + 1) as f64).abs() <= 1. / 24.);
    }
}
#[test]
fn translating_camera_pan_no_false_cuts() {
    let mut f = vec![];
    for t in 0..48 {
        let mut im = Image::new(64, 32);
        for y in 0..32 {
            for x in 0..64 {
                let v = 0.3 + 0.05 * (((x + t) as f64 / 64.) * std::f64::consts::TAU).sin();
                for c in 0..3 {
                    im.rgb[c][y * 64 + x] = v;
                }
            }
        }
        f.push(im);
    }
    assert!(cuts::detect(&f, "combined", 0.35, 3, true).0.is_empty());
}
#[test]
fn dip_one_span_hard_black_stays_two() {
    let mut f = vec![image(1.); 12];
    for i in 1..=12 {
        f.push(image(1. - i as f64 / 12.));
    }
    for i in 1..=12 {
        f.push(image(i as f64 / 12.));
    }
    f.extend(vec![image(1.); 12]);
    let (c, _) = cuts::detect(&f, "combined", 0.35, 3, true);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].kind, "fade");
    assert_eq!(c[0].start, Some(11));
    assert_eq!(c[0].end, Some(35));
    let f = [
        vec![image(1.); 12],
        vec![image(0.); 12],
        vec![image(1.); 12],
    ]
    .concat();
    let (c, _) = cuts::detect(&f, "combined", 0.35, 3, true);
    assert_eq!(c.len(), 2);
    assert!(c.iter().all(|c| c.kind == "hard_cut"));
}
#[test]
fn checker_pan_and_spatial_object_hard_flash() {
    let mut f = vec![];
    for t in 0..48 {
        let mut im = Image::new(64, 32);
        for y in 0..32 {
            for x in 0..64 {
                for ch in 0..3 {
                    im.rgb[ch][y * 64 + x] = if ((x + t * 16) / 16) % 2 == 0 { 0. } else { 1. };
                }
            }
        }
        f.push(im);
    }
    assert!(cuts::detect(&f, "combined", 0.35, 3, true).0.is_empty());
    let a = Image::new(64, 64);
    let mut b = a.clone();
    for c in 0..3 {
        for y in 0..32 {
            for x in 0..32 {
                b.rgb[c][y * 64 + x] = 1.;
            }
        }
    }
    let ratio = kimo_core::perception::analysis::spatial_change(&a, &b, 4, 4, 0.1);
    assert_eq!(ratio, 0.25);
    let hist = vec![a.hist(), a.hist(), b.hist(), b.hist()];
    let means = vec![0., 0., 0.25, 0.25];
    let scores = vec![0., 0., 1., 0.];
    let ratios = vec![0., 0., ratio, 0.];
    assert!(cuts::detect_stats_spatial(
        &scores,
        &means,
        &hist,
        0.1,
        1,
        false,
        Some((&ratios, 0.6))
    )
    .0
    .is_empty());
    assert_eq!(
        cuts::detect_stats_spatial(&scores, &means, &hist, 0.1, 1, false, None)
            .0
            .len(),
        1
    );
    let white = {
        let mut im = a.clone();
        for c in 0..3 {
            im.rgb[c].fill(1.);
        }
        im
    };
    assert_eq!(
        kimo_core::perception::analysis::spatial_change(&a, &white, 4, 4, 0.35),
        1.
    );
    assert_eq!(
        kimo_core::perception::analysis::spatial_change(&a, &white, 1, 1, 0.35),
        1.
    );
    let mut s = fixture();
    let on = dispatch(&mut s, "detect_cuts", &json!({"spatial":{"grid":"4x4"}})).unwrap();
    assert_eq!(on["count"], 2);
}

#[test]
fn fast_whip_pan_is_not_a_hard_cut_but_a_real_cut_still_is() {
    // A deterministic pseudo-random "world" texture wider than the frame, panned across quickly
    // (30px/frame for 8 frames) with a 3-tap directional blur standing in for real motion blur --
    // content shifts continuously across frames, the way a phone's fast whip pan looks. This must
    // not register as a cut. A real hard cut (the world inverted outright, then held static) sits
    // earlier in the same clip and must still be detected.
    fn hashf(x: i64) -> f64 {
        let h = (x as i64).wrapping_mul(2654435761) as u32;
        let h = h ^ (h >> 13);
        let h = h.wrapping_mul(2246822519);
        (h % 1000) as f64 / 999.0
    }
    fn world(x: i64, invert: bool) -> f64 {
        let v = hashf(x);
        if invert { 1. - v } else { v }
    }
    fn frame(w: usize, h: usize, offset: i64, invert: bool) -> Image {
        let mut im = Image::new(w, h);
        for x in 0..w {
            // 3-tap box blur along the pan direction, standing in for real motion blur.
            let v = (world(x as i64 + offset, invert)
                + world(x as i64 + offset + 1, invert)
                + world(x as i64 + offset + 2, invert))
                / 3.;
            for y in 0..h {
                for c in 0..3 {
                    im.rgb[c][y * w + x] = v;
                }
            }
        }
        im
    }
    let (w, h) = (64, 8);
    let mut frames = vec![];
    for _ in 0..10 {
        frames.push(frame(w, h, 0, false));
    } // static shot
    for _ in 0..10 {
        frames.push(frame(w, h, 0, true));
    } // HARD CUT: whole scene inverted, then held
    let pan_offsets = [0, 30, 60, 90, 120, 150, 180, 210, 240];
    for o in pan_offsets {
        frames.push(frame(w, h, o, true));
    } // fast whip pan, same scene
    for _ in 0..10 {
        frames.push(frame(w, h, *pan_offsets.last().unwrap(), true));
    } // settles
    let (cuts, _) = cuts::detect(&frames, "combined", 0.35, 3, true);
    assert_eq!(
        cuts.len(),
        1,
        "the whip pan must not add a cut, only the real one survives: {cuts:?}"
    );
    assert_eq!(cuts[0].kind, "hard_cut");
    assert_eq!(cuts[0].index, 10, "the real cut is still found where the scene actually changed: {cuts:?}");
}
