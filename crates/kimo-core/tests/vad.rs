use kimo_core::perception::vad::*;
fn signal(duration: f64, floor_db: f64, bursts: &[(f64, f64)], speech_db: f64) -> Vec<f32> {
    let mut rng = 19u64;
    let mut out = Vec::new();
    let gain = 10f64.powf(speech_db / 20.);
    let noise = 10f64.powf(floor_db / 20.) * 3f64.sqrt();
    for i in 0..(duration * 48000.) as usize {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let n = (rng as f64 / u64::MAX as f64 * 2. - 1.) * noise;
        let t = i as f64 / 48000.;
        let x = if bursts.iter().any(|(a, b)| t >= *a && t < *b) {
            let voice = (2. * std::f64::consts::PI * 440. * t).sin()
                + 0.6 * (2. * std::f64::consts::PI * 900. * t).sin()
                + 0.4 * (2. * std::f64::consts::PI * 1700. * t).sin();
            voice * gain * (0.7 + 0.3 * (2. * std::f64::consts::PI * 9. * t).sin())
        } else {
            0.
        };
        let v = (x + n) as f32;
        out.extend([v, v]);
    }
    out
}
fn bytes(r: &ResultVad) -> Vec<u8> {
    serde_json::to_vec(r).unwrap()
}
#[test]
fn four_bursts_boundaries_and_breathy() {
    let bursts = [(1., 1.6), (2.5, 3.1), (4., 4.8), (6., 6.5)];
    for level in [-25., -38.] {
        let x = signal(8., -50., &bursts, level);
        let p = Params::default();
        let r = analyze(&x, &p).unwrap();
        assert_eq!(r.speech.len(), 4, "{level} {r:?}");
        for (s, (a, b)) in r.speech.iter().zip(bursts) {
            assert!((s.start - (a - 0.12)).abs() <= 0.06, "{s:?}");
            assert!((s.end - (b + 0.32)).abs() <= 0.06, "{s:?}");
        }
    }
}
#[test]
fn pure_noise_tones_and_silent() {
    for db in [-60., -50., -40., -30.] {
        let x = signal(3., db, &[], 0.);
        for ag in 0..4 {
            let mut p = Params::default();
            p.aggressiveness = ag;
            assert!(
                analyze(&x, &p).unwrap().speech.is_empty(),
                "noise db {db} ag {ag}"
            );
        }
    }
    for ag in 2..4 {
        let mut p = Params::default();
        p.aggressiveness = ag;
        let mut x = signal(3., -60., &[], 0.);
        for (i, c) in x.chunks_mut(2).enumerate() {
            if i >= 48000 && i < 96000 {
                let v = (2. * std::f64::consts::PI * 700. * i as f64 / 48000.).sin() as f32 * 0.1;
                c.fill(v)
            }
        }
        assert!(analyze(&x, &p).unwrap().speech.is_empty());
    }
    let r = analyze(&vec![0.; 96000], &Params::default()).unwrap();
    assert!(r.speech.is_empty());
    assert!(r.noise_floor_db.is_finite());
    assert_eq!(r.speech_ratio, 0.);
    assert_eq!(r.count, 0);
    assert!(!r.truncated);
    serde_json::to_string(&r).unwrap();
}
#[test]
fn short_click_gap_hangover_and_monotonic() {
    let mut p = Params::default();
    let x = signal(5., -50., &[(1., 1.1)], -20.);
    assert!(analyze(&x, &p).unwrap().speech.is_empty());
    let x = signal(5., -50., &[(1., 1.6), (1.75, 2.4)], -25.);
    assert_eq!(analyze(&x, &p).unwrap().speech.len(), 1);
    let x = signal(5., -50., &[(1., 1.6), (3., 3.6)], -25.);
    p.pad_ms = 0.;
    p.hangover_ms = 0.;
    let a = analyze(&x, &p).unwrap();
    p.hangover_ms = 200.;
    let b = analyze(&x, &p).unwrap();
    assert!((b.speech[0].end - a.speech[0].end - 0.2).abs() < 1e-9);
    let mut prev = 1.;
    for ag in 0..4 {
        p.aggressiveness = ag;
        let r = analyze(&x, &p).unwrap();
        assert!(r.speech_ratio <= prev);
        prev = r.speech_ratio;
    }
}
#[test]
fn cuts_do_not_intersect_speech_200_fixtures() {
    for seed in 0..200 {
        let a = 0.2 + seed as f64 * 0.003;
        let x = signal(2., -50., &[(a, a + 0.3), (1.1, 1.4)], -25.);
        let mut p = Params::default();
        p.pad_ms = 100. + (seed % 51) as f64;
        let r = analyze(&x, &p).unwrap();
        for c in &r.cut_suggestions {
            for s in &r.speech {
                assert!(c.end <= s.start || c.start >= s.end)
            }
            assert!(c.remove_s >= p.min_silence_ms / 1000.);
        }
        assert!((r.speech[0].start - (a - p.pad_ms / 1000.)).abs() < 0.061);
    }
}
#[test]
fn fixed_thread_reductions_repeat_and_cancel() {
    let x = signal(2., -50., &[(0.5, 1.2)], -25.);
    kimo_core::par::set_threads(1);
    let a = bytes(&analyze(&x, &Params::default()).unwrap());
    kimo_core::par::set_threads(4);
    assert_eq!(a, bytes(&analyze(&x, &Params::default()).unwrap()));
    assert_eq!(a, bytes(&analyze(&x, &Params::default()).unwrap()));
    kimo_core::par::set_threads(0);
    let tok = kimo_core::guard::new_token();
    tok.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(
        std::panic::catch_unwind(|| kimo_core::guard::with_token(&tok, || analyze(
            &x,
            &Params::default()
        )))
        .is_err()
    );
}
#[test]
fn fuzz_invalid_pcm_and_params() {
    for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(analyze(&[v; 12], &Params::default()).is_err());
    }
    assert!(analyze(&[], &Params::default()).is_err());
    assert!(analyze(&[0.; 7], &Params::default()).is_err());
    for i in 0..1000 {
        let mut p = Params::default();
        p.pad_ms = if i % 2 == 0 { -1. } else { f64::NAN };
        assert!(analyze(&[0.; 120], &p).is_err());
    }
}
#[test]
fn silence_windows_exact() {
    let x = signal(3., -100., &[(1., 2.)], -20.);
    let mut p = Params::default();
    p.mode = "silence".into();
    let r = analyze(&x, &p).unwrap();
    assert_eq!(
        serde_json::to_value(&r.silence).unwrap(),
        serde_json::to_value(silence_probe(&x, -60., 300.)).unwrap()
    );
}
#[test]
fn release_speed_90s() {
    let x = signal(90., -50., &[(5., 6.), (40., 42.), (70., 73.)], -25.);
    let t = std::time::Instant::now();
    let r = analyze(&x, &Params::default()).unwrap();
    eprintln!(
        "VAD_90S {}s speech {}",
        t.elapsed().as_secs_f64(),
        r.speech.len()
    );
    if cfg!(not(debug_assertions)) {
        assert!(t.elapsed().as_secs_f64() < 1.);
    }
}
#[test]
fn tone_low_aggressiveness_fp_rate_report() {
    let mut x = signal(3., -60., &[], 0.);
    for (i, c) in x.chunks_mut(2).enumerate() {
        if (48000..96000).contains(&i) {
            c.fill((2. * std::f64::consts::PI * 700. * i as f64 / 48000.).sin() as f32 * 0.1)
        }
    }
    for ag in 0..4 {
        let mut p = Params::default();
        p.aggressiveness = ag;
        let r = analyze(&x, &p).unwrap();
        eprintln!(
            "TONE_FP ag={ag} intervals={} ratio={}",
            r.count, r.speech_ratio
        );
    }
}
#[test]
fn tool_targets_params_fuzz_and_image() {
    use kimo_core::commands::{dispatch, Session};
    use serde_json::json;
    let path = std::env::temp_dir().join(format!("vad-input-{}.wav", std::process::id()));
    kimo_core::audio::write_wav(
        path.to_str().unwrap(),
        &signal(2., -50., &[(0.5, 1.2)], -25.),
    )
    .unwrap();
    let mut s = Session::new();
    let a = dispatch(&mut s, "import_media", &json!({"path":path})).unwrap()["asset_id"].clone();
    let tr = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    let c = dispatch(
        &mut s,
        "add_clip",
        &json!({"track_id":tr,"source_type":"media","asset_id":a,"start":0,"duration":2}),
    )
    .unwrap()["clip_id"]
        .clone();
    for target in [
        json!({"clip_id":c}),
        json!({"track_id":tr}),
        json!("mix"),
        json!({"path":path}),
        json!(path),
    ] {
        let args = json!({"target":target,"start":0.1,"end":1.9});
        let r = dispatch(&mut s, "audio_speech_intervals", &args).unwrap();
        assert_eq!(
            r,
            dispatch(&mut s, "audio_speech_intervals", &args).unwrap()
        );
        assert_eq!(r["params"]["target"], target);
        assert!(r["speech"][0]["start"].as_f64().unwrap() >= 0.1);
        let args = json!({"target":target,"mode":"silence"});
        let r = dispatch(&mut s, "audio_speech_intervals", &args).unwrap();
        assert_eq!(r["method"], "audio_probe_rms_silence_v1");
    }
    let a = dispatch(&mut s, "audio_probe", &json!({"target":{"clip_id":c}})).unwrap();
    let b = dispatch(
        &mut s,
        "audio_speech_intervals",
        &json!({"target":{"clip_id":c},"mode":"silence"}),
    )
    .unwrap();
    assert_eq!(a["silence"], b["silence"]);
    let args = json!({"target":{"clip_id":c},"image":"waveform"});
    let r = dispatch(&mut s, "audio_speech_intervals", &args).unwrap();
    assert_eq!(
        r,
        dispatch(&mut s, "audio_speech_intervals", &args).unwrap()
    );
    std::fs::copy(
        r["waveform"]["path"].as_str().unwrap(),
        std::env::temp_dir().join("vad-waveform-proof.png"),
    )
    .unwrap();
    for key in [
        "aggressiveness",
        "min_speech_ms",
        "min_silence_ms",
        "pad_ms",
        "hangover_ms",
        "merge_gap_ms",
        "threshold_db",
        "max_items",
        "start",
        "end",
    ] {
        for v in [
            json!(-1e300),
            json!(1e300),
            json!("junk"),
            json!([]),
            json!(null),
        ] {
            let mut a = json!({"target":{"clip_id":c}});
            a[key] = v;
            let before = serde_json::to_value(&s.project).unwrap();
            let _ = dispatch(&mut s, "audio_speech_intervals", &a);
            assert_eq!(before, serde_json::to_value(&s.project).unwrap());
        }
    }
    std::fs::remove_file(path).unwrap();
}
#[test]
fn aggressiveness_monotonic_200_random_fixtures() {
    for seed in 0..200 {
        let x = signal(
            2.,
            -50.,
            &[(0.3, 0.7), (1., 1.4)],
            -45. + (seed % 30) as f64,
        );
        let mut prev = 1.;
        for ag in 0..4 {
            let mut p = Params::default();
            p.aggressiveness = ag;
            let r = analyze(&x, &p).unwrap();
            assert!(r.speech_ratio <= prev + 1e-9, "seed {seed} ag {ag}");
            prev = r.speech_ratio;
        }
    }
}
// Harmonic voiced source filtered by Gaussian formant envelopes, interrupted by
// closure + deterministic broadband release. This is synthetic coverage, not voice recall.
fn vowel_plosives(formants: [f64; 3], db: f64) -> Vec<f32> {
    let mut x = signal(4., -55., &[], 0.);
    let gain = 10f64.powf(db / 20.);
    let mut rng = 41u64;
    for (i, c) in x.chunks_mut(2).enumerate() {
        let t = i as f64 / 48000.;
        if !(0.7..2.8).contains(&t) {
            continue;
        }
        let local = t - 0.7;
        let phase = local % 0.52;
        let mut v = 0.;
        if phase >= 0.045 {
            for h in 1..=30 {
                let freq = h as f64 * 120.;
                let amp = formants
                    .iter()
                    .map(|f| (-0.5 * ((freq - f) / 150.).powi(2)).exp())
                    .sum::<f64>()
                    / h as f64;
                v += amp * (2. * std::f64::consts::PI * freq * t).sin()
            }
            v *= gain * 5.;
        } else if phase >= 0.035 {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            v = (rng as f64 / u64::MAX as f64 * 2. - 1.) * gain;
        }
        for y in c {
            *y += v as f32
        }
    }
    x
}
#[test]
fn vowel_formant_plosive_dropouts() {
    for formants in [
        [700., 1200., 2500.],
        [300., 2200., 3000.],
        [400., 900., 2200.],
    ] {
        for db in [-25., -38.] {
            let x = vowel_plosives(formants, db);
            let r = analyze(&x, &Params::default()).unwrap();
            assert_eq!(r.speech.len(), 1, "{formants:?} db={db} {r:?}");
            assert!(r.speech[0].start <= 0.78, "late onset {r:?}");
            assert!(r.speech[0].end >= 2.75, "lost vowel tail {r:?}");
            for cut in &r.cut_suggestions {
                assert!(
                    cut.end <= 0.7 || cut.start >= 2.8,
                    "plosive became cut {r:?}"
                )
            }
        }
    }
}
#[test]
fn timebase_fractional_fps_and_samples() {
    use kimo_core::perception::timebase::*;
    use serde_json::json;
    for fps in [24., 25., 30., 24000. / 1001., 30000. / 1001.] {
        for n in [1, 12347, 987654321] {
            let mut v = json!({"start":n as f64/48000.,"end":(n+10001) as f64/48000.});
            annotate(&mut v, fps, true);
            assert_eq!(v["start_sample"], n);
            assert_eq!(v["sample_count"], 10001);
            let f = &v["frame_aligned"];
            assert!(f["start_sample"].as_u64().unwrap() >= n);
            assert!(f["end_sample"].as_u64().unwrap() <= n + 10001);
            assert_eq!(
                f["start_sample"],
                frame_sample(f["start_frame"].as_u64().unwrap(), fps)
            );
        }
    }
}
#[test]
fn speech_outward_silence_inward_no_frame_double_owned() {
    use kimo_core::perception::timebase::*;
    use serde_json::json;
    for fps in [24., 30., 60.] {
        let mut s = json!({"start":1.0003,"end":2.5001});
        annotate(&mut s, fps, false);
        let mut left = json!({"start":0.,"end":1.0003});
        let mut right = json!({"start":2.5001,"end":4.});
        annotate(&mut left, fps, true);
        annotate(&mut right, fps, true);
        let a = &s["frame_aligned"];
        assert!(a["start_sample"].as_u64().unwrap() <= s["start_sample"].as_u64().unwrap());
        assert!(a["end_sample"].as_u64().unwrap() >= s["end_sample"].as_u64().unwrap());
        assert!(
            left["frame_aligned"]["end_frame"].as_u64().unwrap()
                <= a["start_frame"].as_u64().unwrap()
        );
        assert!(
            right["frame_aligned"]["start_frame"].as_u64().unwrap()
                >= a["end_frame"].as_u64().unwrap()
        );
        for v in [&s, &left, &right] {
            assert_eq!(
                v["sample_count"].as_u64().unwrap(),
                v["end_sample"].as_u64().unwrap() - v["start_sample"].as_u64().unwrap()
            )
        }
    }
}
#[test]
fn tools_exact_odd_i0_and_probe_equal() {
    use kimo_core::commands::{dispatch, Session};
    use serde_json::json;
    let path = std::env::temp_dir().join(format!("vad-odd-time-{}.wav", std::process::id()));
    kimo_core::audio::write_wav(
        path.to_str().unwrap(),
        &signal(3., -100., &[(1., 2.)], -20.),
    )
    .unwrap();
    let mut s = Session::new();
    s.project.fps = 30000. / 1001.;
    let id = dispatch(&mut s, "import_media", &json!({"path":path})).unwrap()["asset_id"].clone();
    let tr = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    let c = dispatch(
        &mut s,
        "add_clip",
        &json!({"track_id":tr,"source_type":"media","asset_id":id,"start":0.,"duration":3.}),
    )
    .unwrap()["clip_id"]
        .clone();
    let requested = 12347.4 / 48000.;
    let a = dispatch(
        &mut s,
        "audio_probe",
        &json!({"target":{"clip_id":c},"start":requested,"end":2.8}),
    )
    .unwrap();
    let b = dispatch(
        &mut s,
        "audio_speech_intervals",
        &json!({"target":{"clip_id":c},"start":requested,"end":2.8,"mode":"silence"}),
    )
    .unwrap();
    assert_eq!(a["silence"], b["silence"]);
    assert_eq!(a["range_samples"], b["range_samples"]);
    assert_eq!(a["range_samples"]["start_sample"], 12347);
    assert_eq!(a["meta"]["range"]["start"], 12347. / 48000.);
    assert_eq!(b["params"]["start"], 12347. / 48000.);
    assert_eq!(a["envelope"]["t0_sample"], 12347);
    assert_eq!(a["timebase"], b["timebase"]);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn transient_attack_tail_frame_safety_fuzz() {
    use kimo_core::perception::timebase::*;
    use serde_json::json;
    for seed in 0..24 {
        let fps = [24., 30., 60., 30000. / 1001.][seed % 4];
        let onset_frame = 16 + (seed % 5) as u64;
        let onset = frame_sample(onset_frame, fps) as f64 / 48000.;
        let end = onset + 0.92;
        let tail_end = end + 0.18;
        let mut x = signal(3., -65., &[(onset + 0.012, end)], -25.);
        let mut rng = 17u64;
        for (i, c) in x.chunks_mut(2).enumerate() {
            let t = i as f64 / 48000.;
            let v = if t >= onset && t < onset + 0.012 {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                (rng as f64 / u64::MAX as f64 * 2. - 1.) * 0.09
            } else if t >= end && t < tail_end {
                0.02 * (-18. * (t - end)).exp()
                    * ((2. * std::f64::consts::PI * 440. * t).sin()
                        + 0.6 * (2. * std::f64::consts::PI * 900. * t).sin())
            } else {
                0.
            };
            for y in c {
                *y += v as f32
            }
        }
        let r = analyze(&x, &Params::default()).unwrap();
        assert_eq!(r.speech.len(), 1, "seed={seed} {r:?}");
        let speech = &r.speech[0];
        assert!(
            speech.start <= onset && speech.end >= tail_end,
            "attack/tail clipped seed={seed} {r:?}"
        );
        let mut s = json!({"start":speech.start,"end":speech.end});
        annotate(&mut s, fps, false);
        assert!(s["frame_aligned"]["start_sample"].as_u64().unwrap() <= sample(onset));
        assert!(s["frame_aligned"]["end_sample"].as_u64().unwrap() >= sample(tail_end));
        for cut in r.cut_suggestions {
            let mut c = json!({"start":cut.start,"end":cut.end});
            annotate(&mut c, fps, true);
            let f = &c["frame_aligned"];
            if f["usable"] == true {
                assert!(
                    f["end_sample"].as_u64().unwrap() <= sample(onset)
                        || f["start_sample"].as_u64().unwrap() >= sample(tail_end),
                    "transient cut seed={seed} {c}"
                )
            }
        }
    }
}
