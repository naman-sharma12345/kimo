//! v6 eyes: saliency_map. Known references: a flat field has no saliency (empty: true, no peak); a Gaussian blob on a flat
//! 64x36 grid (inside the top right third, off the thirds lines) peaks within one cell of the blob centre; a constant
//! frame gives an all-zero map (min-max guard); through the tool, a white disc (r 12 px) at (480, 120) on a lightly
//! grained grey 640x360 picture (128 +-4 levels, deterministic; a 24-bit BMP written by the test and imported as media) peaks within 15 px of
//! the disc centre, and a 48 px box around the disc holds far more mass per area than the same box on empty grey;
//! motion on a still picture reads zero change; refused arguments.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::saliency_map::{area_down, minmax_scale, spectral_residual, stats};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn f(v: &Value) -> f64 { v.as_f64().unwrap_or_else(|| panic!("not a number: {v}")) }

#[test] fn flat_field_has_no_saliency() {
    let g = vec![0.5; 64 * 36]; let s = spectral_residual(&g, 64, 36, 2.5);
    assert!(s.iter().all(|v| *v == 0.0)); let st = stats(&s, 64, 36, 640, 360, &[[0.0, 0.0, 64.0, 36.0]], 3);
    assert_eq!(st["empty"], json!(true)); assert_eq!(st["peak"], Value::Null); assert_eq!(st["rects"][0]["mass_frac"], Value::Null);
}

#[test] fn blob_on_flat_grid_peaks_at_the_blob() {
    // blob centre (53, 6): inside the top right third (x 42.7..64, y 0..12), clear of the thirds lines
    let (w, h) = (64usize, 36usize); let (bx, by) = (53.0, 6.0); let mut g = vec![0.5; w * h];
    for y in 0..h { for x in 0..w { let d2 = (x as f64 - bx).powi(2) + (y as f64 - by).powi(2); g[y * w + x] += 0.4 * (-d2 / (2.0 * 1.5 * 1.5)).exp(); } }
    let s = spectral_residual(&g, w, h, 2.5); let st = stats(&s, w, h, w, h, &[], 3);
    eprintln!("blob grid: {st}");
    assert!((f(&st["peak"]["x"]) - (bx + 0.5)).abs() <= 1.0 && (f(&st["peak"]["y"]) - (by + 0.5)).abs() <= 1.0, "{st}");
    assert_eq!(f(&st["peak"]["value"]), 1.0);
    let m: f64 = st["grid"]["mass"].as_array().unwrap().iter().map(f).sum(); assert!((m - 1.0).abs() < 1e-3, "{m}");
    assert!(f(&st["grid"]["mass"][2]) > 0.5, "top right third holds most of the mass: {st}");
}

fn disc_bmp() -> std::path::PathBuf {
    // 24-bit BMP (on kimo's image list), rows bottom-up, 640 * 3 bytes per row (already a multiple of 4).
    // Light grain on the grey (128 + -4..3 levels from a fixed LCG): spectral residual is ill-conditioned on a perfectly flat picture.
    let (w, h) = (640usize, 360usize); let size = 54 + w * h * 3; let mut b = Vec::with_capacity(size);
    b.extend_from_slice(b"BM"); b.extend_from_slice(&(size as u32).to_le_bytes()); b.extend_from_slice(&[0; 4]); b.extend_from_slice(&54u32.to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes()); b.extend_from_slice(&(w as i32).to_le_bytes()); b.extend_from_slice(&(h as i32).to_le_bytes()); b.extend_from_slice(&1u16.to_le_bytes()); b.extend_from_slice(&24u16.to_le_bytes());
    b.extend_from_slice(&[0; 4]); b.extend_from_slice(&((w * h * 3) as u32).to_le_bytes()); b.extend_from_slice(&[0; 16]);
    let mut st: u64 = 0x9E37_79B9_7F4A_7C15;
    for yy in 0..h { let y = h - 1 - yy; for x in 0..w {
        st = st.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); let grain = (st >> 61) as i32 - 4;
        let d = ((x as f64 + 0.5 - 480.0).powi(2) + (y as f64 + 0.5 - 120.0).powi(2)).sqrt(); let v = if d <= 12.0 { 255u8 } else { (128 + grain) as u8 }; b.extend_from_slice(&[v, v, v]); } }
    let p = std::env::temp_dir().join(format!("kimo_sal_{}_disc.bmp", std::process::id())); std::fs::write(&p, b).unwrap(); p
}

