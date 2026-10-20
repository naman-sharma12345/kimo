//! `stabilize` end to end through the command, on a short lossless synthetic plate (ffv1 in .mkv, 48 frames 256x144, 24 fps)
//! with an exact ground-truth camera: classic args reach the classic engine untouched; the warp engine keys x/y/scale/rotation
//! (or pin corners) at every frame, adds the crop mask, and its residual jitter measured from the KEYS (the transform_bb convention
//! rebuilt from x/y/scale/rotation) is gated; synthesize_edges fills the stabilize_only borders with pixels checked against the analytic
//! texture; refusals leave the project unchanged.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::robust_est::{self, Model};
use kimo_core::trackmath::{self, H3, P};
use serde_json::{json, Value};
use std::f64::consts::TAU;

const W: usize = 256; const H: usize = 144; const N: usize = 48; const FPS: f64 = 24.0;
const CX: f64 = W as f64 * 0.5; const CY: f64 = H as f64 * 0.5;

struct Rng(u64);
impl Rng {
    fn new(s: u64) -> Rng { let mut r = Rng(s.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1); for _ in 0..8 { r.next(); } r }
    fn next(&mut self) -> u64 { let mut x = self.0; x ^= x << 13; x ^= x >> 7; x ^= x << 17; self.0 = x; x }
    fn f(&mut self) -> f64 { (self.next() >> 11) as f64 / (1u64 << 53) as f64 }
    fn range(&mut self, a: f64, b: f64) -> f64 { a + (b - a) * self.f() }
}
fn tr(x: f64, y: f64) -> H3 { [[1.0, 0.0, x], [0.0, 1.0, y], [0.0, 0.0, 1.0]] }
fn sim_c(ls: f64, th: f64, tx: f64, ty: f64) -> H3 { let s = ls.exp(); trackmath::mul(&trackmath::mul(&tr(CX, CY), &[[s * th.cos(), -s * th.sin(), tx], [s * th.sin(), s * th.cos(), ty], [0.0, 0.0, 1.0]]), &tr(-CX, -CY)) }
fn ap(h: &H3, p: P) -> P { trackmath::apply(h, p).unwrap() }

