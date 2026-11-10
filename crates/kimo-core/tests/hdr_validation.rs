use kimo_core::color::{hdr_validation::*, metadata::Mastering};
use kimo_core::effects::Frame;
fn metadata() -> Mastering {
    Mastering {
        red: [0.68, 0.32],
        green: [0.265, 0.69],
        blue: [0.15, 0.06],
        white: [0.3127, 0.329],
        min_nits: 0.005,
        max_nits: 1000.0,
        max_cll: 1000,
        max_fall: 400,
    }
}
fn frame(rgb: [f32; 3]) -> Frame {
    Frame {
        w: 1,
        h: 1,
        d: vec![rgb[0], rgb[1], rgb[2], 1.0],
    }
}
#[test]
fn transfer_grid_measured() {
    let e = transfer_evidence();
    println!("{}", serde_json::to_string_pretty(&e).unwrap());
    assert!(e.passed, "{e:?}");
}
#[test]
fn reference_ootf_neutrals_and_chroma() {
    let mut max_error: f64 = 0.0;
    for peak in [400.0, 600.0, 1000.0, 2000.0] {
        let t = HlgTarget::new(peak).unwrap();
        assert_eq!(t.ootf([0.0; 3]).unwrap(), [0.0; 3]);
        for i in 0..=4096 {
            let x = i as f64 / 4096.0;
            for v in t.ootf([x; 3]).unwrap() {
                max_error = max_error.max((v - peak * x.powf(t.gamma())).abs());
            }
        }
        for rgb in [
            [0.8, 0.2, 0.05],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ] {
            let y = 0.2627 * rgb[0] + 0.6780 * rgb[1] + 0.0593 * rgb[2];
            let out = t.ootf(rgb).unwrap();
            let yd = 0.2627 * out[0] + 0.6780 * out[1] + 0.0593 * out[2];
            max_error = max_error.max((yd - peak * y.powf(t.gamma())).abs());
            if rgb[2] > 0.0 {
                assert!((out[0] / out[2] - rgb[0] / rgb[2]).abs() < 1e-12);
            }
        }
        assert!((t.ootf([1.0; 3]).unwrap()[0] - peak).abs() < 1e-12);
    }
    println!("HLG neutral/luma maximum absolute nits error: {max_error:.16e}");
    assert!(max_error < 1e-10);
    assert_eq!(HlgTarget::new(1000.0).unwrap().gamma(), 1.2);
    assert!((HlgTarget::new(400.0).unwrap().gamma() - 1.0328651963577442).abs() < 1e-15);
}
#[test]
fn target_parameter_domain_and_atomic_rejection() {
    for p in [
        f64::NAN,
        f64::INFINITY,
        -1.0,
        0.0,
        399.999,
        2000.001,
        10000.0,
    ] {
        assert!(HlgTarget::new(p).is_err());
    }
    let t = HlgTarget::new(1000.0).unwrap();
    for rgb in [[-0.1, 0.0, 0.0], [1.0001, 0.0, 0.0], [f64::NAN, 0.0, 0.0]] {
        assert!(t.ootf(rgb).is_err());
    }
    let mut f = Frame {
        w: 2,
        h: 1,
        d: vec![0.1, 0.1, 0.1, 0.5, -0.1, 0.0, 0.0, 1.0],
    };
    let original = f.d.clone();
    assert!(t.apply(&mut f).is_err());
    assert_eq!(f.d, original);
    let mut f = frame([0.18; 3]);
    f.d[3] = 0.375;
    t.apply(&mut f).unwrap();
    assert_eq!(f.d[3].to_bits(), 0.375f32.to_bits());
}
#[test]
fn pq_nits_scale_and_metadata_consistency() {
    let m = metadata();
    let a = audit_pq_linear(&frame([10.0; 3]), 100.0, Some(&m)).unwrap();
    let b = audit_pq_linear(&frame([5.0; 3]), 200.0, Some(&m)).unwrap();
    assert_eq!(a.max_rgb_nits, Some(1000.0));
    assert_eq!(a.max_rgb_nits, b.max_rgb_nits);
    assert_eq!(a.max_cll_exceeded, Some(false));
    assert_eq!(a.max_fall_exceeded, Some(true));
    assert_eq!(a.mastering_peak_exceeded, Some(false));
    let x = audit_pq_linear(&frame([11.0, 0.0, 0.0]), 100.0, Some(&m)).unwrap();
    assert_eq!(x.max_cll_exceeded, Some(true));
    assert_eq!(x.max_fall_exceeded, Some(true));
    assert_eq!(x.mastering_peak_exceeded, Some(true));
    let mut unknown = m.clone();
    unknown.max_cll = 0;
    unknown.max_fall = 0;
    let x = audit_pq_linear(&frame([1.0; 3]), 100.0, Some(&unknown)).unwrap();
    assert_eq!(x.max_cll_exceeded, None);
    assert_eq!(x.max_fall_exceeded, None);
}
#[test]
fn excursions_invalid_samples_and_transparency_not_hidden() {
    let f = Frame {
        w: 5,
        h: 1,
        d: vec![
            -1.0,
            0.0,
            0.0,
            1.0,
            101.0,
            0.0,
            0.0,
            1.0,
            f32::NAN,
            0.0,
            0.0,
            1.0,
            1.0,
            1.0,
            1.0,
            2.0,
            1000.0,
            0.0,
            0.0,
            0.0,
        ],
    };
    let e = audit_pq_linear(&f, 100.0, Some(&metadata())).unwrap();
    assert_eq!(e.visible_pixels, 2);
    assert_eq!(e.negative_rgb_pixels, 1);
    assert_eq!(e.above_10000_nits_pixels, 1);
    assert_eq!(e.nonfinite_pixels, 1);
    assert_eq!(e.invalid_alpha_pixels, 1);
    assert_eq!(e.frame_average_max_rgb_nits, None);
    assert_eq!(e.max_cll_exceeded, None);
    for white in [f64::NAN, 0.0, 10001.0] {
        assert!(audit_pq_linear(&frame([0.0; 3]), white, None).is_err());
    }
    assert!(audit_pq_linear(
        &Frame {
            w: 0,
            h: 0,
            d: vec![]
        },
        100.0,
        None
    )
    .is_err());
}
#[test]
fn static_metadata_exact_serialization_and_independent_content_peak() {
    let m = metadata();
    assert_eq!(m.x265().unwrap(),"hdr10=1:master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,50):max-cll=1000,400");
    let mut x = m.clone();
    x.max_cll = 2000;
    assert!(x.validate().is_ok()); // content may exceed mastering peak
    x.max_fall = 2001;
    assert!(x.validate().is_err());
    let mut x = m.clone();
    x.min_nits = 0.00496;
    assert!(x.x265().unwrap().contains("L(10000000,50)"));
    let mut x = m.clone();
    x.min_nits = 1.0;
    x.max_nits = 1.000001;
    assert!(x.x265().is_err());
    let v = serde_json::to_value(&m).unwrap();
    let round: Mastering = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(round, m);
    let mut bad = v;
    bad["dynamic_metadata"] = serde_json::json!({});
    assert!(serde_json::from_value::<Mastering>(bad).is_err());
}
#[test]
fn read_only_diagnostics_and_identity_paths() {
    use kimo_core::{
        color::pipeline::output,
        commands::{dispatch, Session},
    };
    use serde_json::json;
    let mut s = Session::new();
    let before = serde_json::to_value(&s.project).unwrap();
    let v = dispatch(
        &mut s,
        "evaluate_hlg_reference_display",
        &json!({"peak_nits":1000,"scene_rgb":[1,1,1]}),
    )
    .unwrap();
    assert_eq!(v["display_rgb_nits"], json!([1000.0, 1000.0, 1000.0]));
    assert_eq!(v["gamma"], 1.2);
    assert_eq!(serde_json::to_value(&s.project).unwrap(), before);
    for args in [
        json!({}),
        json!({"peak_nits":1000,"scene_rgb":[1,1]}),
        json!({"peak_nits":1000,"scene_rgb":[-1,0,0]}),
        json!({"peak_nits":100,"scene_rgb":[0,0,0]}),
    ] {
        assert!(dispatch(&mut s, "evaluate_hlg_reference_display", &args).is_err());
        assert_eq!(serde_json::to_value(&s.project).unwrap(), before);
    }
    let mut f = Frame {
        w: 1,
        h: 1,
        d: vec![-0.2, 3.0, 0.18, 0.375],
    };
    let bits: Vec<_> = f.d.iter().map(|v| v.to_bits()).collect();
    output(&s.project, &mut f).unwrap();
    assert_eq!(bits, f.d.iter().map(|v| v.to_bits()).collect::<Vec<_>>());
    assert_eq!(
        kimo_core::color::managed::encode(7, 0.18, 100.0).to_bits(),
        kimo_core::color::managed::encode(7, 0.18, 203.0).to_bits()
    );
}
