use kimo_core::perception::audio_raw::*;
#[test]
fn fir_identity_swap_exact_and_unclamped() {
    let s = Samples {
        rate: 48000,
        planes: vec![vec![1.5, -2., 0.125], vec![0.2, 0.3, -0.4]],
    };
    assert_eq!(fir(&s, &[1.]).unwrap().planes, s.planes);
    assert_eq!(
        biquad(
            &s,
            &[Section {
                b: [1., 0., 0.],
                a: [1., 0., 0.]
            }]
        )
        .unwrap()
        .planes,
        s.planes
    );
    let m = mix_matrix(&s, &[vec![0., 1.], vec![1., 0.]]).unwrap();
    assert_eq!(m.planes, vec![s.planes[1].clone(), s.planes[0].clone()]);
    assert_eq!(mix_matrix(&s, &[vec![2., 0.]]).unwrap().planes[0][0], 3.);
}
#[test]
fn response_three_frequencies_analytic() {
    let section = Section {
        b: [0.2, 0.1, 0.05],
        a: [1., -0.5, 0.12],
    };
    for hz in [100., 1000., 10000.] {
        let w = std::f64::consts::TAU * hz / 48000.;
        let x: Vec<f32> = (0..48000).map(|i| (w * i as f64).sin() as f32).collect();
        let s = Samples {
            rate: 48000,
            planes: vec![x.clone()],
        };
        let y = biquad(&s, &[section]).unwrap();
        let energy = |v: &[f32]| v[24000..].iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
        let gain = (energy(&y.planes[0]) / energy(&x)).sqrt();
        let power = |c: [f64; 3]| {
            let re = c[0] + c[1] * w.cos() + c[2] * (2. * w).cos();
            let im = -c[1] * w.sin() - c[2] * (2. * w).sin();
            re * re + im * im
        };
        let expected = (power(section.b) / power(section.a)).sqrt();
        assert!((20. * (gain / expected).log10()).abs() < 0.05);
    }
}
#[test]
fn malformed_fuzz_and_threads() {
    let s = Samples {
        rate: 48000,
        planes: vec![vec![0.1; 1024], vec![0.2; 1024]],
    };
    let section = Section {
        b: [0.2, 0.1, 0.05],
        a: [1., -0.5, 0.12],
    };
    kimo_core::par::set_threads(1);
    let a = biquad(&s, &[section]).unwrap().planes;
    kimo_core::par::set_threads(4);
    assert_eq!(biquad(&s, &[section]).unwrap().planes, a);
    for _ in 0..1000 {
        assert!(fir(&s, &[]).is_err());
        assert!(mix_matrix(&s, &[vec![1.]]).is_err());
        assert!(biquad(
            &s,
            &[Section {
                b: [f64::NAN, 0., 0.],
                a: [0., 0., 0.]
            }]
        )
        .is_err());
    }
}
use kimo_core::commands::{dispatch, dispatch_ex, Session};
use serde_json::json;
/// KIMO_ASSET_DIR is process-global and the asset store (assets::store_dir, audio_samples::store_audio) reads it at import time, so tests running in
/// parallel in this binary must not see each other's value. Every test here that sets it or imports samples (the dependent readers) holds ASSET_ENV for
/// its whole body through AssetDirGuard, which restores the previous value when dropped, including during a panic unwind. The tests that do neither
/// (pure DSP / analytic response) need no lock.
static ASSET_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct AssetDirGuard {
    prev: Option<std::ffi::OsString>,
    _lock: std::sync::MutexGuard<'static, ()>,
}
impl AssetDirGuard {
    fn set(dir: &std::path::Path) -> Self {
        // A panicking test poisons the mutex but its guard has already restored the variable, so the poison carries no bad state.
        let lock = ASSET_ENV.lock().unwrap_or_else(|p| p.into_inner());
        let prev = std::env::var_os("KIMO_ASSET_DIR");
        std::env::set_var("KIMO_ASSET_DIR", dir);
        AssetDirGuard { prev, _lock: lock }
    }
}
impl Drop for AssetDirGuard {
    // Runs before the fields drop, so the variable is restored while the lock is still held.
    fn drop(&mut self) {
        match self.prev.take() {
            Some(v) => std::env::set_var("KIMO_ASSET_DIR", v),
            None => std::env::remove_var("KIMO_ASSET_DIR"),
        }
    }
}
#[test]
fn asset_dir_guard_restores_the_previous_value_even_on_panic() {
    let before = { let _l = ASSET_ENV.lock().unwrap_or_else(|p| p.into_inner()); std::env::var_os("KIMO_ASSET_DIR") };
    let tmp = std::env::temp_dir().join(format!("kimo-raw-guard-{}", std::process::id()));
    let r = std::panic::catch_unwind(|| {
        let _g = AssetDirGuard::set(&tmp);
        assert_eq!(std::env::var_os("KIMO_ASSET_DIR").as_deref(), Some(tmp.as_os_str()));
        panic!("deliberate panic while the guard is held");
    });
    assert!(r.is_err());
    let _l = ASSET_ENV.lock().unwrap_or_else(|p| p.into_inner());
    assert_eq!(std::env::var_os("KIMO_ASSET_DIR"), before, "the guard restored the previous value during unwind");
}
#[test]
fn sample_api_roundtrip_formats_peaks_mixer_collect_limits() {
    let dir = std::env::temp_dir().join(format!("kimo-raw-api-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _asset_env = AssetDirGuard::set(&dir);
    let mut s = Session::new();
    let planes = vec![
        (0..96)
            .map(|i| if i % 7 == 0 { 1.25f32 } else { i as f32 / 128. })
            .collect::<Vec<_>>(),
        (0..96).map(|i| -i as f32 / 256.).collect::<Vec<_>>(),
    ];
    let a = dispatch(
        &mut s,
        "import_samples",
        &json!({"rate":48000,"channels":2,"data":planes}),
    )
    .unwrap();
    let again = dispatch(
        &mut s,
        "import_samples",
        &json!({"rate":48000,"channels":2,"data":planes}),
    )
    .unwrap();
    assert_eq!(a["asset_id"], again["asset_id"]);
    let before = serde_json::to_string(&s.project).unwrap();
    let target = format!("file:{}", a["path"].as_str().unwrap());
    let r = dispatch(
        &mut s,
        "read_samples",
        &json!({"target":target,"stage":"source"}),
    )
    .unwrap();
    assert_eq!(r["data"], json!(planes));
    assert_eq!(r["hash"], a["hash"]);
    assert_eq!(r["source_info"]["native_channels"], 2);
    let base = dispatch(
        &mut s,
        "read_samples",
        &json!({"target":target,"format":"base64_f32le"}),
    )
    .unwrap();
    let b = unb64(base["data"].as_str().unwrap()).unwrap();
    assert_eq!(
        b,
        bytes(&Samples {
            rate: 48000,
            planes: planes.clone()
        })
    );
    for format in ["wav_f32", "raw_f32le", "npy"] {
        let out = dispatch(
            &mut s,
            "read_samples",
            &json!({"target":target,"format":"file","file_format":format}),
        )
        .unwrap();
        assert_eq!(out["hash"], a["hash"]);
        let mut params = json!({"rate":48000,"channels":2,"format":"path","path":out["data"]});
        if format == "raw_f32le" {
            params["layout"] = json!("planar");
        }
        let imported = dispatch(&mut s, "import_samples", &params).unwrap();
        assert_eq!(imported["hash"], a["hash"]);
    }
    let peaksr = dispatch(
        &mut s,
        "read_samples",
        &json!({"target":target,"peaks":{"n":4}}),
    )
    .unwrap();
    assert_eq!(
        peaksr["peaks"],
        json!(peaks(
            &Samples {
                rate: 48000,
                planes: planes.clone()
            },
            4
        ))
    );
    assert_eq!(before, serde_json::to_string(&s.project).unwrap());
    let clip = dispatch(
        &mut s,
        "add_audio_clip",
        &json!({"asset_id":a["asset_id"],"start":0,"duration":96./48000.}),
    )
    .unwrap()["clip_id"]
        .clone();
    let r = dispatch(&mut s, "read_samples", &json!({"target":"mix"})).unwrap();
    let pcm = kimo_core::audio::mix_strict(&s.project).unwrap();
    let expected = vec![
        pcm.chunks_exact(2).map(|v| v[0]).collect::<Vec<_>>(),
        pcm.chunks_exact(2).map(|v| v[1]).collect::<Vec<_>>(),
    ];
    assert_eq!(r["data"], json!(expected));
    let cliptarget = format!("clip:{}", clip.as_str().unwrap());
    dispatch(
        &mut s,
        "add_audio_effect",
        &json!({"clip_id":clip,"kind":"mix_matrix","params":{"matrix":[[0,1],[1,0]]}}),
    )
    .unwrap();
    let swapped = dispatch(
        &mut s,
        "read_samples",
        &json!({"target":cliptarget,"stage":"post_filter"}),
    )
    .unwrap();
    assert_eq!(swapped["data"], json!([planes[1], planes[0]]));
    let raw = dispatch(
        &mut s,
        "read_samples",
        &json!({"target":cliptarget,"stage":"source"}),
    )
    .unwrap();
    assert_eq!(raw["data"], json!(planes));
    let deps = dispatch(&mut s, "list_dependencies", &json!({})).unwrap();
    assert!(deps.to_string().contains("audio"));
    let dest = dir.join("collected");
    let collect = dispatch(
        &mut s,
        "collect_project",
        &json!({"dest_dir":dest,"dry_run":true}),
    )
    .unwrap();
    assert_eq!(collect["plan"]["files"], 1);
    assert_eq!(collect["plan"]["missing"], 0);
    let copied = dispatch(&mut s, "collect_project", &json!({"dest_dir":dest})).unwrap();
    assert!(
        copied["project"].is_string()
            || copied["path"].is_string()
            || dest.join("project.kimo").exists()
    );
    assert!(std::fs::read_dir(dest.join("media")).unwrap().any(|e| e
        .unwrap()
        .path()
        .extension()
        .and_then(|e| e.to_str())
        == Some("wav")));
    let e = dispatch_ex(
        &mut s,
        "read_samples",
        &json!({"target":target,"rate":44100}),
    )
    .unwrap_err();
    assert!(e.message.contains("mixer rate is fixed 48000"));
    dispatch(&mut s, "set_limits", &json!({"max_samples":4})).unwrap();
    let e = dispatch_ex(&mut s, "read_samples", &json!({"target":target})).unwrap_err();
    assert!(e.message.contains("max_samples") && e.message.contains("set_limits"));
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn analytic_response_matches_native_and_effect_defaults() {
    let mut s = Session::new();
    let r=dispatch(&mut s,"audio_filter_response",&json!({"sections":[{"b0":0.2,"b1":0.1,"b2":0.05,"a1":-0.5,"a2":0.12}],"freqs":[100,1000,10000]})).unwrap();
    for (hz, v) in [100., 1000., 10000.]
        .iter()
        .zip(r["response"].as_array().unwrap())
    {
        let w = std::f64::consts::TAU * hz / 48000.;
        let p = |c: [f64; 3]| {
            let re = c[0] + c[1] * w.cos() + c[2] * (2. * w).cos();
            let im = -c[1] * w.sin() - c[2] * (2. * w).sin();
            re * re + im * im
        };
        assert!(
            (v["magnitude_db"].as_f64().unwrap()
                - 10. * (p([0.2, 0.1, 0.05]) / p([1., -0.5, 0.12])).log10())
            .abs()
                < 1e-10
        );
    }
}
#[test]
fn keyed_coefficients_rendered_fft_response_and_overflow_rejection() {
    let dir = std::env::temp_dir().join(format!("kimo-raw-key-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _asset_env = AssetDirGuard::set(&dir);
    let mut s = Session::new();
    for hz in [100., 1000., 10000.] {
        let x: Vec<f32> = (0..48000)
            .map(|i| 0.1 * (std::f64::consts::TAU * hz * i as f64 / 48000.).sin() as f32)
            .collect();
        let a = dispatch(
            &mut s,
            "import_samples",
            &json!({"rate":48000,"channels":2,"data":[x,x]}),
        )
        .unwrap();
        let id = dispatch(
            &mut s,
            "add_audio_clip",
            &json!({"asset_id":a["asset_id"],"start":0,"duration":1}),
        )
        .unwrap()["clip_id"]
            .clone();
        dispatch(&mut s,"add_audio_effect",&json!({"clip_id":id,"kind":"biquad","params":{"sections":[{"b0":0.2,"b1":0.1,"b2":0.05,"a1":-0.5,"a2":0.12}]}})).unwrap();
        let r = dispatch(
            &mut s,
            "read_samples",
            &json!({"target":format!("clip:{}",id.as_str().unwrap()),"format":"base64_f32le"}),
        )
        .unwrap();
        let b = unb64(r["data"].as_str().unwrap()).unwrap();
        let samples = from_bytes(48000, 2, &b, true, 4).unwrap();
        let w = std::f64::consts::TAU * hz / 48000.;
        let amp = |v: &[f32]| {
            let (re, im) = v
                .iter()
                .enumerate()
                .skip(24000)
                .fold((0., 0.), |(r, m), (i, &v)| {
                    (
                        r + v as f64 * (w * i as f64).cos(),
                        m - v as f64 * (w * i as f64).sin(),
                    )
                });
            (re * re + im * im).sqrt()
        };
        let gain = 20. * (amp(&samples.planes[0]) / amp(&x)).log10();
        let response = dispatch(
            &mut s,
            "audio_filter_response",
            &json!({"sections":[{"b0":0.2,"b1":0.1,"b2":0.05,"a1":-0.5,"a2":0.12}],"freqs":[hz]}),
        )
        .unwrap();
        assert!((gain - response["response"][0]["magnitude_db"].as_f64().unwrap()).abs() < 0.2);
    }
    let a = dispatch(
        &mut s,
        "import_samples",
        &json!({"rate":48000,"channels":2,"data":[vec![0.25;480],vec![0.5;480]]}),
    )
    .unwrap();
    let id = dispatch(
        &mut s,
        "add_audio_clip",
        &json!({"asset_id":a["asset_id"],"start":0,"duration":0.01}),
    )
    .unwrap()["clip_id"]
        .clone();
    dispatch(
        &mut s,
        "add_audio_effect",
        &json!({"clip_id":id,"kind":"biquad"}),
    )
    .unwrap();
    for (t, v) in [(0., 0.), (0.01, 1.)] {
        dispatch(&mut s,"set_audio_effect_key",&json!({"clip_id":id,"effect_index":0,"coefficient":"sections.0.b0","time":t,"value":v})).unwrap();
    }
    let r = dispatch(
        &mut s,
        "read_samples",
        &json!({"target":format!("clip:{}",id.as_str().unwrap())}),
    )
    .unwrap();
    for i in [0, 120, 240, 479] {
        let y = r["data"][0][i].as_f64().unwrap();
        assert!((y - 0.25 * i as f64 / 480.).abs() < 1e-6);
    }
    dispatch(
        &mut s,
        "add_audio_effect",
        &json!({"clip_id":id,"kind":"biquad","params":{"sections":[{"a1":-1000.}]}}),
    )
    .unwrap();
    assert!(dispatch(
        &mut s,
        "read_samples",
        &json!({"target":format!("clip:{}",id.as_str().unwrap())})
    )
    .is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn export_quantization_symmetric_dc_and_zero_crossing() {
    let dir = std::env::temp_dir().join(format!("kimo-raw-quant-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let x: Vec<f32> = (0..48000)
        .map(|i| {
            let phase = std::f64::consts::TAU * 1000. * i as f64 / 48000.;
            (0.5 * phase.sin()) as f32
        })
        .collect();
    let s = Samples {
        rate: 48000,
        planes: vec![x.clone(), x.clone()],
    };
    let source = dir.join("float.wav");
    std::fs::write(&source, wav(&s).unwrap()).unwrap();
    for (bits, codec) in [(16, "pcm_s16le"), (24, "pcm_s24le")] {
        let path = dir.join(format!("{bits}.wav"));
        assert!(std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-i",
                source.to_str().unwrap(),
                "-c:a",
                codec,
                path.to_str().unwrap()
            ])
            .status()
            .unwrap()
            .success());
        let decoded = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-i",
                path.to_str().unwrap(),
                "-f",
                "f32le",
                "-",
            ])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        let q = from_bytes(48000, 2, &decoded.stdout, false, 4).unwrap();
        let mean = q.planes[0].iter().map(|v| *v as f64).sum::<f64>() / 48000.;
        let lsb = 1. / (1u64 << (bits - 1)) as f64;
        assert!(mean.abs() <= lsb);
        for i in 1..x.len() {
            if x[i - 1].signum() != x[i].signum() {
                let delta = q.planes[0][i] - q.planes[0][i - 1];
                let expected = x[i] - x[i - 1];
                assert!((delta - expected).abs() as f64 <= 2. * lsb);
            }
        }
    }
    let mut inter = Vec::with_capacity(x.len() * 2);
    for v in x {
        inter.extend([v, v]);
    }
    let path = dir.join("legacy16.wav");
    kimo_core::audio::write_wav(path.to_str().unwrap(), &inter).unwrap();
    let b = std::fs::read(path).unwrap();
    let q: Vec<_> = b[44..]
        .chunks_exact(2)
        .map(|v| i16::from_le_bytes(v.try_into().unwrap()) as i64)
        .collect();
    assert!(q.iter().sum::<i64>().abs() <= q.len() as i64);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn api_fuzz_repeat_threads_native_mono_layout_and_errors() {
    let dir = std::env::temp_dir().join(format!("kimo-raw-edge-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _asset_env = AssetDirGuard::set(&dir);
    let mut s = Session::new();
    let a = dispatch(
        &mut s,
        "import_samples",
        &json!({"rate":24000,"channels":1,"layout":"interleaved","data":[0.1,-0.2,0.3,-0.4]}),
    )
    .unwrap();
    let target = format!("file:{}", a["path"].as_str().unwrap());
    let mut first = None;
    for threads in [1, 4, 1] {
        kimo_core::par::set_threads(threads);
        let r = dispatch(&mut s, "read_samples", &json!({"target":target})).unwrap();
        assert_eq!(r["rate"], 48000);
        assert_eq!(r["channels"], 2);
        assert_eq!(r["source_info"]["native_channels"], 1);
        assert_eq!(r["source_info"]["native_rate"], 24000);
        let r = serde_json::to_string(&r).unwrap();
        if let Some(f) = &first {
            assert_eq!(&r, f);
        } else {
            first = Some(r);
        }
    }
    for format in ["base64_f32le", "base64_s16le", "base64_f64le"] {
        let data = if format == "base64_s16le" {
            vec![1i16, -1]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>()
        } else if format == "base64_f64le" {
            vec![0.125f64, -0.125]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>()
        } else {
            vec![0.125f32, -0.125]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>()
        };
        assert!(dispatch(&mut s,"import_samples",&json!({"rate":48000,"channels":2,"layout":"interleaved","format":format,"data":b64(&data)})).is_ok());
    }
    let before = serde_json::to_string(&s.project).unwrap();
    for i in 0..1000 {
        let a = match i % 5 {
            0 => json!({"rate":0,"channels":2,"data":[]}),
            1 => json!({"rate":48000,"channels":2,"data":[[],[1]]}),
            2 => json!({"rate":48000,"channels":2,"format":"base64_f32le","data":"bad!"}),
            3 => json!({"rate":48000,"channels":0,"data":[]}),
            _ => json!({"rate":48000,"channels":2,"data":[[null],[0]]}),
        };
        assert!(dispatch(&mut s, "import_samples", &a).is_err());
        assert_eq!(before, serde_json::to_string(&s.project).unwrap());
    }
    let e = dispatch_ex(
        &mut s,
        "import_samples",
        &json!({"rate":48000,"channels":2,"data":[[null],[0]]}),
    )
    .unwrap_err();
    assert_eq!(e.code, "invalid_samples");
    assert!(e.message.contains("index 0"));
    let e = dispatch_ex(
        &mut s,
        "read_samples",
        &json!({"target":target,"channels":[2]}),
    )
    .unwrap_err();
    assert!(e.message.contains("available decoded channels"));
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn same_pcm_different_shape_distinct_asset_key_preserved_native() {
    let dir = std::env::temp_dir().join(format!("kimo-raw-shape-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _asset_env = AssetDirGuard::set(&dir);
    let mut s = Session::new();
    let a = dispatch(
        &mut s,
        "import_samples",
        &json!({"rate":48000,"channels":2,"data":[[0.1,0.2],[0.3,0.4]]}),
    )
    .unwrap();
    let b = dispatch(
        &mut s,
        "import_samples",
        &json!({"rate":44100,"channels":1,"data":[[0.1,0.2,0.3,0.4]]}),
    )
    .unwrap();
    assert_eq!(a["hash"], b["hash"]);
    assert_ne!(a["asset_key"], b["asset_key"]);
    assert_ne!(a["path"], b["path"]);
    assert_eq!(s.project.assets.len(), 2);
    let raw = std::fs::read(b["path"].as_str().unwrap()).unwrap();
    assert_eq!(
        &raw[44..],
        &bytes(&Samples {
            rate: 44100,
            planes: vec![vec![0.1, 0.2, 0.3, 0.4]]
        })
    );
    let read = dispatch(
        &mut s,
        "read_samples",
        &json!({"target":format!("file:{}",b["path"].as_str().unwrap())}),
    )
    .unwrap();
    assert_eq!(read["rate"], 48000);
    assert_eq!(read["channels"], 2);
    assert_eq!(read["source_info"]["native_rate"], 44100);
    let decoded =
        kimo_core::perception::audio_targets::decode(&s, &json!({"path":b["path"]})).unwrap();
    let expected = vec![
        decoded
            .pcm
            .chunks_exact(2)
            .map(|v| v[0])
            .collect::<Vec<_>>(),
        decoded
            .pcm
            .chunks_exact(2)
            .map(|v| v[1])
            .collect::<Vec<_>>(),
    ];
    assert_eq!(read["data"], json!(expected));
    std::fs::remove_dir_all(dir).unwrap();
}
