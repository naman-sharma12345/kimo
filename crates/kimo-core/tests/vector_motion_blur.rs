//! Vector motion blur for footage (RSMB / Nuke VectorBlur class): a textured disc translating 10 px per frame over a static textured
//! background, rendered to a lossless clip, then blurred with a 180 degree centred shutter. Ground truth = the exact temporal integral of the
//! continuous scene over the shutter (64 time midpoints x 4x4 sub-pixels per pixel), independent of kimo. Gates, fixed before any run:
//! PSNR >= 35 dB over the motion region (every pixel within 3 px of anything the disc sweeps; whole-frame PSNR is printed too, but is not a gate:
//! the static background dominates it and the unblurred frame already scores 37 dB there); background leak (pixels more than 3 px from anything the disc sweeps) max error < 2/255;
//! edge band (pixels the disc partially covers during the shutter) mean abs error < 0.02.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{self, render_frame, Cache};
use serde_json::{json, Value};
use std::io::Write;

const W: usize = 160; const H: usize = 96; const FPS: f64 = 24.0; const NF: usize = 12; const R: f64 = 16.0; const VX: f64 = 10.0 * FPS; const CX0: f64 = 30.0; const CY: f64 = 48.0;
const K: usize = 6; // evaluated frame
fn bg(x: f64, y: f64) -> [f64; 3] { [0.35 + 0.25 * (x * 0.21).sin() * (y * 0.17).cos(), 0.40 + 0.20 * ((x + y) * 0.13).sin(), 0.50 + 0.20 * (x * 0.09 - y * 0.23).cos()] }
fn disc_tex(u: f64, v: f64) -> [f64; 3] { [0.90 + 0.08 * (u * 0.5).sin(), 0.20 + 0.15 * (v * 0.45).cos(), 0.15 + 0.10 * ((u + v) * 0.35).sin()] }
/// scene colour and disc coverage (0/1) at a point and time (seconds)
fn scene(x: f64, y: f64, t: f64, moving: bool) -> ([f64; 3], f64) { let cx = if moving { CX0 + VX * t } else { CX0 + VX * (K as f64 / FPS) }; let (u, v) = (x - cx, y - CY);
    if u * u + v * v <= R * R { (disc_tex(u, v), 1.0) } else { (bg(x, y), 0.0) } }
/// one pixel averaged over 4x4 sub-pixels and `nt` time midpoints across [t0, t0 + len]
fn pixel(i: usize, j: usize, t0: f64, len: f64, nt: usize, moving: bool) -> ([f64; 3], f64) { let (mut c, mut cov) = ([0.0; 3], 0.0); let n = (16 * nt) as f64;
    for k in 0..nt { let t = t0 + (k as f64 + 0.5) / nt as f64 * len; for sy in 0..4 { for sx in 0..4 { let (x, y) = (i as f64 + (sx as f64 + 0.5) / 4.0, j as f64 + (sy as f64 + 0.5) / 4.0);
        let (s, d) = scene(x, y, t, moving); for q in 0..3 { c[q] += s[q] / n; } cov += d / n; } } } (c, cov) }
fn write_clip(moving: bool) -> String {
    let path = std::env::temp_dir().join(format!("kimo-vblur-{}-{}.mkv", std::process::id(), moving)).to_str().unwrap().to_string();
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{W}x{H}"), "-r", "24", "-i", "-", "-c:v", "ffv1", "-pix_fmt", "bgr0", &path])
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    { let si = ch.stdin.as_mut().unwrap(); for f in 0..NF { let mut buf = Vec::with_capacity(W * H * 3);
        for j in 0..H { for i in 0..W { let (c, _) = pixel(i, j, f as f64 / FPS, 0.0, 1, moving); for q in 0..3 { buf.push((c[q].clamp(0.0, 1.0) * 255.0 + 0.5) as u8); } } } si.write_all(&buf).unwrap(); } }
    assert!(ch.wait().unwrap().success()); path
}
fn session(path: &str) -> (Session, String) {
    let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width": W, "height": H, "fps": FPS})).unwrap();
    let a = dispatch(&mut s, "import_media", &json!({"path": path})).unwrap()["asset_id"].clone(); let tr = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    let c = dispatch(&mut s, "add_clip", &json!({"track_id": tr, "source_type": "media", "asset_id": a, "start": 0, "duration": NF as f64 / FPS})).unwrap()["clip_id"].as_str().unwrap().to_string(); (s, c)
}
fn render_at(s: &Session, t: f64) -> Vec<f32> { let mut cache = Cache::new(); render::prepare(&s.project, &mut cache).unwrap(); render_frame(&s.project, t, &cache).unwrap().d }
struct M { psnr: f64, psnr_motion: f64, leak: f64, edge: f64, n_leak: usize, n_edge: usize }
fn metrics(d: &[f32]) -> M {
    let t = K as f64 / FPS; let sh = 0.5 / FPS; let mut ex = vec![[0.0; 3]; W * H]; let mut cov = vec![0.0; W * H];
    for j in 0..H { for i in 0..W { let (c, v) = pixel(i, j, t - sh / 2.0, sh, 64, true); ex[j * W + i] = c; cov[j * W + i] = v; } }
    let swept: Vec<bool> = cov.iter().map(|&v| v > 0.0).collect(); let mut near = vec![false; W * H];
    for j in 0..H { for i in 0..W { if !swept[j * W + i] { continue; } for dj in -3i64..=3 { for di in -3i64..=3 { let (x, y) = (i as i64 + di, j as i64 + dj); if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H { near[y as usize * W + x as usize] = true; } } } } }
    let (mut se, mut leak, mut edge, mut ne, mut nl, mut sm, mut nm) = (0.0, 0f64, 0.0, 0usize, 0usize, 0.0, 0usize);
    for p in 0..W * H { let mut e_px = 0f64; for q in 0..3 { let e = d[p * 4 + q] as f64 - ex[p][q]; se += e * e; if near[p] { sm += e * e; nm += 1; } e_px = e_px.max(e.abs()); }
        if !near[p] { leak = leak.max(e_px); nl += 1; } if cov[p] > 0.02 && cov[p] < 0.98 { edge += e_px; ne += 1; } }
    M { psnr: 10.0 * (1.0 / (se / (W * H * 3) as f64)).log10(), psnr_motion: 10.0 * (1.0 / (sm / nm as f64)).log10(), leak, edge: edge / ne.max(1) as f64, n_leak: nl, n_edge: ne }
}
fn have_ffmpeg() -> bool { std::process::Command::new("ffmpeg").arg("-version").output().is_ok() }
fn blur(s: &mut Session, c: &str, extra: Value) -> Result<Value, String> { let mut a = json!({"clip_id": c, "amount": 0.5}); for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); } dispatch(s, "set_flow_blur", &a) }