/// Analytic texture (sinusoids with wavelengths 6..48 px and Gaussian blobs) and a jittered camera: T_t = J_t G_t about the centre.
struct Plate { waves: Vec<[f64; 4]>, blobs: Vec<[f64; 4]>, jit: Vec<Vec<(f64, f64, f64)>> }
impl Plate {
    fn new() -> Plate { let mut r = Rng::new(77);
        let waves = (0..60).map(|_| { let k = TAU / (6.0 * 8f64.powf(r.f())); let a = r.range(0.0, TAU); [k * a.cos(), k * a.sin(), r.range(0.0, TAU), 5.5] }).collect::<Vec<_>>();
        let blobs = (0..100).map(|_| { let a = r.range(40.0, 80.0); [r.range(-80.0, 380.0), r.range(-80.0, 240.0), r.range(2.0, 6.0), if r.f() < 0.5 { -a } else { a }] }).collect::<Vec<_>>();
        let s2 = 2f64.sqrt(); let mut jit = vec![]; for q in [2.0 / s2, 2.0 / s2, 0.3f64.to_radians(), 0.003] { jit.push((0..4).map(|_| (q / s2, r.range(2.0, 8.0) / FPS, r.range(0.0, TAU))).collect::<Vec<_>>()); }
        Plate { waves, blobs, jit } }
    fn tex(&self, x: f64, y: f64) -> f64 { let mut s = 128.0; for w in &self.waves { s += w[3] * (w[0] * x + w[1] * y + w[2]).sin(); } for b in &self.blobs { s += b[3] * (-((x - b[0]).powi(2) + (y - b[1]).powi(2)) / (2.0 * b[2] * b[2])).exp(); } s }
    fn jv(&self, k: usize, t: f64) -> f64 { self.jit[k].iter().map(|&(a, f, ph)| a * (TAU * f * t + ph).sin()).sum() }
    fn g(&self, t: f64) -> H3 { sim_c(0.0, (0.01 * t).to_radians(), -0.8 * t, -0.3 * t) }
    fn cam(&self, t: f64) -> H3 { trackmath::mul(&sim_c(self.jv(3, t), self.jv(2, t), self.jv(0, t), self.jv(1, t)), &self.g(t)) }
    fn frame_rgb(&self, t: usize) -> Vec<u8> { let ti = trackmath::inv3(&self.cam(t as f64)).unwrap(); let mut o = Vec::with_capacity(W * H * 3);
        for v in 0..H { for u in 0..W { let q = ap(&ti, (u as f64, v as f64)); let g = self.tex(q.0, q.1).round().clamp(0.0, 255.0) as u8; o.extend_from_slice(&[g, g, g]); } } o }
    fn grid(&self) -> Vec<P> { let ti = trackmath::inv3(&self.cam(0.0)).unwrap(); let mut v = vec![]; for j in 0..5 { for i in 0..7 { v.push(ap(&ti, (W as f64 * (0.2 + 0.6 * i as f64 / 6.0), H as f64 * (0.2 + 0.6 * j as f64 / 4.0)))); } } v }
}
fn plate_path() -> std::path::PathBuf {
    static P: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    P.get_or_init(|| { use std::io::Write; let path = std::env::temp_dir().join(format!("kimo-stab-{}.mkv", std::process::id())); let pl = Plate::new();
        let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{W}x{H}"), "-r", "24", "-i", "-", "-c:v", "ffv1", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().expect("ffmpeg");
        { let mut si = ch.stdin.take().unwrap(); for t in 0..N { si.write_all(&pl.frame_rgb(t)).unwrap(); } }
        assert!(ch.wait().unwrap().success(), "ffmpeg failed to write the plate"); path }).clone()
}
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn setup() -> (Session, String) {
    let path = plate_path(); let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": FPS}));
    let a = c(&mut s, "import_media", json!({"path": path}))["asset_id"].clone(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": N as f64 / FPS}))["clip_id"].as_str().unwrap().to_string(); (s, id)
}
fn keys(s: &Session, id: &str, prop: &str) -> Vec<(f64, f64)> { s.project.find_clip(id).unwrap().props.get(prop).map(|p| p.keys.iter().map(|k| (k.t, k.v)).collect()).unwrap_or_default() }
/// B_t rebuilt from the x/y/scale/rotation keys with transform_bb's convention: out = R(rot) scale (p - c) + c + (x, y).
fn b_from_keys(s: &Session, id: &str) -> Vec<H3> {
    let (x, y, sc, r) = (keys(s, id, "x"), keys(s, id, "y"), keys(s, id, "scale"), keys(s, id, "rotation")); assert_eq!(x.len(), N, "x keys at every frame");
    (0..N).map(|i| { let (k, th) = (sc[i].1, r[i].1.to_radians()); let (a, b) = (k * th.cos(), k * th.sin()); [[a, -b, CX + x[i].1 - (a * CX - b * CY)], [b, a, CY + y[i].1 - (b * CX + a * CY)], [0.0, 0.0, 1.0]] }).collect()
}
/// Residual jitter RMS (px) of the output against the true smooth path: e_t = B_t T_t x - K G_t x (one similarity K), high-passed
/// by e - Gauss_5(e) with odd reflection at the ends; the 7x5 grid in the central 60% of frame 0.
fn residual_jitter(pl: &Plate, b: &[H3]) -> f64 {
    let xs = pl.grid(); let (n, m) = (b.len(), xs.len());
    let q: Vec<Vec<P>> = (0..n).map(|t| xs.iter().map(|x| ap(&b[t], ap(&pl.cam(t as f64), *x))).collect()).collect(); let g: Vec<Vec<P>> = (0..n).map(|t| xs.iter().map(|x| ap(&pl.g(t as f64), *x)).collect()).collect();
    let (gs, qs): (Vec<P>, Vec<P>) = (0..n).flat_map(|t| (0..m).map(move |k| (t, k))).map(|(t, k)| (g[t][k], q[t][k])).unzip(); let kk = robust_est::refit(Model::Similarity, &gs, &qs).unwrap();
    let r = 15i64; let ws: Vec<f64> = (-r..=r).map(|k| (-((k * k) as f64) / 50.0).exp()).collect(); let wsum: f64 = ws.iter().sum(); let mut acc = 0.0;
    for k in 0..m { for c in 0..2 { let e: Vec<f64> = (0..n).map(|t| { let f = ap(&kk, g[t][k]); if c == 0 { q[t][k].0 - f.0 } else { q[t][k].1 - f.1 } }).collect(); let ni = n as i64;
        let at = |i: i64| if i < 0 { 2.0 * e[0] - e[(-i).min(ni - 1) as usize] } else if i >= ni { 2.0 * e[n - 1] - e[(2 * (ni - 1) - i).max(0) as usize] } else { e[i as usize] };
        for t in 0..ni { let lp = (-r..=r).map(|d| ws[(d + r) as usize] * at(t + d)).sum::<f64>() / wsum; acc += (e[t as usize] - lp).powi(2); } } }
    (acc / (n * m) as f64).sqrt()
}

#[test] fn classic_args_reach_the_classic_engine_unchanged() {
    let (mut s, id) = setup(); let direct = kimo_core::tracking::stabilize(&s.project, &id, 8.0, false, true).unwrap();
    let r = c(&mut s, "stabilize", json!({"clip_id": id, "smooth": 8})); assert!(r.get("max_zoom").is_some() && r.get("engine").is_none(), "{r}");
    let want = |f: &dyn Fn(&kimo_core::tracking::StabSample) -> f64| direct.iter().map(|d| (d.t, f(d))).collect::<Vec<_>>();
    assert_eq!(keys(&s, &id, "x"), want(&|d| d.dx)); assert_eq!(keys(&s, &id, "y"), want(&|d| d.dy)); assert_eq!(keys(&s, &id, "rotation"), want(&|d| d.rot_deg)); assert_eq!(keys(&s, &id, "scale"), want(&|d| d.scale));
    assert!(s.project.find_clip(&id).unwrap().mask.is_none() && !s.project.find_clip(&id).unwrap().props.contains_key("pin_on"));
}
#[test] fn warp_similarity_keys_every_frame_and_beats_the_jitter_gate() {
    let pl = Plate::new(); let (mut s, id) = setup();
    let r = c(&mut s, "stabilize", json!({"clip_id": id, "method": "position_scale_rotation", "smoothness": 50}));
    assert_eq!(r["engine"], "warp"); assert_eq!(r["frames"], N); assert!(r["zoom"].as_f64().unwrap() >= 1.0, "{r}");
    for p in ["x", "y", "scale", "rotation"] { let k = keys(&s, &id, p); assert_eq!(k.len(), N, "{p}"); for (i, (t, _)) in k.iter().enumerate() { assert!((t - i as f64 / FPS).abs() < 1e-9); } }
    let warp = residual_jitter(&pl, &b_from_keys(&s, &id)); let input = residual_jitter(&pl, &vec![trackmath::IDENT; N]);
    let (mut s2, id2) = setup(); c(&mut s2, "stabilize", json!({"clip_id": id2})); let classic = residual_jitter(&pl, &b_from_keys(&s2, &id2));
    println!("stabilize_tool residual_jitter_rms warp={warp:.4} classic={classic:.4} input={input:.4} zoom={} reanchors={}", r["zoom"], r["reanchors"]);
    assert!(input > 0.5, "the plate is not shaky: {input}"); assert!(warp <= 0.10, "warp residual jitter {warp} px RMS (classic {classic}, input {input})");
}
#[test] fn warp_perspective_keys_the_pin_corners() {
    let (mut s, id) = setup(); let r = c(&mut s, "stabilize", json!({"clip_id": id, "method": "perspective", "framing": "crop_auto_scale"})); assert_eq!(r["method"], "perspective");
    let cl = s.project.find_clip(&id).unwrap(); assert!(cl.props["pin_on"].at(0.5) >= 0.5);
    for i in 0..4 { for a in ["x", "y"] { assert_eq!(keys(&s, &id, &format!("pin{i}{a}")).len(), N, "pin{i}{a}"); } }
    // auto-scale zooms in: each pinned quad is at least as large as the canvas
    let k = |p: &str| keys(&s, &id, p); let (x0, x1, y0, y2) = (k("pin0x"), k("pin1x"), k("pin0y"), k("pin2y"));
    for i in 0..N { assert!(x1[i].1 - x0[i].1 >= W as f64 - 1.0 && y2[i].1 - y0[i].1 >= H as f64 - 1.0, "frame {i}: pinned quad smaller than the canvas"); }
    assert!(s.project.find_clip(&id).unwrap().props.get("x").map_or(true, |p| p.keys.is_empty()), "perspective leaves x/y/scale/rotation untouched");
}
#[test] fn warp_crop_adds_the_stabilize_crop_mask() {
    let (mut s, id) = setup(); let r = c(&mut s, "stabilize", json!({"clip_id": id, "method": "similarity", "framing": "crop"})); let crop = r["crop"].as_array().expect("crop rect").clone(); assert_eq!(crop.len(), 4); assert_eq!(r["zoom"], 1.0);
    let cl = s.project.find_clip(&id).unwrap(); let ls: Vec<_> = cl.mask.as_ref().expect("mask").layers.iter().filter(|l| l.name == "stabilize_crop").collect(); assert_eq!(ls.len(), 1);
    assert_eq!(ls[0].kind, "path"); assert_eq!(ls[0].path_keys.len(), N); assert!(ls[0].path_keys.iter().all(|k| k.points.len() == 4)); assert_eq!(ls[0].props["feather"].base, 0.0);
    // a second run with another framing removes the stale layer
    c(&mut s, "stabilize", json!({"clip_id": id, "framing": "stabilize_only"})); assert!(s.project.find_clip(&id).unwrap().mask.as_ref().unwrap().layers.iter().all(|l| l.name != "stabilize_crop"));
}
#[test] fn refusals_leave_the_project_unchanged() {
    let (mut s, id) = setup(); let tr = s.project.tracks[0].id.clone(); let sh = c(&mut s, "add_shape", json!({"track_id": tr, "shape": "rect", "width": 40, "height": 40, "start": 3, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    let before = serde_json::to_string(&s.project).unwrap();
    for (a, want) in [(json!({"clip_id": id, "method": "bogus"}), "position_scale_rotation"), (json!({"clip_id": id, "smoothness": 101}), "0..100"), (json!({"clip_id": id, "smoothness": -1}), "0..100"),
        (json!({"clip_id": id, "smooth": 5, "method": "perspective"}), "classic engine"), (json!({"clip_id": id, "max_features": 10}), "50..2000"), (json!({"clip_id": id, "mode": "locked"}), "no_motion"),
        (json!({"clip_id": sh, "method": "position"}), "media clip")] {
        let e = dispatch(&mut s, "stabilize", &a).expect_err(&format!("accepted {a}")); assert!(e.contains(want), "{a}: {e}");
        assert_eq!(before, serde_json::to_string(&s.project).unwrap(), "{a}: a refused call changed the project"); }
}

/// Frame at timeline time t on a TRANSPARENT canvas (render_frame_opts(.., true)): the canvas keeps the clip's alpha, which an opaque
/// render (render_frame / render_png) composites onto black. Colour is premultiplied there: gray = rgb / alpha. Also writes the
/// opaque PNG of the same frame to the temp dir for eyeballing.
fn render_t(s: &Session, t: f64, tag: &str) -> Frame {
    let mut cache = kimo_core::render::Cache::new(); kimo_core::render::prepare_at(&s.project, &mut cache, Some(t)).unwrap();
    let f = kimo_core::render::render_frame_opts(&s.project, t, &cache, true).unwrap(); assert_eq!((f.w, f.h), (W, H));
    let png = std::env::temp_dir().join(format!("kimo-stab-synth-{}-{tag}-{:02}.png", std::process::id(), (t * FPS).round())); kimo_core::render::render_png(&s.project, t, png.to_str().unwrap(), &mut cache).unwrap(); f
}
fn gray(f: &Frame, i: usize) -> f64 { let a = f.d[i + 3] as f64; if a > 0.0 { f.d[i] as f64 / a * 255.0 } else { 0.0 } }
/// Synthesize Edges against the exact ground truth (the world is static): output pixel o at frame t shows tex(T_t^-1 B_t^-1 o), with
/// B_t rebuilt from the keys (pixel-index coordinates, transform_bb's convention, the same as residual_jitter). Uncovered = alpha < 0.5
/// in the stabilize_only render of the same corrections; the fill must cover them (alpha >= 0.99) with pixels as accurate as the
/// interior (2 px band at the frame-t image edge excluded), and must not touch the interior.
#[test] fn synthesize_edges_fills_borders_with_the_right_pixels() { synth_case(None, 7); }
#[test] fn synthesize_edges_fills_borders_with_the_right_pixels_range_05() { synth_case(Some(0.5), 12); }
/// range None = the default synthesis_range 0.3 s (7 frames at 24 fps)
fn synth_case(range: Option<f64>, nf: usize) {
    let pl = Plate::new();
    let mut a = json!({"method": "position_scale_rotation", "framing": "synthesize_edges"}); if let Some(r) = range { a["synthesis_range"] = json!(r); }
    let (mut se, id) = setup(); a["clip_id"] = json!(id); let r = c(&mut se, "stabilize", a);
    assert_eq!(r["framing"], "synthesize_edges"); assert_eq!(r["zoom"], 1.0); assert!(r["crop"].is_null(), "{r}"); assert_eq!(r["synth_frames"], nf, "{r}");
    assert_eq!(se.project.find_clip(&id).unwrap().props["stab_fill"].at(0.0), nf as f64); for p in ["stab_ss", "stab_sr", "stab_sx", "stab_sy"] { assert_eq!(keys(&se, &id, p).len(), N, "{p}"); }
    let (mut so, id2) = setup(); c(&mut so, "stabilize", json!({"clip_id": id2, "method": "position_scale_rotation", "framing": "stabilize_only"}));
    for p in ["x", "y", "scale", "rotation"] { assert_eq!(keys(&se, &id, p), keys(&so, &id2, p), "synthesize_edges keeps the stabilize_only corrections ({p})"); }
    assert!(so.project.find_clip(&id2).unwrap().props.get("stab_fill").is_none());
    let b = b_from_keys(&se, &id); let mut total_unc = 0;
    for t in [12usize, 24, 36] {
        let (fo, fs) = (render_t(&so, t as f64 / FPS, "only"), render_t(&se, t as f64 / FPS, "synth"));
        let (bi, ti) = (trackmath::inv3(&b[t]).unwrap(), trackmath::inv3(&pl.cam(t as f64)).unwrap());
        let (mut unc, mut filled, mut ef, mut ni, mut ei, mut changed) = (0usize, 0usize, 0f64, 0usize, 0f64, 0usize);
        for y in 0..H { for x in 0..W { let i = (y * W + x) * 4; let q = ap(&bi, (x as f64, y as f64)); let wp = ap(&ti, q); let truth = pl.tex(wp.0, wp.1).clamp(0.0, 255.0);
            if (fo.d[i + 3] as f64) < 0.5 { unc += 1; if fs.d[i + 3] as f64 >= 0.99 { filled += 1; ef += (gray(&fs, i) - truth).abs(); } }
            else if q.0 >= 1.5 && q.1 >= 1.5 && q.0 <= W as f64 - 2.5 && q.1 <= H as f64 - 2.5 { ni += 1; ei += (gray(&fs, i) - truth).abs(); if fo.d[i..i + 4] != fs.d[i..i + 4] { changed += 1; } } } }
        total_unc += unc; let ff = if unc == 0 { 1.0 } else { filled as f64 / unc as f64 }; let mf = if filled == 0 { 0.0 } else { ef / filled as f64 }; let mi = ei / ni.max(1) as f64;
        println!("stabilize_tool synthesize_edges range={} t={t} uncovered={unc} filled_frac={ff:.4} mae_filled={mf:.3} mae_interior={mi:.3} interior_px={ni} interior_changed={changed}", range.unwrap_or(0.3));
        assert!(ni > W * H / 2, "frame {t}: only {ni} interior pixels");
        assert!(ff >= 0.99, "frame {t}: filled {filled} of {unc} uncovered pixels ({ff})");
        assert!(mf <= 1.5 * mi + 0.5, "frame {t}: filled MAE {mf} vs interior MAE {mi}");
        assert_eq!(changed, 0, "frame {t}: the fill changed {changed} interior pixels");
    }
    assert!(total_unc > 0, "stabilize_only left no border on the tested frames: nothing was tested");
    // another framing removes the stale synthesize props
    c(&mut se, "stabilize", json!({"clip_id": id, "framing": "stabilize_only"})); let cl = se.project.find_clip(&id).unwrap();
    for k in ["stab_fill", "stab_ss", "stab_sr", "stab_sx", "stab_sy"] { assert!(cl.props.get(k).is_none(), "{k} left behind"); }
}
#[test] fn synthesize_edges_refusals_leave_the_project_unchanged() {
    let (mut s, id) = setup(); let before = serde_json::to_string(&s.project).unwrap();
    for (a, want) in [(json!({"clip_id": id, "method": "perspective", "framing": "synthesize_edges"}), "synthesize_edges supports position and position_scale_rotation"),
        (json!({"clip_id": id, "framing": "synthesize_edges", "synthesis_range": 0}), "0.03..2"), (json!({"clip_id": id, "framing": "synthesize_edges", "synthesis_range": 5}), "0.03..2"),
        (json!({"clip_id": id, "framing": "synthesize_edges", "smooth": 5}), "classic engine")] {
        let e = dispatch(&mut s, "stabilize", &a).expect_err(&format!("accepted {a}")); assert!(e.contains(want), "{a}: {e}");
        assert_eq!(before, serde_json::to_string(&s.project).unwrap(), "{a}: a refused call changed the project"); }
}
