use kimo_core::perception::audio_events::{analyze, Params, SR};
fn train(bpm: f64, seconds: f64) -> (Vec<f32>, Vec<f64>) {
    let mut x = vec![0.; (seconds * SR as f64) as usize];
    let mut t = 0.25;
    let mut times = vec![];
    while t < seconds - 0.1 {
        let i = (t * SR as f64).round() as usize;
        x[i] = 0.9;
        times.push(i as f64 / SR as f64);
        t += 60. / bpm;
    }
    (x, times)
}
#[test]
fn click_train_all_hits_within_two_ms_no_misses_or_false_positives() {
    let (x, t) = train(120., 8.);
    let a = analyze(&x, &Params::default()).unwrap();
    eprintln!(
        "click times {:?}",
        a.transients.iter().map(|e| e.t).collect::<Vec<_>>()
    );
    assert_eq!(a.count, t.len());
    for (e, t) in a.transients.iter().zip(t) {
        assert!((e.t - t).abs() < 0.002);
    }
}
#[test]
fn beats_90_120_150_and_60_octave_rule() {
    for b in [60., 90., 120., 150.] {
        let (x, _) = train(b, 12.);
        let mut p = Params::default();
        p.mode = "beat".into();
        let a = analyze(&x, &p).unwrap();
        eprintln!("tempo {b}: {:?} conf {}", a.bpm, a.confidence);
        assert!((a.bpm.unwrap() - b).abs() < 0.5);
        assert!(a.confidence > 0.5);
        assert!(!a.beat_grid.is_empty());
        if b == 60. {
            assert_eq!(a.alt_bpm, Some(120.));
        }
    }
}
#[test]
fn silence_noise_min_gap_sensitivity_and_repeat() {
    let z = vec![0.; SR];
    assert_eq!(analyze(&z, &Params::default()).unwrap().count, 0);
    let mut noise = vec![0.; SR];
    let mut n = 1u64;
    for x in &mut noise {
        n = n.wrapping_mul(6364136223846793005).wrapping_add(1);
        *x = ((n >> 32) as u32 as f64 / u32::MAX as f64 * 2. - 1.) as f32;
    }
    let mut prior = 0;
    for sensitivity in [0., 0.25, 0.5, 0.75, 1.] {
        let mut p = Params::default();
        p.sensitivity = sensitivity;
        let a = analyze(&noise, &p).unwrap();
        assert!(
            a.count >= prior,
            "counts not monotonic {prior}->{}",
            a.count
        );
        prior = a.count;
        for w in a.transients.windows(2) {
            assert!(w[1].t - w[0].t >= 0.08 - 1e-12);
        }
        assert_eq!(
            serde_json::to_vec(&a).unwrap(),
            serde_json::to_vec(&analyze(&noise, &p).unwrap()).unwrap()
        );
        assert!(a.count <= 13);
    }
}
#[test]
fn pad_no_onsets_kick_snare_plus_pad_within_five_ms() {
    let mut x = vec![0.; SR * 4];
    for (i, v) in x.iter_mut().enumerate() {
        *v = (std::f64::consts::TAU * 220. * i as f64 / SR as f64).sin() as f32 * 0.03;
    }
    let pad = analyze(&x, &Params::default()).unwrap();
    eprintln!("pad count {}", pad.count);
    assert_eq!(pad.count, 0);
    for t in [0.5, 1.5, 2.5] {
        let start = (t * SR as f64) as usize;
        for j in 0..4800 {
            let s = j as f64 / SR as f64;
            x[start + j] +=
                (0.7 * (-s * 50.).exp() * (std::f64::consts::TAU * 80. * s).cos()) as f32;
        }
    }
    let a = analyze(&x, &Params::default()).unwrap();
    eprintln!(
        "kick {:?}",
        a.transients.iter().map(|e| e.t).collect::<Vec<_>>()
    );
    assert_eq!(a.count, 3);
    for (e, t) in a.transients.iter().zip([0.5, 1.5, 2.5]) {
        assert!((e.t - t).abs() < 0.005);
    }
}
#[test]
fn nonperiodic_no_confident_grid() {
    let mut x = vec![0.; SR * 8];
    for t in [0.3, 0.77, 1.9, 2.14, 3.87, 5.12, 6.9] {
        x[(t * SR as f64) as usize] = 1.;
    }
    let mut p = Params::default();
    p.mode = "beat".into();
    let a = analyze(&x, &p).unwrap();
    assert!(a.confidence <= 0.5);
    assert!(a.beat_grid.is_empty());
}
#[test]
fn min_gap_parameter_is_effective() {
    let (x, _) = train(600., 3.);
    let mut p = Params::default();
    p.min_gap_ms = 200.;
    let a = analyze(&x, &p).unwrap();
    assert!(a.count < 28);
    for w in a.transients.windows(2) {
        assert!(w[1].t - w[0].t >= 0.2 - 1e-12);
    }
}
#[test]
fn fixed_order_one_vs_four_threads() {
    let (x, _) = train(120., 5.);
    kimo_core::par::set_threads(1);
    let a = analyze(&x, &Params::default()).unwrap();
    kimo_core::par::set_threads(4);
    let b = analyze(&x, &Params::default()).unwrap();
    assert_eq!(
        serde_json::to_vec(&a).unwrap(),
        serde_json::to_vec(&b).unwrap()
    );
    kimo_core::par::set_threads(0);
}
#[test]
fn ninety_second_benchmark() {
    let (x, _) = train(120., 90.);
    let t = std::time::Instant::now();
    let a = analyze(&x, &Params::default()).unwrap();
    eprintln!(
        "90s MONO algorithm {:.3}s {} events",
        t.elapsed().as_secs_f64(),
        a.count
    );
    if !cfg!(debug_assertions) {
        assert!(t.elapsed().as_secs_f64() <= 1.5);
    }
}
#[test]
fn snare_with_steady_pad() {
    let mut x = vec![0.; SR * 4];
    for (i, v) in x.iter_mut().enumerate() {
        *v = (std::f64::consts::TAU * 220. * i as f64 / SR as f64).sin() as f32 * 0.02;
    }
    let mut state = 9u64;
    for t in [0.5, 1.5, 2.5] {
        let start = (t * SR as f64) as usize;
        for j in 0..3000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let n = (state >> 32) as u32 as f64 / u32::MAX as f64 * 2. - 1.;
            x[start + j] += (n * 0.8 * (-(j as f64) / 400.).exp()) as f32;
        }
    }
    let a = analyze(&x, &Params::default()).unwrap();
    assert_eq!(a.count, 3);
    for (e, t) in a.transients.iter().zip([0.5, 1.5, 2.5]) {
        assert!((e.t - t).abs() < 0.005, "{}", e.t);
    }
}
#[test]
fn tool_path_mp4_clip_track_mix_fps_repeat_and_errors() {
    use kimo_core::commands::{dispatch, dispatch_ex, Session};
    use serde_json::json;
    let (x, _) = train(120., 3.);
    let stereo = x.iter().flat_map(|v| [*v, *v]).collect::<Vec<_>>();
    let path = std::env::temp_dir().join(format!("transients-{}.wav", std::process::id()));
    kimo_core::audio::write_wav(path.to_str().unwrap(), &stereo).unwrap();
    let mp4 = path.with_extension("mp4");
    assert!(std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=s=16x16:r=24:d=3",
            "-i",
            path.to_str().unwrap(),
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            "-shortest",
            mp4.to_str().unwrap()
        ])
        .status()
        .unwrap()
        .success());
    let mut s = Session::new();
    let call = |s: &mut Session, n: &str, a: serde_json::Value| dispatch(s, n, &a).unwrap();
    let asset = call(&mut s, "import_media", json!({"path":path}))["asset_id"].clone();
    let track = call(&mut s, "add_track", json!({}))["track_id"].clone();
    let clip = call(
        &mut s,
        "add_clip",
        json!({"track_id":track,"asset_id":asset,"source_type":"media","start":2,"duration":3}),
    )["clip_id"]
        .clone();
    for fps in [24., 25., 30., 29.97, 60.] {
        call(&mut s, "project_set", json!({"fps":fps}));
        let before = serde_json::to_vec(&s.project).unwrap();
        for target in [
            json!(path),
            json!(mp4),
            clip.clone(),
            track.clone(),
            json!("mix"),
        ] {
            let a = json!({"target":target,"image":"none"});
            let r = call(&mut s, "audio_transients", a.clone());
            assert_eq!(r["count"], 6);
            assert_eq!(r, call(&mut s, "audio_transients", a));
            for e in r["transients"].as_array().unwrap() {
                let t = e["t"].as_f64().unwrap();
                let snap = e["nearest_frame_time"].as_f64().unwrap();
                assert!((snap * fps - (snap * fps).round()).abs() < 1e-9);
                assert!((e["frame_offset_ms"].as_f64().unwrap() - (snap - t) * 1000.).abs() < 1e-9);
                assert!((snap - t).abs() <= 0.5 / fps + 1e-9);
            }
        }
        assert_eq!(serde_json::to_vec(&s.project).unwrap(), before);
    }
    let r = call(
        &mut s,
        "audio_transients",
        json!({"target":path,"image":"waveform"}),
    );
    assert!(r["waveform"]["svg"].as_str().unwrap().starts_with("<svg"));
    for (a, code) in [
        (json!({"target":path,"start":8}), "range_empty"),
        (
            json!({"target":path,"start":0,"end":0.01}),
            "insufficient_data",
        ),
        (
            json!({"target":"/nonexistent/transients.wav"}),
            "decode_failed",
        ),
    ] {
        assert_eq!(
            dispatch_ex(&mut s, "audio_transients", &a)
                .unwrap_err()
                .code,
            code
        );
    }
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(mp4).unwrap();
}
#[test]
fn cancellation_and_malformed_fuzz_zero_panics() {
    let (x, _) = train(120., 1.);
    let token = kimo_core::guard::new_token();
    token.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(
        std::panic::catch_unwind(|| kimo_core::guard::with_token(&token, || analyze(
            &x,
            &Params::default()
        )))
        .is_err()
    );
    let mut seed = 1u64;
    for _ in 0..1000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut p = Params::default();
        p.sensitivity = (seed % 300) as f64 / 100. - 1.;
        p.min_gap_ms = (seed % 4000) as f64;
        p.band = [(seed % 30000) as f64, ((seed >> 16) % 30000) as f64];
        assert!(std::panic::catch_unwind(|| analyze(&x, &p)).is_ok());
    }
    let mut p = Params::default();
    p.band = [20., 21.];
    assert!(analyze(&x, &p).is_err());
    assert!(analyze(&[f32::NAN; 1024], &Params::default()).is_err());
}
#[test]
fn truncation_and_bands() {
    let (x, _) = train(120., 3.);
    let mut p = Params::default();
    p.max_items = 2;
    let a = analyze(&x, &p).unwrap();
    assert_eq!(a.count, 6);
    assert!(a.truncated);
    assert_eq!(a.transients.len(), 2);
    for band in [[20., 250.], [250., 4000.], [4000., 24000.]] {
        p.band = band;
        let a = analyze(&x, &p).unwrap();
        assert_eq!(a.count, 6);
    }
}
#[test]
fn full_tool_ninety_second_stereo_mix_benchmark() {
    use kimo_core::commands::{dispatch, Session};
    use serde_json::json;
    let (x, _) = train(120., 90.);
    let stereo = x.iter().flat_map(|v| [*v, *v]).collect::<Vec<_>>();
    let path = std::env::temp_dir().join(format!("transients90-{}.wav", std::process::id()));
    kimo_core::audio::write_wav(path.to_str().unwrap(), &stereo).unwrap();
    let mut s = Session::new();
    let asset =
        dispatch(&mut s, "import_media", &json!({"path":path})).unwrap()["asset_id"].clone();
    let track = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    dispatch(
        &mut s,
        "add_clip",
        &json!({"track_id":track,"asset_id":asset,"source_type":"media","start":0,"duration":90}),
    )
    .unwrap();
    let start = std::time::Instant::now();
    let r = dispatch(&mut s, "audio_transients", &json!({"target":"mix"})).unwrap();
    let elapsed = start.elapsed().as_secs_f64();
    eprintln!(
        "90s stereo MIX whole-tool {elapsed:.6}s count={}",
        r["count"]
    );
    assert_eq!(r["count"], 180);
    if !cfg!(debug_assertions) {
        assert!(elapsed <= 1.5, "90s mix takes {elapsed}");
    }
    std::fs::remove_file(path).unwrap();
}