#[test] fn vector_blur_meets_the_disc_gates() {
    if !have_ffmpeg() { eprintln!("SKIPPED: ffmpeg not found"); return; }
    let path = write_clip(true); let t = K as f64 / FPS + 1e-4;
    let (s, _) = session(&path); let m0 = metrics(&render_at(&s, t)); println!("no blur      : motion-region PSNR {:.2} dB (frame {:.2}) | bg leak max {:.5} ({} px) | edge band MAE {:.5} ({} px)", m0.psnr_motion, m0.psnr, m0.leak, m0.n_leak, m0.edge, m0.n_edge);
    let (mut s, c) = session(&path); blur(&mut s, &c, json!({"quality": 4})).unwrap(); let m1 = metrics(&render_at(&s, t)); println!("flow_blur    : motion-region PSNR {:.2} dB (frame {:.2}) | bg leak max {:.5} | edge band MAE {:.5}", m1.psnr_motion, m1.psnr, m1.leak, m1.edge);
    let (mut s, c) = session(&path); blur(&mut s, &c, json!({"quality": 4, "mode": "vector"})).unwrap(); let t0 = std::time::Instant::now(); let d = render_at(&s, t); let ms = t0.elapsed().as_secs_f64() * 1e3;
    let m2 = metrics(&d); println!("vector       : motion-region PSNR {:.2} dB (frame {:.2}) | bg leak max {:.5} | edge band MAE {:.5} | {:.0} ms incl. decode", m2.psnr_motion, m2.psnr, m2.leak, m2.edge, ms);
    let _ = std::fs::remove_file(&path);
    assert!(m2.psnr_motion >= 35.0, "vector motion-region PSNR {:.2} dB < 35", m2.psnr_motion); assert!(m2.leak < 2.0 / 255.0, "vector bg leak {:.5} >= 2/255", m2.leak); assert!(m2.edge < 0.02, "vector edge MAE {:.5} >= 0.02", m2.edge);
}

#[test] fn vector_blur_on_static_footage_is_bit_identical() {
    if !have_ffmpeg() { eprintln!("SKIPPED: ffmpeg not found"); return; }
    let path = write_clip(false); let t = K as f64 / FPS + 1e-4;
    let (s, _) = session(&path); let off = render_at(&s, t); let (mut s, c) = session(&path); blur(&mut s, &c, json!({"mode": "vector"})).unwrap(); let on = render_at(&s, t); let _ = std::fs::remove_file(&path);
    assert!(off.iter().zip(&on).all(|(a, b)| a.to_bits() == b.to_bits()), "static footage: vector blur changed pixels");
}

#[test] fn vector_mode_refusals() {
    if !have_ffmpeg() { eprintln!("SKIPPED: ffmpeg not found"); return; }
    let path = write_clip(false); let (mut s, c) = session(&path); let before = serde_json::to_string(&s.project).unwrap();
    for (a, want) in [(json!({"mode": "zigzag"}), "mode must be smear or vector"), (json!({"mode": "vector", "shutter_curve": "zigzag"}), "shutter_curve must be box, triangle, gaussian or custom")] {
        let e = blur(&mut s, &c, a.clone()).expect_err(&format!("accepted {a}")); assert!(e.contains(want), "{a}: '{e}'"); assert_eq!(serde_json::to_string(&s.project).unwrap(), before); }
    let _ = std::fs::remove_file(&path);
}
