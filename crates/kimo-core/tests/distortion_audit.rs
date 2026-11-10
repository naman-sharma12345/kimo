use kimo_core::effects::{Frame, Params};
fn p(m: &Params, k: &str, d: f64, t: f64) -> f32 {
    m.get(k).map(|x| x.at(t)).unwrap_or(d) as f32
}
fn legacy(kind: &str, params: &Params, f: &mut Frame, t: f64) {
    match kind {
        "rgb_split" => {
            let a = p(params, "amount", 6.0, t) as isize;
            let src = f.d.clone();
            let w = f.w as isize;
            for y in 0..f.h {
                for x in 0..f.w {
                    let i = (y * f.w + x) * 4;
                    let xr = (x as isize + a).clamp(0, w - 1) as usize;
                    let xb = (x as isize - a).clamp(0, w - 1) as usize;
                    f.d[i] = src[(y * f.w + xr) * 4];
                    f.d[i + 2] = src[(y * f.w + xb) * 4 + 2];
                }
            }
        }
        "shake" => {
            let a = p(params, "amount", 8.0, t);
            let sp = p(params, "speed", 24.0, t);
            let n = |k: f32| (((k * 12.9898).sin() * 43758.545).fract()) * 2.0 - 1.0;
            let step = (t as f32 * sp).floor();
            let (dx, dy) = ((n(step) * a) as isize, (n(step + 7.0) * a) as isize);
            let src = f.d.clone();
            let (w, h) = (f.w as isize, f.h as isize);
            for y in 0..h {
                for x in 0..w {
                    let (sx, sy) = ((x - dx).clamp(0, w - 1), (y - dy).clamp(0, h - 1));
                    for j in 0..4 {
                        f.d[((y * w + x) * 4) as usize + j] = src[((sy * w + sx) * 4) as usize + j];
                    }
                }
            }
        }
        "glitch_blocks" => {
            let (a, n, rate) = (
                p(params, "amount", 30.0, t),
                p(params, "slices", 12.0, t).max(1.0) as usize,
                p(params, "rate", 12.0, t),
            );
            let step = (t as f32 * rate).floor();
            let src = f.d.clone();
            let (w, h) = (f.w, f.h);
            let sh = (h / n).max(1);
            let rnd = |k: f32| (((k * 12.9898).sin() * 43758.545).fract()).abs();
            for y in 0..h {
                let sl = (y / sh) as f32;
                let r = rnd(sl + step * 17.0);
                let off = if r > 0.55 {
                    ((rnd(sl * 3.1 + step) * 2.0 - 1.0) * a) as isize
                } else {
                    0
                };
                for x in 0..w {
                    let sx = (x as isize + off).rem_euclid(w as isize) as usize;
                    let o = (y * w + x) * 4;
                    f.d[o] = src[(y * w + sx) * 4];
                    f.d[o + 1] = src[(y * w
                        + ((sx as isize + off.signum() * 4).rem_euclid(w as isize) as usize))
                        * 4
                        + 1];
                    f.d[o + 2] = src[(y * w + sx) * 4 + 2];
                    f.d[o + 3] = src[(y * w + sx) * 4 + 3];
                }
            }
        }
        "chromatic_aberration" => {
            let a = p(params, "amount", 4.0, t);
            let src = f.d.clone();
            let (w, h) = (f.w as f32, f.h as f32);
            for y in 0..f.h {
                for x in 0..f.w {
                    let (nx, ny) = (x as f32 / w - 0.5, y as f32 / h - 0.5);
                    let o = (y * f.w + x) * 4;
                    for (j, s) in [(0usize, 1.0f32), (2usize, -1.0f32)] {
                        let sx = (x as f32 + nx * a * 2.0 * s).clamp(0.0, w - 1.0) as usize;
                        let sy = (y as f32 + ny * a * 2.0 * s).clamp(0.0, h - 1.0) as usize;
                        f.d[o + j] = src[(sy * f.w + sx) * 4 + j];
                    }
                }
            }
        }
        _ => panic!("unsupported reference"),
    }
}
fn patterned(w: usize, h: usize) -> Frame {
    Frame {
        w,
        h,
        d: (0..w * h * 4).map(|i| (i % 73) as f32 / 72.).collect(),
    }
}
#[test]
fn frozen_distortion_defaults_and_legacy_amounts() {
    for (w, h) in [(1, 1), (17, 19), (65, 37)] {
        for kind in [
            "rgb_split",
            "shake",
            "glitch_blocks",
            "chromatic_aberration",
        ] {
            for amount in [None, Some(0.), Some(3.7), Some(-6.8), Some(30.)] {
                for t in [0., 0.13, 0.5, 1.73] {
                    let mut ps = Params::new();
                    if let Some(a) = amount {
                        ps.insert("amount".into(), kimo_core::keyframe::Prop::new(a));
                    }
                    let mut a = patterned(w, h);
                    let mut b = patterned(w, h);
                    legacy(kind, &ps, &mut a, t);
                    kimo_core::effects::apply(kind, &ps, &Default::default(), &mut b, t, 1)
                        .unwrap();
                    assert_eq!(a.d, b.d, "{kind} {w}x{h} amount{amount:?} time{t}");
                }
            }
        }
    }
}
#[test]
fn every_exposed_parameter_is_live_and_animatable() {
    use kimo_core::keyframe::{Ease, Key, Prop};
    let cases: [(&str, &str, f64); 26] = [
        ("rgb_split", "r_offset", 19.),
        ("rgb_split", "g_offset", 14.),
        ("rgb_split", "b_offset", 16.),
        ("rgb_split", "angle", 90.),
        ("shake", "amount_y", 25.),
        ("shake", "noise_frequency", 4.),
        ("shake", "noise_gain", 60.),
        ("shake", "y_seed_offset", 3.),
        ("glitch_blocks", "noise_frequency", 2.),
        ("glitch_blocks", "noise_gain", 11.),
        ("glitch_blocks", "step_seed_scale", 6.),
        ("glitch_blocks", "slice_threshold", -1.),
        ("glitch_blocks", "slice_seed_scale", 8.),
        ("glitch_blocks", "green_offset", 12.),
        ("glitch_blocks", "slice_height", 4.),
        ("chromatic_aberration", "cx", 0.),
        ("chromatic_aberration", "cy", 0.),
        ("chromatic_aberration", "radial_scale", 6.),
        ("chromatic_aberration", "r_scale", -3.),
        ("chromatic_aberration", "b_scale", 3.),
        ("glitch_blocks", "amount", 12.),
        ("glitch_blocks", "slices", 4.),
        ("glitch_blocks", "rate", 3.),
        ("shake", "amount", 3.),
        ("shake", "speed", 3.),
        ("chromatic_aberration", "amount", 8.),
    ];
    for (kind, param, value) in cases {
        let mut changed = false;
        for t in [0.13, 0.27, 0.63, 1.73] {
            let mut a = patterned(65, 37);
            let mut b = patterned(65, 37);
            kimo_core::effects::apply(kind, &Default::default(), &Default::default(), &mut a, t, 1)
                .unwrap();
            let mut ps = Params::new();
            ps.insert(param.into(), Prop::new(value));
            kimo_core::effects::apply(kind, &ps, &Default::default(), &mut b, t, 1).unwrap();
            changed |= a.d != b.d;
            let mut keyed = ps.clone();
            keyed.insert(
                param.into(),
                Prop {
                    base: 0.,
                    keys: vec![
                        Key {
                            t: 0.,
                            v: value - 2.,
                            ease: Ease::Linear,
                            bez: None,
                            out: None,
                            inn: None,
                        },
                        Key {
                            t: 2.,
                            v: value + 2.,
                            ease: Ease::Linear,
                            bez: None,
                            out: None,
                            inn: None,
                        },
                    ],
                    expr: vec![],
                },
            );
            let v = keyed[param].at(t);
            let mut constant = ps.clone();
            constant.insert(param.into(), Prop::new(v));
            let mut ka = patterned(65, 37);
            let mut kb = patterned(65, 37);
            kimo_core::effects::apply(kind, &keyed, &Default::default(), &mut ka, t, 1).unwrap();
            kimo_core::effects::apply(kind, &constant, &Default::default(), &mut kb, t, 1).unwrap();
            assert_eq!(ka.d, kb.d, "ignored keyframe {kind}.{param}");
        }
        assert!(changed, "dead knob {kind}.{param}");
    }
}
#[test]
fn description_formulas_and_default_values() {
    use kimo_core::commands::{dispatch, Session};
    use serde_json::json;
    let mut s = Session::new();
    for kind in [
        "rgb_split",
        "shake",
        "glitch_blocks",
        "chromatic_aberration",
    ] {
        let r = dispatch(&mut s, "effect_describe", &json!({"kind":kind})).unwrap();
        assert!(r["formula"].as_str().unwrap().contains("Formula:"));
        assert!(r["params"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["animatable"] == true));
    }
    let r = dispatch(&mut s, "effect_describe", &json!({"kind":"glitch_blocks"})).unwrap();
    assert!(r["formula"].as_str().unwrap().contains("slice_threshold"));
    let r = dispatch(&mut s, "effect_describe", &json!({"kind":"shake"})).unwrap();
    assert!(r["formula"].as_str().unwrap().contains("signed_fract"));
}
#[test]
fn doc_signed_fract_and_integer_sampling_match_code() {
    use kimo_core::commands::{dispatch, Session};
    use kimo_core::keyframe::Prop;
    use serde_json::json;
    let mut s = Session::new();
    let description = dispatch(&mut s, "effect_describe", &json!({"kind":"shake"})).unwrap();
    assert!(description["formula"]
        .as_str()
        .unwrap()
        .contains("dx=trunc(n(step)*amount)"));
    let mut ps = Params::new();
    for (k, v) in [
        ("noise_frequency", 4.),
        ("noise_gain", 1.5),
        ("amount", 3.),
        ("amount_y", 0.),
        ("speed", 1.),
    ] {
        ps.insert(k.into(), Prop::new(v));
    }
    let raw = 4f32.sin() * 1.5;
    assert!(raw < 0.);
    let dx = ((2. * raw.fract() - 1.) * 3.) as isize;
    let mathematical_dx = ((2. * (raw - raw.floor()) - 1.) * 3.) as isize;
    assert_ne!(dx, mathematical_dx);
    let src = patterned(17, 19);
    let mut out = patterned(17, 19);
    kimo_core::effects::apply("shake", &ps, &Default::default(), &mut out, 1., 1).unwrap();
    for y in 0..19 {
        for x in 0..17 {
            let sx = (x as isize - dx).clamp(0, 16) as usize;
            assert_eq!(
                &out.d[(y * 17 + x) * 4..(y * 17 + x) * 4 + 4],
                &src.d[(y * 17 + sx) * 4..(y * 17 + sx) * 4 + 4]
            );
        }
    }
    let description = dispatch(&mut s, "effect_describe", &json!({"kind":"rgb_split"})).unwrap();
    assert!(description["formula"]
        .as_str()
        .unwrap()
        .contains("offset truncated toward zero before adding integer position"));
    let mut ps = Params::new();
    for (k, v) in [("r_offset", 1.9), ("b_offset", -1.9)] {
        ps.insert(k.into(), Prop::new(v));
    }
    let mut out = patterned(17, 19);
    kimo_core::effects::apply("rgb_split", &ps, &Default::default(), &mut out, 0., 1).unwrap();
    for y in 0..19 {
        for x in 0usize..17 {
            let i = (y * 17 + x) * 4;
            assert_eq!(out.d[i], src.d[(y * 17 + (x + 1).min(16)) * 4]);
            assert_eq!(out.d[i + 2], src.d[(y * 17 + x.saturating_sub(1)) * 4 + 2]);
        }
    }
}
