//! v6 eyes: render_flow numbers (stats). Known references: a uniform half-resolution field (1.5, -0.5) on a 32x18 grid
//! for a 64x36 canvas reads (3, -1) canvas px per frame everywhere, speed sqrt(10), direction atan2(-1, 3) = -18.435 deg;
//! a zero field reads speed 0, no direction, nothing moving; render_flow's own dense flow between a smooth texture and the
//! same texture moved 4 px right reads a median vector of (4, 0) within 0.25 px; refused arguments.
use kimo_core::effects::Frame;
use kimo_core::ext::flow_stats::{params, stats, FParams};
use serde_json::{json, Value};

fn f(v: &Value) -> f64 { v.as_f64().unwrap_or_else(|| panic!("not a number: {v}")) }

#[test] fn uniform_field_in_canvas_pixels() {
    let (fw, fh) = (32, 18); let u = vec![1.5f32; fw * fh]; let v = vec![-0.5f32; fw * fh];
    let s = stats(&u, &v, fw, fh, 64, 36, &FParams::default());
    for k in ["mean_u", "median_u"] { assert!((f(&s[k]) - 3.0).abs() < 1e-6, "{k} {s}"); }
    for k in ["mean_v", "median_v"] { assert!((f(&s[k]) + 1.0).abs() < 1e-6, "{k} {s}"); }
    for k in ["mean_speed", "speed_p50", "speed_p90", "speed_p99", "speed_max"] { assert!((f(&s[k]) - 10f64.sqrt()).abs() < 1e-3, "{k} {s}"); }
    assert!((f(&s["mean_dir_deg"]) + 18.4349).abs() < 1e-3, "{s}"); assert_eq!(f(&s["moving_frac"]), 1.0);
    let cells = s["grid"]["cells"].as_array().unwrap(); assert_eq!(cells.len(), 16);
    for c in cells { assert!((f(&c[0]) - 3.0).abs() < 1e-6 && (f(&c[1]) + 1.0).abs() < 1e-6, "{c}"); }
    assert_eq!(s["field"], json!([32, 18])); assert_eq!(s["params"], json!({"grid": 4, "moving_px": 0.5}));
    // moving_px above the speed: nothing counts as moving; grid 2 gives 4 cells
    let s = stats(&u, &v, fw, fh, 64, 36, &FParams { grid: 2, moving_px: 4.0 }); assert_eq!(f(&s["moving_frac"]), 0.0); assert_eq!(s["grid"]["cells"].as_array().unwrap().len(), 4);
}

#[test] fn zero_field_and_one_fast_point() {
    let (fw, fh) = (20, 10); let mut u = vec![0f32; fw * fh]; let v = vec![0f32; fw * fh];
    let s = stats(&u, &v, fw, fh, 40, 20, &FParams::default());
    assert_eq!(f(&s["speed_max"]), 0.0); assert_eq!(s["mean_dir_deg"], Value::Null); assert_eq!(f(&s["moving_frac"]), 0.0);
    u[3 * fw + 7] = 5.0; let s = stats(&u, &v, fw, fh, 40, 20, &FParams::default());
    assert!((f(&s["speed_max"]) - 10.0).abs() < 1e-9, "{s}"); assert_eq!(s["speed_max_at"], json!([15.0, 7.0])); assert!((f(&s["moving_frac"]) - 1.0 / 200.0).abs() < 1e-9);
    assert_eq!(f(&s["speed_p50"]), 0.0); assert_eq!(f(&s["mean_dir_deg"]), 0.0);
}

fn texture(w: usize, h: usize, dx: f32) -> Frame {
    let mut fr = Frame::new(w, h);
    for y in 0..h { for x in 0..w { let (xf, yf) = (x as f32 - dx, y as f32);
        let g = 0.5 + 0.2 * (xf * 0.21).sin() * (yf * 0.17).cos() + 0.15 * ((xf + yf) * 0.11).sin() + 0.1 * (xf * 0.05 - yf * 0.07).cos();
        let i = (y * w + x) * 4; fr.d[i] = g; fr.d[i + 1] = g; fr.d[i + 2] = g; fr.d[i + 3] = 1.0; } }
    fr
}

#[test] fn known_translation_through_render_flows_own_flow() {
    let (w, h) = (192, 128); let a = texture(w, h, 0.0); let b = texture(w, h, 4.0);
    let (u, v, fw, fh) = kimo_core::flow::flow(&a, &b, 5);
    let s = stats(&u, &v, fw, fh, w, h, &FParams::default());
    eprintln!("translation 4 px: {s}");
    assert!((f(&s["median_u"]) - 4.0).abs() < 0.25, "{s}"); assert!(f(&s["median_v"]).abs() < 0.25, "{s}");
}

#[test] fn refused_arguments() {
    for v in [json!("yes"), json!(1), json!({"grid": 0}), json!({"grid": 17}), json!({"grid": 2.5}), json!({"moving_px": -1}), json!({"nope": 1})] {
        assert!(params(&v).is_err(), "accepted {v}");
    }
    assert!(params(&json!(false)).unwrap().is_none()); assert!(params(&Value::Null).unwrap().is_none());
    let p = params(&json!({"grid": 8, "moving_px": 2})).unwrap().unwrap(); assert_eq!(p.grid, 8); assert_eq!(p.moving_px, 2.0);
    // through dispatch: a bad stats argument is refused before any work, a missing clip fails as before
    let mut s = kimo_core::commands::Session::new();
    let e = kimo_core::commands::dispatch(&mut s, "render_flow", &json!({"clip_id": "nope", "time": 0.0, "path": "/tmp/x.png", "stats": {"grid": 0}})).unwrap_err(); assert!(e.contains("stats.grid"), "{e}");
    let e0 = kimo_core::commands::dispatch(&mut s, "render_flow", &json!({"clip_id": "nope", "time": 0.0, "path": "/tmp/x.png"})).unwrap_err();
    let e1 = kimo_core::commands::dispatch(&mut s, "render_flow", &json!({"clip_id": "nope", "time": 0.0, "path": "/tmp/x.png", "stats": true})).unwrap_err(); assert_eq!(e0, e1);
}
