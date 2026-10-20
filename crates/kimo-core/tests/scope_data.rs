//! Scopes as exact data (render_display_scope data:true) next to the PNG: waveform, RGB parade, histogram and a Rec.709
//! vectorscope grid with 100%/75% targets, checked against synthetic frames whose every bin is worked out here.
//! Legacy (SDR) projects: IRE = encoded value * 100, ceiling 100; bins=101 makes bin = round(IRE).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
const W: usize = 64; const H: usize = 16;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn proj(clip: Value) -> Session { let mut s = Session::new(); c(&mut s, "project_set", json!({"width":W,"height":H,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); let mut a = clip; a["track_id"] = t; a["start"] = json!(0); a["duration"] = json!(2); c(&mut s, "add_clip", a); s }
fn solid(rgb: [f64; 3]) -> Session { proj(json!({"source_type":"solid","color":rgb})) }
fn scope(s: &mut Session, extra: Value) -> Value { let p = std::env::temp_dir().join(format!("kimo_scope_data_{}.png", std::process::id()));
    let mut a = json!({"time":0.5,"scope":"vectorscope","path":p.to_str().unwrap(),"data":true,"bins":101,"columns":W}); for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); }
    let r = c(s, "render_display_scope", a); assert!(r["data"].is_object(), "no data in result: {r}"); r["data"].clone() }
fn u(v: &Value) -> u64 { v.as_u64().unwrap_or_else(|| panic!("not a count: {v}")) }

#[test]
fn waveform_of_a_horizontal_ramp_is_one_bin_per_column() {
    // gradient black -> white at 90 deg: column x has value (x+0.5)/64 -> IRE 1.5625*(x+0.5); never within 1/32 of a bin edge
    let mut s = proj(json!({"source_type":"gradient","color":[0,0,0],"color2":[1,1,1],"angle":90}));
    let d = scope(&mut s, json!({"scope":"waveform"}));
    for x in 0..W { let want = ((x as f64 + 0.5) * 100.0 / W as f64 + 0.5).floor() as usize; let col = d["waveform"]["counts"][x].as_array().unwrap();
        for (b, n) in col.iter().enumerate() { assert_eq!(u(n), if b == want { H as u64 } else { 0 }, "waveform col {x} bin {b} (want all {H} at {want})"); }
        for ch in ["r", "g", "b"] { assert_eq!(u(&d["parade"][ch][x][want]), H as u64, "parade {ch} col {x}"); } }
    assert_eq!(u(&d["pixels"]["visible"]), (W * H) as u64);
}

#[test]
fn parade_and_histogram_of_a_solid_are_exact() {
    let mut s = solid([0.25, 0.5, 0.75]); let d = scope(&mut s, json!({"scope":"parade"}));
    // luma 0.2126*25 + 0.7152*50 + 0.0722*75 = 46.49 IRE -> bin 46
    for (ch, bin) in [("r", 25), ("g", 50), ("b", 75), ("luma", 46)] { let hist = d["histogram"][ch].as_array().unwrap();
        for (b, n) in hist.iter().enumerate() { assert_eq!(u(n), if b == bin { (W * H) as u64 } else { 0 }, "histogram {ch} bin {b}"); } }
    for x in [0, 17, W - 1] { assert_eq!(u(&d["parade"]["r"][x][25]), H as u64); assert_eq!(u(&d["parade"]["g"][x][50]), H as u64); assert_eq!(u(&d["parade"]["b"][x][75]), H as u64); assert_eq!(u(&d["waveform"]["counts"][x][46]), H as u64); }
}

#[test]
fn vectorscope_targets_are_rec709_and_bars_land_on_them() {
    let (kr, kb) = (0.2126f64, 0.0722f64);
    let cbcr = |r: f64, g: f64, b: f64| { let y = kr * r + 0.7152 * g + kb * b; [(b - y) / 1.8556, (r - y) / 1.5748] };
    for (name, rgb) in [("R", [1.0, 0.0, 0.0]), ("Y", [1.0, 1.0, 0.0]), ("G", [0.0, 1.0, 0.0]), ("C", [0.0, 1.0, 1.0]), ("B", [0.0, 0.0, 1.0]), ("M", [1.0, 0.0, 1.0])] {
        for (lvl, k) in [("100", 1.0), ("75", 0.75)] {
            let rgbk = [rgb[0] * k, rgb[1] * k, rgb[2] * k]; let mut s = solid(rgbk); let d = scope(&mut s, json!({"grid":64}));
            let t = &d["vectorscope"]["targets"][lvl][name]; let e = cbcr(rgbk[0], rgbk[1], rgbk[2]);
            for i in 0..2 { assert!((t["cbcr"][i].as_f64().unwrap() - e[i]).abs() < 1e-12, "{lvl}% {name} target {t} vs {e:?}"); }
            let (row, col) = (u(&t["cell"][0]) as usize, u(&t["cell"][1]) as usize);
            assert_eq!(col, (((e[0] + 0.5) * 64.0).floor() as usize).min(63)); assert_eq!(row, (((0.5 - e[1]) * 64.0).floor() as usize).min(63));
            assert_eq!(u(&d["vectorscope"]["counts"][row][col]), (W * H) as u64, "{lvl}% {name} bar not on its target cell");
        } }
    // 100% red sits on the Cr = +0.5 edge: row 0; neutral grey sits in the centre cell
    let mut s = solid([0.4, 0.4, 0.4]); let d = scope(&mut s, json!({"grid":64})); assert_eq!(u(&d["vectorscope"]["counts"][32][32]), (W * H) as u64);
    assert_eq!(d["vectorscope"]["skin_line_deg"].as_f64(), Some(123.0));
}

#[test]
fn png_is_unchanged_by_data_and_refusals() {
    let mut s = solid([0.6, 0.3, 0.2]); let dir = std::env::temp_dir(); let pid = std::process::id();
    let (a, b) = (dir.join(format!("kimo_sd_a_{pid}.png")), dir.join(format!("kimo_sd_b_{pid}.png")));
    for sc in ["waveform", "parade", "vectorscope", "histogram"] {
        c(&mut s, "render_display_scope", json!({"time":0.5,"scope":sc,"path":a.to_str().unwrap()}));
        c(&mut s, "render_display_scope", json!({"time":0.5,"scope":sc,"path":b.to_str().unwrap(),"data":true,"bins":512,"grid":256}));
        assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap(), "{sc}: data:true changed the PNG"); }
    for (bad, want) in [(json!({"bins":8}), "bins"), (json!({"grid":2000}), "grid"), (json!({"columns":0}), "columns"), (json!({"data":"yes"}), "data")] {
        let mut x = json!({"time":0.5,"scope":"waveform","path":a.to_str().unwrap()}); for (k, v) in bad.as_object().unwrap() { x[k] = v.clone(); }
        let e = dispatch(&mut s, "render_display_scope", &x).expect_err(&format!("{bad} must be refused")); assert!(e.to_string().contains(want), "{bad}: '{e}'"); }
    let _ = std::fs::remove_file(&a); let _ = std::fs::remove_file(&b);
}
