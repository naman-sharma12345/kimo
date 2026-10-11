pub use kimo_core::{color, effects, guard};
#[path = "../src/color/subject_match.rs"]
mod subject_match;
use kimo_core::{
    color::{managed, matching},
    effects::Frame,
};
use subject_match::*;
fn frame(p: &[[f32; 4]]) -> Frame {
    Frame {
        w: p.len(),
        h: 1,
        d: p.iter().flatten().copied().collect(),
    }
}
fn sample() -> (Frame, Vec<[f32; 3]>) {
    let p: Vec<_> = (0..24)
        .map(|i| {
            let t = (i % 8) as f32 / 7.;
            match i / 8 {
                0 => [0.3 + 0.25 * t, 0.16 + 0.18 * t, 0.1 + 0.12 * t, 1.],
                1 => [0.05 + 0.06 * t, 0.25 + 0.15 * t, 0.3 + 0.22 * t, 1.],
                _ => [0.4 + 0.3 * t, 0.4 + 0.3 * t, 0.4 + 0.3 * t, 1.],
            }
        })
        .collect();
    (
        frame(&p),
        (0..24)
            .map(|i| {
                let mut w = [0.; 3];
                w[i / 8] = 1.;
                w
            })
            .collect(),
    )
}
#[test]
fn per_region_statistics_are_exact_population() {
    let (f, w) = sample();
    let s = statistics(&f, &Regions { weights: &w }).unwrap();
    for r in 0..3 {
        let a = s.regions[r].as_ref().unwrap();
        assert_eq!(a.contributing_pixels, 8);
        assert_eq!(a.weight, 8.);
        assert_eq!(a.effective_samples, 8.);
        let lab: Vec<_> = f.d[r * 32..(r + 1) * 32]
            .chunks_exact(4)
            .map(|p| matching::to_lab([p[0] as f64, p[1] as f64, p[2] as f64]))
            .collect();
        let mean: [f64; 3] = std::array::from_fn(|j| lab.iter().map(|c| c[j]).sum::<f64>() / 8.);
        for j in 0..3 {
            assert!((a.lab_mean[j] - mean[j]).abs() < 1e-13);
            for k in 0..3 {
                let cov = lab
                    .iter()
                    .map(|c| (c[j] - mean[j]) * (c[k] - mean[k]))
                    .sum::<f64>()
                    / 8.;
                assert!((a.lab_covariance[j][k] - cov).abs() < 1e-13);
            }
        }
    }
}
#[test]
fn alpha_and_soft_weights_use_weighted_population() {
    let f = frame(&[[0.2, 0.2, 0.2, 1.], [0.7, 0.7, 0.7, 0.5], [1., 0., 0., 0.]]);
    let w = [[0.5, 0.5, 0.], [1., 0., 0.], [1., 0., 0.]];
    let s = statistics(&f, &Regions { weights: &w }).unwrap();
    let a = s.regions[SKIN].as_ref().unwrap();
    assert_eq!(a.weight, 1.);
    assert_eq!(a.contributing_pixels, 2);
    assert_eq!(a.effective_samples, 2.);
    let low = matching::to_lab([0.2f32 as f64; 3]);
    let high = matching::to_lab([0.7f32 as f64; 3]);
    assert!((a.lab_mean[0] - (low[0] + high[0]) / 2.).abs() < 1e-13);
    assert!((a.lab_covariance[0][0] - (high[0] - low[0]).powi(2) / 4.).abs() < 1e-13);
    assert!(s.regions[LIGHTING].is_none());
    assert_eq!(s.regions[BACKGROUND].as_ref().unwrap().weight, 0.5);
}
#[test]
fn amount_zero_and_identical_frames_are_bit_exact() {
    let (mut f, w) = sample();
    f.d[3] = 0.;
    f.d[7] = 0.5;
    let original = f.d.clone();
    let mut o = Options::default();
    o.amount = 0.;
    let p = fit(
        &f,
        &Regions { weights: &w },
        &f,
        &Regions { weights: &w },
        o,
    )
    .unwrap();
    p.apply(&mut f, &Regions { weights: &w }).unwrap();
    assert_eq!(f.d, original);
    let p = fit(
        &f,
        &Regions { weights: &w },
        &f,
        &Regions { weights: &w },
        Options::default(),
    )
    .unwrap();
    p.apply(&mut f, &Regions { weights: &w }).unwrap();
    assert_eq!(f.d, original);
}
#[test]
fn deterministic_matching_and_alpha_preservation() {
    let (f, w) = sample();
    let mut reference = Frame {
        w: f.w,
        h: f.h,
        d: f.d.clone(),
    };
    for px in reference.d.chunks_exact_mut(4) {
        px[0] = (px[0] * 1.1).min(1.);
        px[2] *= 0.9;
    }
    let p = fit(
        &f,
        &Regions { weights: &w },
        &reference,
        &Regions { weights: &w },
        Options::default(),
    )
    .unwrap();
    let mut a = Frame {
        w: f.w,
        h: f.h,
        d: f.d.clone(),
    };
    a.d[3] = 0.;
    a.d[7] = 0.25;
    let mut b = Frame {
        w: a.w,
        h: a.h,
        d: a.d.clone(),
    };
    let original = a.d.clone();
    p.apply(&mut a, &Regions { weights: &w }).unwrap();
    p.apply(&mut b, &Regions { weights: &w }).unwrap();
    assert_eq!(a.d, b.d);
    assert_eq!(&a.d[..4], &original[..4]);
    for (out, src) in a.d.chunks_exact(4).zip(original.chunks_exact(4)) {
        assert_eq!(out[3], src[3]);
    }
}
#[test]
fn creative_exposure_uses_linear_luminance() {
    let f = frame(&[[0.3, 0.3, 0.3, 1.]; 8]);
    let w = [[0., 0., 1.]; 8];
    let mut o = Options::default();
    o.region_amount = [0., 0., 1.];
    o.exposure_target[LIGHTING] = ExposureTarget::LinearLuminance(0.18);
    let p = fit(
        &f,
        &Regions { weights: &w },
        &f,
        &Regions { weights: &w },
        o,
    )
    .unwrap();
    let mut out = frame(&[[0.3, 0.3, 0.3, 1.]; 8]);
    p.apply(&mut out, &Regions { weights: &w }).unwrap();
    for px in out.d.chunks_exact(4) {
        let y = managed::decode(1, px[1] as f64, 100.);
        assert!((y - 0.18).abs() < 2e-7);
    }
}
#[test]
fn independent_regions_do_not_leak_and_unassigned_is_exact() {
    let (f, mut w) = sample();
    w[23] = [0.; 3];
    let mut r = Frame {
        w: f.w,
        h: f.h,
        d: f.d.clone(),
    };
    for p in r.d[..32].chunks_exact_mut(4) {
        p[0] *= 1.2;
    }
    let mut o = Options::default();
    o.region_amount = [1., 0., 0.];
    let p = fit(
        &f,
        &Regions { weights: &w },
        &r,
        &Regions { weights: &w },
        o,
    )
    .unwrap();
    let mut out = Frame {
        w: f.w,
        h: f.h,
        d: f.d.clone(),
    };
    p.apply(&mut out, &Regions { weights: &w }).unwrap();
    assert_ne!(&out.d[..32], &f.d[..32]);
    assert_eq!(&out.d[32..], &f.d[32..]);
}
#[test]
fn hostile_params_rejected_before_mutation() {
    let (mut f, w) = sample();
    let original = f.d.clone();
    let mut variants = vec![];
    for v in [f64::NAN, f64::INFINITY, -1., 2.] {
        let mut o = Options::default();
        o.amount = v;
        variants.push(o);
        let mut o = Options::default();
        o.region_amount[0] = v;
        variants.push(o);
    }
    for t in [
        ExposureTarget::LinearLuminance(f64::NAN),
        ExposureTarget::LinearLuminance(0.),
        ExposureTarget::LinearLuminance(2.),
        ExposureTarget::ReferenceOffsetStops(9.),
    ] {
        let mut o = Options::default();
        o.exposure_target[0] = t;
        variants.push(o);
    }
    for o in variants {
        assert!(fit(
            &f,
            &Regions { weights: &w },
            &f,
            &Regions { weights: &w },
            o
        )
        .is_err());
    }
    let p = fit(
        &f,
        &Regions { weights: &w },
        &f,
        &Regions { weights: &w },
        Options::default(),
    )
    .unwrap();
    let mut bad = w.clone();
    bad[23] = [f32::NAN, 0., 0.];
    assert!(p.apply(&mut f, &Regions { weights: &bad }).is_err());
    assert_eq!(f.d, original);
    bad[23] = [1., 1., 0.];
    assert!(p.apply(&mut f, &Regions { weights: &bad }).is_err());
    assert_eq!(f.d, original);
    assert!(p.apply(&mut f, &Regions { weights: &w[..23] }).is_err());
    assert_eq!(f.d, original);
    let invalid = Frame {
        w: usize::MAX,
        h: 2,
        d: vec![],
    };
    assert!(statistics(&invalid, &Regions { weights: &[] }).is_err());
}
#[test]
fn absent_or_low_effective_support_requires_explicit_disable() {
    let f = frame(&[[0.3, 0.2, 0.1, 1.]; 8]);
    let w = [[1., 0., 0.]; 8];
    assert!(fit(
        &f,
        &Regions { weights: &w },
        &f,
        &Regions { weights: &w },
        Options::default()
    )
    .is_err());
    let mut o = Options::default();
    o.region_amount = [1., 0., 0.];
    assert!(fit(
        &f,
        &Regions { weights: &w },
        &f,
        &Regions { weights: &w },
        o
    )
    .is_ok());
    let mut w = w;
    for p in &mut w[1..] {
        p[0] = 0.001;
    }
    assert!(fit(
        &f,
        &Regions { weights: &w },
        &f,
        &Regions { weights: &w },
        o
    )
    .is_err());
}
#[test]
fn preserve_and_reference_offset_exposure() {
    let f = frame(&[[0.3, 0.3, 0.3, 1.]; 8]);
    let r = frame(&[[0.6, 0.6, 0.6, 1.]; 8]);
    let w = [[0., 0., 1.]; 8];
    for t in [
        ExposureTarget::PreserveSource,
        ExposureTarget::ReferenceOffsetStops(-1.),
    ] {
        let mut o = Options::default();
        o.region_amount = [0., 0., 1.];
        o.exposure_target[2] = t;
        let p = fit(
            &f,
            &Regions { weights: &w },
            &r,
            &Regions { weights: &w },
            o,
        )
        .unwrap();
        let mut out = frame(&[[0.3, 0.3, 0.3, 1.]; 8]);
        p.apply(&mut out, &Regions { weights: &w }).unwrap();
        let expected = match t {
            ExposureTarget::PreserveSource => managed::decode(1, 0.3f32 as f64, 100.),
            _ => managed::decode(1, 0.6f32 as f64, 100.) / 2.,
        };
        assert!((managed::decode(1, out.d[0] as f64, 100.) - expected).abs() < 2e-7);
    }
}
#[test]
fn hostile_controls_full_matrix() {
    let (f, w) = sample();
    let mut cases = vec![];
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1., 1.01] {
        let mut o = Options::default();
        o.contrast_amount = v;
        cases.push(o);
        for r in 0..3 {
            let mut o = Options::default();
            o.region_amount[r] = v;
            cases.push(o);
        }
    }
    for v in [0., -1., 0.1001, f64::NAN, f64::INFINITY] {
        let mut o = Options::default();
        o.regularization = v;
        cases.push(o);
    }
    for v in [-1., 8.001, f64::NAN, f64::INFINITY] {
        let mut o = Options::default();
        o.max_exposure_stops = v;
        cases.push(o);
    }
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -8.001, 8.001] {
        let mut o = Options::default();
        o.exposure_target[1] = ExposureTarget::ReferenceOffsetStops(v);
        cases.push(o);
    }
    for o in cases {
        assert!(fit(
            &f,
            &Regions { weights: &w },
            &f,
            &Regions { weights: &w },
            o
        )
        .is_err());
    }
}
#[test]
fn hostile_frame_and_matte_matrix_is_transactional() {
    let (f, w) = sample();
    let p = fit(
        &f,
        &Regions { weights: &w },
        &f,
        &Regions { weights: &w },
        Options::default(),
    )
    .unwrap();
    for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 1.01] {
        for c in 0..4 {
            let mut bad = Frame {
                w: f.w,
                h: f.h,
                d: f.d.clone(),
            };
            bad.d[23 * 4 + c] = v;
            let bytes: Vec<_> = bad.d.iter().map(|v| v.to_bits()).collect();
            assert!(p.apply(&mut bad, &Regions { weights: &w }).is_err());
            assert_eq!(bad.d.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), bytes);
        }
        for c in 0..3 {
            let mut bad = w.clone();
            bad[23] = [0.; 3];
            bad[23][c] = v;
            let mut out = Frame {
                w: f.w,
                h: f.h,
                d: f.d.clone(),
            };
            assert!(p.apply(&mut out, &Regions { weights: &bad }).is_err());
            assert_eq!(out.d, f.d);
        }
    }
    for bad in [
        Frame {
            w: 0,
            h: 1,
            d: vec![],
        },
        Frame {
            w: 1,
            h: 0,
            d: vec![],
        },
        Frame {
            w: 1,
            h: 1,
            d: vec![0.; 3],
        },
        Frame {
            w: usize::MAX,
            h: usize::MAX,
            d: vec![],
        },
    ] {
        assert!(statistics(&bad, &Regions { weights: &[] }).is_err());
    }
}
#[test]
fn lighting_log_statistics_match_direct_weighted_calculation() {
    let f = frame(&[
        [0.1, 0.2, 0.3, 1.],
        [0.6, 0.5, 0.4, 0.25],
        [0., 0., 0., 1.],
        [1., 1., 1., 0.],
    ]);
    let w = [[0., 0., 0.2], [0., 0., 0.8], [0., 0., 0.4], [0., 0., 1.]];
    let a = statistics(&f, &Regions { weights: &w }).unwrap().regions[2]
        .clone()
        .unwrap();
    let mut values = vec![];
    for (px, w) in f.d.chunks_exact(4).zip(w) {
        let l = px[..3]
            .iter()
            .map(|v| managed::decode(1, *v as f64, 100.))
            .collect::<Vec<_>>();
        let y = (l[0] * 0.2126390058715104 + l[1] * 0.715168678767756 + l[2] * 0.0721923153607337)
            .max(1e-6);
        values.push((y.log2(), w[2] as f64 * px[3] as f64));
    }
    let total = values.iter().map(|v| v.1).sum::<f64>();
    let mean = values.iter().map(|v| v.0 * v.1).sum::<f64>() / total;
    let var = values
        .iter()
        .map(|v| (v.0 - mean).powi(2) * v.1)
        .sum::<f64>()
        / total;
    assert!((a.log_luminance_mean - mean).abs() < 1e-12);
    assert!((a.log_luminance_variance - var).abs() < 1e-12);
}
#[test]
fn soft_masks_blend_lab_delta_and_keep_unassigned_remainder() {
    let f = frame(&[[0.3, 0.3, 0.3, 1.]; 8]);
    let r = frame(&[[0.6, 0.6, 0.6, 1.]; 8]);
    let one = [[1., 0., 0.]; 8];
    let half = [[0.5, 0., 0.]; 8];
    let mut o = Options::default();
    o.region_amount = [1., 0., 0.];
    let p = fit(
        &f,
        &Regions { weights: &one },
        &r,
        &Regions { weights: &one },
        o,
    )
    .unwrap();
    let mut full = frame(&[[0.3, 0.3, 0.3, 1.]; 8]);
    let mut partial = frame(&[[0.3, 0.3, 0.3, 1.]; 8]);
    p.apply(&mut full, &Regions { weights: &one }).unwrap();
    p.apply(&mut partial, &Regions { weights: &half }).unwrap();
    let lab = |f: &Frame| matching::to_lab([f.d[0] as f64, f.d[1] as f64, f.d[2] as f64]);
    let a = lab(&f);
    let b = lab(&full);
    let c = lab(&partial);
    for j in 0..3 {
        assert!((c[j] - (a[j] + b[j]) / 2.).abs() < 1e-7);
    }
    o.amount = 0.5;
    let p = fit(
        &f,
        &Regions { weights: &one },
        &r,
        &Regions { weights: &one },
        o,
    )
    .unwrap();
    let mut global = frame(&[[0.3, 0.3, 0.3, 1.]; 8]);
    p.apply(&mut global, &Regions { weights: &one }).unwrap();
    assert_eq!(global.d, partial.d);
}
#[test]
fn true_black_stays_black_under_nonidentity_plan() {
    let f = frame(&[[0., 0., 0., 1.]; 8]);
    let r = frame(&[[0.5, 0.2, 0.1, 1.]; 8]);
    let w = [[1., 0., 0.]; 8];
    let mut o = Options::default();
    o.region_amount = [1., 0., 0.];
    let p = fit(
        &f,
        &Regions { weights: &w },
        &r,
        &Regions { weights: &w },
        o,
    )
    .unwrap();
    let mut out = frame(&[[0., 0., 0., 1.]; 8]);
    p.apply(&mut out, &Regions { weights: &w }).unwrap();
    for px in out.d.chunks_exact(4) {
        assert!(px[..3].iter().all(|v| v.abs() < 1e-7));
        assert_eq!(px[3], 1.);
    }
}
#[test]
fn finite_bounded_outputs_over_extreme_valid_parameters() {
    let colors: Vec<_> = (0..64)
        .map(|i| {
            [
                (i % 7) as f32 / 6.,
                (i % 11) as f32 / 10.,
                (i % 13) as f32 / 12.,
                if i % 4 == 0 { 0. } else { 0.5 },
            ]
        })
        .collect();
    let f = frame(&colors);
    let reference = frame(&colors.iter().rev().copied().collect::<Vec<_>>());
    let w = [[1., 0., 0.]; 64];
    for reg in [1e-8, 0.1] {
        for stops in [-8., 8.] {
            let mut o = Options::default();
            o.region_amount = [1., 0., 0.];
            o.regularization = reg;
            o.max_exposure_stops = 8.;
            o.exposure_target[0] = ExposureTarget::ReferenceOffsetStops(stops);
            let p = fit(
                &f,
                &Regions { weights: &w },
                &reference,
                &Regions { weights: &w },
                o,
            )
            .unwrap();
            let mut out = frame(&colors);
            p.apply(&mut out, &Regions { weights: &w }).unwrap();
            assert!(out
                .d
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v)));
        }
    }
}
#[test]
fn independent_frame_sizes_and_luminance_change_bound() {
    let f = frame(&[[0.2, 0.2, 0.2, 1.]; 8]);
    let r = frame(&[[0.7, 0.7, 0.7, 1.]; 16]);
    let a = [[0., 1., 0.]; 8];
    let b = [[0., 1., 0.]; 16];
    let mut o = Options::default();
    o.region_amount = [0., 1., 0.];
    o.max_exposure_stops = 0.5;
    let p = fit(
        &f,
        &Regions { weights: &a },
        &r,
        &Regions { weights: &b },
        o,
    )
    .unwrap();
    let mut out = frame(&[[0.2, 0.2, 0.2, 1.]; 8]);
    p.apply(&mut out, &Regions { weights: &a }).unwrap();
    let y = managed::decode(1, out.d[0] as f64, 100.);
    let src = managed::decode(1, 0.2f32 as f64, 100.);
    assert!(((y / src).log2() - 0.5).abs() < 2e-6);
}