#[test] fn disc_through_the_tool() {
    let bmp = disc_bmp();
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 640, "height": 360, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); let a = c(&mut s, "import_media", json!({"path": bmp.to_str().unwrap()}))["asset_id"].clone();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0.0, "duration": 2.0}));
    let png = std::env::temp_dir().join(format!("kimo_sal_{}_map.png", std::process::id()));
    let r = c(&mut s, "saliency_map", json!({"time": 0.5, "rects": [[456, 96, 48, 48], [40, 240, 48, 48]], "path": png.to_str().unwrap()}));
    eprintln!("disc: {r}");
    assert!((f(&r["peak"]["x"]) - 480.0).abs() <= 15.0 && (f(&r["peak"]["y"]) - 120.0).abs() <= 15.0, "{r}");
    assert_eq!(r["work"], json!([64, 36])); assert_eq!(r["canvas"], json!([640, 360])); assert_eq!(r["motion"], Value::Null);
    let (a0, a1) = (&r["rects"][0], &r["rects"][1]);
    assert!(f(&a0["mass_per_area"]) > 5.0 && f(&a0["mass_per_area"]) > 10.0 * f(&a1["mass_per_area"]).max(1e-6), "{r}");
    assert!(f(&a0["top10_frac"]) > f(&a1["top10_frac"]), "{r}"); assert_eq!(f(&a0["area_frac"]), 0.01);
    assert!(std::fs::metadata(&png).map(|m| m.len() > 0).unwrap_or(false), "map png written");
    // the same call again reads the same numbers (path aside)
    let r2 = c(&mut s, "saliency_map", json!({"time": 0.5, "rects": [[456, 96, 48, 48], [40, 240, 48, 48]]}));
    let mut r1 = r.clone(); r1.as_object_mut().unwrap().remove("path"); assert_eq!(r1, r2);
    // motion on a still picture: zero change; with weight 1 nothing is left
    let m = c(&mut s, "saliency_map", json!({"time": 0.5, "motion": 1.0}));
    assert_eq!(f(&m["motion"]["mean_delta"]), 0.0); assert_eq!(f(&m["motion"]["max_delta"]), 0.0); assert_eq!(m["empty"], json!(true));
    let _ = std::fs::remove_file(&png); let _ = std::fs::remove_file(&bmp);
}

#[test] fn constant_frame_gives_an_all_zero_map() {
    // min-max guard: max == min returns all zeros (no divide by zero, no NaN), peak value 0, for any constant
    for c in [0.0, 0.37, 1.0, 1e6] {
        let mut v = vec![c; 64 * 36]; minmax_scale(&mut v);
        assert!(v.iter().all(|x| x.is_finite() && *x == 0.0), "minmax_scale of constant {c}"); assert_eq!(v.iter().cloned().fold(f64::MIN, f64::max), 0.0, "peak value, constant {c}");
        let g = area_down(&vec![c; 640 * 360], 640, 360, 64, 36); let s = spectral_residual(&g, 64, 36, 2.5);
        assert!(s.iter().all(|x| x.is_finite() && *x == 0.0), "saliency of constant frame {c}"); assert_eq!(s.iter().cloned().fold(f64::MIN, f64::max), 0.0, "peak value, frame {c}");
    }
}

#[test] fn refused_arguments() {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    for a in [json!({}), json!({"time": -1}), json!({"time": 0, "work_px": 8}), json!({"time": 0, "blur_px": 40}), json!({"time": 0, "motion": 2}), json!({"time": 0, "grid": 0}),
              json!({"time": 0, "rects": [[0, 0, 65, 10]]}), json!({"time": 0, "rects": [[0, 0, 0, 10]]}), json!({"time": 0, "rects": "x"}), json!({"time": 0, "path": ""})] {
        assert!(dispatch(&mut s, "saliency_map", &a).is_err(), "accepted {a}");
    }
}
