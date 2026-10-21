//! K5 3D camera tracker gates. The plate is built only from known geometry (no image rendering): 120 points on the ground plane (y = 0, y is
//! down) plus 40 points on four boxes, a camera on a dolly + pan path over 60 frames at 1280x720, focal 1000 px, truth lens k1 = -0.08
//! (second case adds p1 = 0.001, p2 = -0.0008). Truth observations use the literal OpenCV formula written in this file (`cv_distort`), then
//! Gaussian noise sigma 0.2 px per axis (seeded), and 20% of the tracks are replaced by random walks. Each case prints one raw line of numbers.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{apply, Frame, Params};
use kimo_core::ext::camera_solve;
use kimo_core::keyframe::Prop;
use kimo_core::sfm::*;
use kimo_core::three_d::{Camera, Pose as KPose, Projector};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;

const W: f64 = 1280.0;
const H: f64 = 720.0;
const F: f64 = 1000.0;
const NF: usize = 60;
const FPS: f64 = 25.0;

fn gauss(r: &mut Rng) -> f64 { let u = (r.next() % 1_000_000) as f64 / 1e6 + 1e-7; let v = (r.next() % 1_000_000) as f64 / 1e6; (-2.0 * u.ln()).sqrt() * (6.2831853 * v).cos() }
fn uni(r: &mut Rng) -> f64 { (r.next() % 1_000_000) as f64 / 1e6 }
/// Independent literal OpenCV (calib3d docs) Brown-Conrady on normalised coordinates.
fn cv_distort(x: f64, y: f64, k1: f64, k2: f64, p1: f64, p2: f64) -> (f64, f64) {
    let r2 = x * x + y * y; let radial = 1.0 + k1 * r2 + k2 * r2 * r2;
    (x * radial + 2.0 * p1 * x * y + p2 * (r2 + 2.0 * x * x), y * radial + p1 * (r2 + 2.0 * y * y) + 2.0 * p2 * x * y)
}
fn intr(k1: f64, k2: f64, p1: f64, p2: f64) -> Intr { Intr { f: F, cx: W / 2.0, cy: H / 2.0, k1, k2, p1, p2 } }
/// World->camera pose of a camera at centre c: camera-to-world = yaw about y, then tilt about x (positive tilt looks down, y is down), then roll.
fn cam_pose(c: V3, yaw: f64, tilt: f64, roll: f64) -> Pose {
    let c2w = mm(&exp_so3(&[0.0, yaw, 0.0]), &mm(&exp_so3(&[-tilt, 0.0, 0.0]), &exp_so3(&[0.0, 0.0, roll]))); let r = tp(&c2w); let v = mv(&r, &c); Pose { r, t: [-v[0], -v[1], -v[2]] }
}
/// Truth distorted pixel (literal formula) and truth undistorted (pinhole) pixel.
fn truth_px(p: &Pose, x: &V3, k: &Intr) -> Option<((f64, f64), (f64, f64))> {
    let c = p.apply(x); if c[2] < 0.3 { return None; } let (a, b) = (c[0] / c[2], c[1] / c[2]); let d = cv_distort(a, b, k.k1, k.k2, k.p1, k.p2);
    Some(((k.f * d.0 + k.cx, k.f * d.1 + k.cy), (k.f * a + k.cx, k.f * b + k.cy)))
}

struct Plate { tracks: Vec<Track>, clean: Vec<Vec<(usize, f64, f64)>>, inlier: Vec<bool>, ground: Vec<bool>, pts: Vec<V3>, poses: Vec<Pose> }
/// Observations of known points through known cameras: noise sigma 0.2 px, then `outlier_frac` of the tracks replaced by random walks.
fn observe(pts: &[V3], poses: &[Pose], k: &Intr, rng: &mut Rng, outlier_frac: f64) -> (Vec<Track>, Vec<Vec<(usize, f64, f64)>>, Vec<bool>) {
    let mut tracks = vec![]; let mut clean = vec![];
    for x in pts { let mut ob = vec![]; let mut cl = vec![];
        for (i, p) in poses.iter().enumerate() { let Some(((u, v), _)) = truth_px(p, x, k) else { continue }; if !(u >= 0.0 && v >= 0.0 && u <= W && v <= H) { continue; } cl.push((i, u, v)); ob.push((i, u + gauss(rng) * 0.2, v + gauss(rng) * 0.2)); }
        tracks.push(Track { obs: ob }); clean.push(cl); }
    let mut inlier = vec![true; pts.len()]; let nout = (pts.len() as f64 * outlier_frac).round() as usize;
    for j in rng.pick(pts.len(), nout) { inlier[j] = false; let frames: Vec<usize> = tracks[j].obs.iter().map(|o| o.0).collect(); let (mut u, mut v) = (uni(rng) * W, uni(rng) * H);
        let mut ob = vec![]; for i in frames { u = (u + gauss(rng) * 4.0).clamp(0.0, W); v = (v + gauss(rng) * 4.0).clamp(0.0, H); ob.push((i, u, v)); } tracks[j].obs = ob; }
    (tracks, clean, inlier)
}
fn plate(seed: u64, k: &Intr) -> Plate {
    let mut rng = Rng(seed); let mut pts = vec![];
    for _ in 0..120 { let z = 4.0 + 14.0 * uni(&mut rng); let x = (uni(&mut rng) - 0.5) * 2.0 * (0.5 * z + 1.0); pts.push([x, 0.0, z]); }
    for &(bx, bz, a, hb) in &[(-2.5, 7.0, 0.6, 1.0), (2.2, 9.0, 0.7, 1.4), (-0.5, 12.0, 0.8, 0.9), (3.8, 15.0, 1.0, 2.0)] {
        for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] { pts.push([bx + sx * a, -hb, bz + sz * a]); }
        for _ in 0..6 { let face = rng.below(4); let u = uni(&mut rng) * 2.0 - 1.0; let y = -hb * (0.2 + 0.8 * uni(&mut rng));
            let (x, z) = match face { 0 => (bx - a, bz + u * a), 1 => (bx + a, bz + u * a), 2 => (bx + u * a, bz - a), _ => (bx + u * a, bz + a) }; pts.push([x, y, z]); } }
    // dolly 3 units sideways and 1 forward at eye height 1.6 (y = -1.6), panning from +6 to -6 deg to keep the set framed, 8 deg down
    let poses: Vec<Pose> = (0..NF).map(|i| { let s = i as f64 / (NF - 1) as f64; cam_pose([-1.5 + 3.0 * s, -1.6 + 0.05 * (3.0 * s).sin(), s], 0.1047 * (1.0 - 2.0 * s), 0.1396, 0.01 * (2.0 * s).sin()) }).collect();
    let (tracks, clean, inlier) = observe(&pts, &poses, k, &mut rng, 0.2); let ground = (0..pts.len()).map(|j| j < 120).collect();
    Plate { tracks, clean, inlier, ground, pts, poses }
}
fn opts(tangential: bool) -> SolveOpts { SolveOpts { focal_px: Some(F), refine_focal: false, refine_dist: true, tangential, ..Default::default() } }
struct Case { plate: Plate, sol: Solution, truth: Intr }
fn radial() -> &'static Case { static C: OnceLock<Case> = OnceLock::new(); C.get_or_init(|| { let truth = intr(-0.08, 0.0, 0.0, 0.0); let plate = plate(2026, &truth); let sol = solve(&plate.tracks, NF, W, H, &opts(false)).expect("radial solve"); Case { plate, sol, truth } }) }
fn tangential() -> &'static Case { static C: OnceLock<Case> = OnceLock::new(); C.get_or_init(|| { let truth = intr(-0.08, 0.0, 0.001, -0.0008); let plate = plate(2027, &truth); let sol = solve(&plate.tracks, NF, W, H, &opts(true)).expect("tangential solve"); Case { plate, sol, truth } }) }

/// Reprojection RMS (px, sqrt of mean squared 2D error) of the solved model over the inlier tracks that have a solved point, against the
/// noise-free truth observations (clean) or the noisy ones fed to the solver.
fn rms_inliers(sol: &Solution, p: &Plate, clean: bool) -> (f64, usize) {
    let (mut s, mut n) = (0.0, 0usize);
    for (pi, &tj) in sol.point_track.iter().enumerate() { if !p.inlier[tj] { continue; } let obs = if clean { &p.clean[tj] } else { &p.tracks[tj].obs };
        for &(i, u, v) in obs { let Some(pose) = &sol.poses[i] else { continue }; let Some((pu, pv)) = project(pose, &sol.points[pi], &sol.intr) else { continue }; s += (pu - u).powi(2) + (pv - v).powi(2); n += 1; } }
    ((s / n.max(1) as f64).sqrt(), n)
}
/// Umeyama similarity (scale included) aligning a onto b; returns the RMS residual.
fn umeyama_rms(a: &[V3], b: &[V3]) -> f64 {
    let n = a.len() as f64; let ma = [0, 1, 2].map(|k| a.iter().map(|p| p[k]).sum::<f64>() / n); let mb = [0, 1, 2].map(|k| b.iter().map(|p| p[k]).sum::<f64>() / n);
    let mut cov = [[0.0; 3]; 3]; for (p, q) in a.iter().zip(b) { for i in 0..3 { for j in 0..3 { cov[i][j] += (q[i] - mb[i]) * (p[j] - ma[j]) / n; } } }
    let (u, s, v) = svd3(&cov).expect("svd"); let mut d = ID3; if det3(&u) * det3(&v) < 0.0 { d[2][2] = -1.0; } let r = mm(&mm(&u, &d), &tp(&v));
    let va: f64 = a.iter().map(|p| (0..3).map(|k| (p[k] - ma[k]).powi(2)).sum::<f64>()).sum::<f64>() / n; let sc = (s[0] + s[1] + d[2][2] * s[2]) / va;
    let mut e = 0.0; for (p, q) in a.iter().zip(b) { let m = mv(&r, &[p[0] - ma[0], p[1] - ma[1], p[2] - ma[2]]); for k in 0..3 { e += (sc * m[k] + mb[k] - q[k]).powi(2); } } (e / n).sqrt()
}
fn scene_scale(p: &Plate) -> f64 { let lo = [0, 1, 2].map(|k| p.pts.iter().map(|x| x[k]).fold(f64::MAX, f64::min)); let hi = [0, 1, 2].map(|k| p.pts.iter().map(|x| x[k]).fold(f64::MIN, f64::max)); norm(&[hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]) }
fn path_rms(c: &Case) -> f64 { let idx: Vec<usize> = (0..NF).filter(|&i| c.sol.poses[i].is_some()).collect(); let a: Vec<V3> = idx.iter().map(|&i| c.sol.poses[i].unwrap().center()).collect(); let b: Vec<V3> = idx.iter().map(|&i| c.plate.poses[i].center()).collect(); umeyama_rms(&a, &b) }

fn abc(name: &str, c: &Case) {
    let (rc, nc) = rms_inliers(&c.sol, &c.plate, true); let (rn, _) = rms_inliers(&c.sol, &c.plate, false); let pr = path_rms(c); let ss = scene_scale(&c.plate);
    let inl_pts = c.sol.point_track.iter().filter(|&&t| c.plate.inlier[t]).count();
    println!("CASE {name}: registered {}/{NF} solver_rms {:.6} rms_clean {:.6} (obs {nc}) rms_noisy {:.6} f {:.4} k1 {:.6} k2 {:.6} p1 {:.7} p2 {:.7} (truth k1 {} k2 {} p1 {} p2 {}) path_rms {:.6} scene_scale {:.4} path_rel {:.6} points {} inlier_points {inl_pts} outlier_points {}",
        c.sol.registered, c.sol.rms, rc, rn, c.sol.intr.f, c.sol.intr.k1, c.sol.intr.k2, c.sol.intr.p1, c.sol.intr.p2, c.truth.k1, c.truth.k2, c.truth.p1, c.truth.p2, pr, ss, pr / ss, c.sol.points.len(), c.sol.points.len() - inl_pts);
    for n in &c.sol.notes { println!("  {n}"); }
    assert!(nc > 1000, "too few inlier observations evaluated: {nc}");
    assert!(rc <= 0.3, "a) rms vs noise-free truth {rc} > 0.3 px");
    assert!((c.sol.intr.k1 - c.truth.k1).abs() <= 0.005, "b) k1 {} vs {}", c.sol.intr.k1, c.truth.k1);
    if c.truth.has_tan() { assert!((c.sol.intr.p1 - c.truth.p1).abs() <= 0.0005 && (c.sol.intr.p2 - c.truth.p2).abs() <= 0.0005, "b) p1 {} p2 {} vs {} {}", c.sol.intr.p1, c.sol.intr.p2, c.truth.p1, c.truth.p2); }
    assert!(pr <= 0.005 * ss, "c) camera path rms {pr} > 0.5% of scene scale {ss}");
}
#[test] fn abc_radial_plate() { abc("radial", radial()); }
#[test] fn abc_tangential_plate() { abc("tangential", tangential()); }

fn call(ss: &mut Session, n: &str, a: Value) -> Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
/// Project 1280x720 at 25 fps with one solid clip spanning the plate; the solve is stored for that clip at timeline 0.
fn session_with(sol: &Solution, tangential: bool) -> (Session, String) {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 1280, "height": 720, "fps": FPS})); let t = call(&mut ss, "add_track", json!({}))["track_id"].clone();
    let clip = call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.2, 0.2], "start": 0, "duration": NF as f64 / FPS}))["clip_id"].as_str().unwrap().to_string();
    let id = camera_solve::store_solution(&clip, 0, FPS, 0.0, 1280, 720, sol.clone(), sol.point_track.len(), tangential); (ss, id)
}
fn pidx(sol: &Solution, ok: impl Fn(usize) -> bool) -> Vec<usize> { (0..sol.points.len()).filter(|&pi| ok(sol.point_track[pi])).collect() }
/// Three inlier ground points (truth z 5..12) spanning the largest truth triangle.
fn spread_triple(c: &Case) -> [usize; 3] {
    let cand = pidx(&c.sol, |t| c.plate.inlier[t] && c.plate.ground[t] && (5.0..=12.0).contains(&c.plate.pts[t][2])); assert!(cand.len() >= 3, "only {} candidate ground points", cand.len());
    let tr = |pi: usize| c.plate.pts[c.sol.point_track[pi]]; let mut best = (0.0, [cand[0], cand[1], cand[2]]);
    for a in 0..cand.len() { for b in a + 1..cand.len() { for d in b + 1..cand.len() { let (p, q, r) = (tr(cand[a]), tr(cand[b]), tr(cand[d])); let area = ((q[0] - p[0]) * (r[2] - p[2]) - (q[2] - p[2]) * (r[0] - p[0])).abs(); if area > best.0 { best = (area, [cand[a], cand[b], cand[d]]); } } } }
    best.1
}

#[test] fn d_set_ground_plane_levels_the_scene() {
    let c = radial(); let (mut ss, id) = session_with(&c.sol, false); let tri = spread_triple(c); let tr = |pi: usize| c.plate.pts[c.sol.point_track[pi]];
    let (a, b) = (tr(tri[0]), tr(tri[1])); let dist = norm(&[a[0] - b[0], a[1] - b[1], a[2] - b[2]]);
    let r = call(&mut ss, "set_ground_plane", json!({"solve_id": id, "track_ids": tri, "scale_tracks": [tri[0], tri[1]], "scale_distance": dist}));
    let after = camera_solve::solution(&id).unwrap();
    let ground: Vec<V3> = pidx(&after, |t| c.plate.inlier[t] && c.plate.ground[t]).iter().map(|&pi| after.points[pi]).collect(); let (_, n, rms, _) = fit_plane(&ground).unwrap();
    let ang = n[1].abs().min(1.0).acos().to_degrees(); let ss_ = scene_scale(&c.plate); let o = norm(&after.points[tri[0]]);
    let (r0, _) = rms_inliers(&c.sol, &c.plate, false); let (r1, _) = rms_inliers(&after, &c.plate, false);
    let cams: Vec<V3> = after.poses.iter().flatten().map(|p| p.center()).collect(); let cy = cams.iter().map(|p| p[1]).sum::<f64>() / cams.len() as f64;
    println!("CASE ground_plane: points {tri:?} scale_distance {dist:.6} result {r} | all-ground normal {n:?} angle_to_up_deg {ang:.6} ground_rms {rms:.6} origin_dist {o:.3e} (gate {:.4}) rms_before {r0:.9} rms_after {r1:.9} diff {:.3e} mean_camera_y {cy:.4} (truth -1.6)", 0.005 * ss_, (r1 - r0).abs());
    assert!(ang <= 0.5, "d) ground normal {ang} deg from world up");
    assert!(cy < 0.0, "d) cameras must be above the ground (y < 0, y is down): mean y {cy}");
    assert!(o <= 0.005 * ss_, "d) origin track point {o} from the origin");
    assert!((r1 - r0).abs() <= 1e-6, "d) reprojection rms changed by {}", (r1 - r0).abs());
    assert_eq!(r["reapplied"], json!(false));
}

/// Max over every frame of |kimo projection of the layer centre - truth undistorted projection of the truth point| (px), and its frame.
fn worst_on_track(ss: &Session, clip: &str, x: &V3, poses: &[Pose]) -> (f64, usize) {
    let c = ss.project.find_clip(clip).expect("placed clip"); let g = |k: &str, t: f64, d: f64| c.props.get(k).map(|p| p.at(t)).unwrap_or(d); let mut worst = (0.0f64, 0usize);
    for i in 0..NF { let t = i as f64 / FPS; let lt = t - c.start; let cam = Camera::from_props(&ss.project.camera, t);
        let kp = KPose { pos: [g("x", lt, 0.0), g("y", lt, 0.0), g("pos_z", lt, 0.0)], rot: [g("rot_x", lt, 0.0), g("rot_y", lt, 0.0), g("rotation", lt, 0.0)], scale: g("scale", lt, 1.0) };
        let (u, v, _) = Projector::new(1280, 720, &cam, &kp).src_to_screen(640.0, 360.0).expect("layer in front of the camera");
        let (_, (tu, tv)) = truth_px(&poses[i], x, &intr(0.0, 0.0, 0.0, 0.0)).expect("truth in front"); let e = ((u - tu).powi(2) + (v - tv).powi(2)).sqrt(); if e > worst.0 { worst = (e, i); } }
    worst
}
#[test] fn e_place_at_track_renders_on_the_track() {
    // kimo's camera is a pinhole, so the layer is compared with the UNDISTORTED truth projection of the truth point
    let c = radial(); let (mut ss, id) = session_with(&c.sol, false);
    let mut by_obs = pidx(&c.sol, |t| c.plate.inlier[t]); by_obs.sort_by_key(|&pi| std::cmp::Reverse(c.plate.clean[c.sol.point_track[pi]].len()));
    let (pa, pb) = (by_obs[0], by_obs[1]); let xa = c.plate.pts[c.sol.point_track[pa]]; let xb = c.plate.pts[c.sol.point_track[pb]];
    let ap = call(&mut ss, "apply_camera_solve", json!({"solve_id": id}));
    let r = call(&mut ss, "place_at_track", json!({"solve_id": id, "track_id": pa})); let null_id = r["clip_id"].as_str().unwrap().to_string();
    let (w0, f0) = worst_on_track(&ss, &null_id, &xa, &c.plate.poses);
    let tri = spread_triple(c); let g = call(&mut ss, "set_ground_plane", json!({"solve_id": id, "track_ids": tri}));
    let (w1, f1) = worst_on_track(&ss, &null_id, &xa, &c.plate.poses);
    let r2 = call(&mut ss, "place_at_track", json!({"solve_id": id, "track_id": pb, "kind": "solid", "size": 30})); let solid = r2["clip_id"].as_str().unwrap().to_string();
    let (w2, f2) = worst_on_track(&ss, &solid, &xb, &c.plate.poses);
    println!("CASE place_at_track: apply {ap} | null at point {pa} {r} worst {w0:.6}px @frame {f0} | after set_ground_plane {g} worst {w1:.6}px @frame {f1} | solid at point {pb} {r2} worst {w2:.6}px @frame {f2}");
    assert_eq!(g["reapplied"], json!(true)); assert!(g["moved_layers"].as_array().unwrap().iter().any(|v| v.as_str() == Some(null_id.as_str())), "the placed null must follow the re-oriented solve");
    assert!(w0 <= 0.5 && w1 <= 0.5 && w2 <= 0.5, "e) layer off the track: {w0} / {w1} / {w2} px");
}

#[test] fn f_nodal_pan_solves_in_rotation_mode() {
    let k = intr(-0.08, 0.0, 0.0, 0.0); let mut rng = Rng(77); let mut pts = vec![];
    for _ in 0..500 { let d = [(uni(&mut rng) - 0.5) * 2.4, (uni(&mut rng) - 0.5) * 1.0, 1.0]; let l = norm(&d); let z = 8.0 + 32.0 * uni(&mut rng); pts.push([d[0] / l * z, d[1] / l * z, d[2] / l * z]); }
    let poses: Vec<Pose> = (0..NF).map(|i| { let s = i as f64 / (NF - 1) as f64; cam_pose([0.0; 3], 0.2618 * (2.0 * s - 1.0), 0.05 * (6.2831853 * s).sin(), 0.02 * s) }).collect();
    let (tracks, _, _) = observe(&pts, &poses, &k, &mut rng, 0.0);
    let sol = solve_auto(&tracks, NF, W, H, &opts(false), "rotation").expect("rotation solve"); assert_eq!(sol.mode, "rotation_only");
    let r0 = sol.poses[0].unwrap().r; let mut worst = (0.0f64, 0usize); let mut sum = 0.0;
    for i in 0..NF { let rs = mm(&sol.poses[i].expect("every frame registered").r, &tp(&r0)); let rg = mm(&poses[i].r, &tp(&poses[0].r)); let e = log_so3(&mm(&tp(&rg), &rs)); let d = norm(&e).to_degrees(); sum += d; if d > worst.0 { worst = (d, i); } }
    println!("CASE nodal: registered {}/{NF} rms {:.6} f {:.4} k1 {:.6} mean_rot_err_deg {:.6} max_rot_err_deg {:.6} @frame {}", sol.registered, sol.rms, sol.intr.f, sol.intr.k1, sum / NF as f64, worst.0, worst.1);
    assert!(worst.0 <= 0.1, "f) frame {} rotation off by {} deg", worst.1, worst.0);
}

#[test] fn g_point_mapping_matches_opencv_literal() {
    let mut rng = Rng(4242); let mut line = String::new();
    for &(k1, k2, p1, p2) in &[(-0.08, 0.0, 0.0, 0.0), (-0.08, 0.01, 0.001, -0.0008), (0.12, -0.03, -0.002, 0.0015)] {
        let k = intr(k1, k2, p1, p2); let (mut e_fwd, mut e_rt) = (0.0f64, 0.0f64);
        for _ in 0..1000 { let o = (uni(&mut rng) * W, uni(&mut rng) * H); let d = distort_px(o, &k); let l = cv_distort((o.0 - k.cx) / k.f, (o.1 - k.cy) / k.f, k1, k2, p1, p2); let l = (k.f * l.0 + k.cx, k.f * l.1 + k.cy);
            e_fwd = e_fwd.max(((d.0 - l.0).powi(2) + (d.1 - l.1).powi(2)).sqrt()); let b = undistort_px(d, &k); e_rt = e_rt.max(((b.0 - o.0).powi(2) + (b.1 - o.1).powi(2)).sqrt()); }
        line += &format!(" [k1 {k1} k2 {k2} p1 {p1} p2 {p2}: distort_vs_literal_max {e_fwd:.3e} roundtrip_max {e_rt:.3e}]");
        assert!(e_fwd <= 1e-6 && e_rt <= 1e-6, "g) model {k1},{k2},{p1},{p2}: forward {e_fwd}, round trip {e_rt}"); }
    println!("CASE point_mapping:{line}");
}

fn smooth_image(w: usize, h: usize) -> Frame {
    let mut d = vec![0f32; w * h * 4]; let tau = 6.2831853f64;
    for y in 0..h { for x in 0..w { let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5); let i = (y * w + x) * 4;
        for c in 0..3 { let ph = c as f64 * 0.7; d[i + c] = (0.5 + 0.15 * (tau * fx / 97.0 + ph).sin() + 0.12 * (tau * fy / 83.0 + ph).cos() + 0.08 * (tau * (fx + fy) / 131.0).sin()) as f32; } d[i + 3] = 1.0; } }
    Frame { w, h, d }
}
fn lens_params(k1: f64, focal: f64) -> Params { let mut p = Params::new(); p.insert("k1".into(), Prop::new(k1)); p.insert("focal_px".into(), Prop::new(focal)); p }
#[test] fn h_redistort_of_undistort_is_identity_on_smooth_image() {
    let (w, h) = (640usize, 360usize); let src = smooth_image(w, h); let mut f = Frame { w, h, d: src.d.clone() }; let p = lens_params(-0.08, 700.0); let none = BTreeMap::new();
    apply("lens_undistort", &p, &none, &mut f, 0.0, 0).unwrap(); apply("lens_redistort", &p, &none, &mut f, 0.0, 0).unwrap();
    let valid: Vec<bool> = (0..w * h).map(|i| f.d[i * 4 + 3] >= 0.999).collect(); let m = 24usize;
    // erode the valid mask by 24 px (outside the frame counts as invalid): horizontal then vertical windows
    let mut hz = vec![false; w * h]; for y in 0..h { for x in 0..w { hz[y * w + x] = x >= m && x + m < w && (x - m..=x + m).all(|xx| valid[y * w + xx]); } }
    let mut inner = vec![false; w * h]; for y in 0..h { for x in 0..w { inner[y * w + x] = y >= m && y + m < h && (y - m..=y + m).all(|yy| hz[yy * w + x]); } }
    let (mut s, mut n) = (0.0f64, 0usize); for i in 0..w * h { if !inner[i] { continue; } for c in 0..3 { s += ((f.d[i * 4 + c] - src.d[i * 4 + c]) as f64).powi(2); n += 1; } }
    let rms = (s / n.max(1) as f64).sqrt(); let cover = n as f64 / 3.0 / (w * h) as f64;
    println!("CASE lens_roundtrip: {w}x{h} k1 -0.08 focal 700 rms {rms:.3e} (gate {:.3e}) over {} px ({:.1}% of the frame, valid {} px)", 0.5 / 255.0, n / 3, cover * 100.0, valid.iter().filter(|v| **v).count());
    assert!(cover > 0.3, "h) evaluated region too small: {cover}"); assert!(rms <= 0.5 / 255.0, "h) rms {rms}");
}


#[test] fn e2_placed_solid_shows_at_its_size_on_the_track() {
    // gates fixed before the run: visible on every rendered frame; at the first frame its area within 10% of size^2 and its centroid within
    // 0.5 px of the undistorted truth projection of the point (kimo's camera is a pinhole)
    let c = radial(); let (mut ss, id) = session_with(&c.sol, false); call(&mut ss, "apply_camera_solve", json!({"solve_id": id}));
    let mut by_obs = pidx(&c.sol, |t| c.plate.inlier[t]); by_obs.sort_by_key(|&pi| std::cmp::Reverse(c.plate.clean[c.sol.point_track[pi]].len()));
    let pb = by_obs[1]; let xb = c.plate.pts[c.sol.point_track[pb]]; let size = 30.0;
    let r = call(&mut ss, "place_at_track", json!({"solve_id": id, "track_id": pb, "kind": "solid", "size": size}));
    let cache = kimo_core::render::Cache::new(); let mut line = String::new();
    for &i in &[0usize, 20, 40, 59] { let f = kimo_core::render::render_frame(&ss.project, i as f64 / FPS, &cache).expect("render");
        let (mut a, mut sx, mut sy) = (0.0f64, 0.0f64, 0.0f64);
        for y in 0..f.h { for x in 0..f.w { let p = &f.d[(y * f.w + x) * 4..]; let k = ((p[0] - 0.2) / 0.8).clamp(0.0, 1.0) as f64; if k > 0.0 { a += k; sx += k * (x as f64 + 0.5); sy += k * (y as f64 + 0.5); } } }
        let (_, (tu, tv)) = truth_px(&c.plate.poses[i], &xb, &intr(0.0, 0.0, 0.0, 0.0)).unwrap();
        let (cx, cy) = if a > 0.0 { (sx / a, sy / a) } else { (f64::NAN, f64::NAN) }; let e = ((cx - tu).powi(2) + (cy - tv).powi(2)).sqrt();
        line += &format!(" [frame {i}: area {a:.2} px2 centroid err {e:.4} px]");
        assert!(a > 0.0, "e2) placed solid invisible at frame {i} ({r})");
        if i == 0 { assert!((a - size * size).abs() <= 0.1 * size * size, "e2) area {a} vs {} at frame 0", size * size); assert!(e <= 0.5, "e2) centroid {e} px off the track"); } }
    println!("CASE placed_solid_size: {r}{line}");
}

// ---- i) defaults ----
/// tests/sfm_synthetic.rs scene_d, verbatim, so the default-path check runs on the existing synthetic setup.
fn sfm_synthetic_scene(nf: usize, noise: f64, outl: f64, seed: u64, f: f64) -> Vec<Track> {
    let mut rng = Rng(seed); let (w, h) = (1280.0, 720.0); let mut pts = vec![]; let (k1, k2) = (0.0, 0.0);
    for _ in 0..400 { let u = |r: &mut Rng| (r.next() % 100000) as f64 / 100000.0; pts.push([(u(&mut rng) - 0.5) * 10.0, (u(&mut rng) - 0.5) * 6.0, 6.0 + u(&mut rng) * 10.0]); }
    let mut poses = vec![]; for i in 0..nf { let s = i as f64 / (nf - 1) as f64; let c = [-1.5 + 3.0 * s, 0.3 * (s * 6.0).sin(), -0.8 * s * s]; let r = exp_so3(&[0.02 * (s * 4.0).sin(), -0.25 * (s - 0.5), 0.03 * s]);
        let t = { let v = mv(&r, &c); [-v[0], -v[1], -v[2]] }; poses.push(Pose { r, t }); }
    let mut tracks = vec![]; for (j, x) in pts.iter().enumerate() { let mut ob = vec![]; for (i, p) in poses.iter().enumerate() { let c = p.apply(x); if c[2] < 0.5 { continue; } let (xa, xb) = (c[0] / c[2], c[1] / c[2]); let r2 = xa * xa + xb * xb; let sc = 1.0 + k1 * r2 + k2 * r2 * r2; let (u, v) = (f * xa * sc + w / 2.0, f * xb * sc + h / 2.0);
        if u < 0.0 || v < 0.0 || u > w || v > h { continue; } if (j * 7 + i * 3) % 11 == 0 { continue; } let bad = (rng.next() % 1000) as f64 / 1000.0 < outl; let (nx, ny) = if bad { (gauss(&mut rng) * 30.0, gauss(&mut rng) * 30.0) } else { (gauss(&mut rng) * noise, gauss(&mut rng) * noise) }; ob.push((i, u + nx, v + ny)); }
        if ob.len() >= 4 { tracks.push(Track { obs: ob }); } }
    tracks
}
fn same_solution(a: &Solution, b: &Solution) -> Result<(), String> {
    if a.mode != b.mode || a.registered != b.registered || a.point_track != b.point_track || a.notes != b.notes { return Err("mode/registered/point_track/notes differ".into()); }
    let s = |x: &Solution| vec![x.intr.f, x.intr.cx, x.intr.cy, x.intr.k1, x.intr.k2, x.intr.p1, x.intr.p2, x.rms, x.k1k2[0], x.k1k2[1]];
    if s(a).iter().zip(s(b)).any(|(p, q)| p.to_bits() != q.to_bits()) { return Err("intrinsics / rms differ".into()); }
    for (p, q) in a.poses.iter().zip(&b.poses) { match (p, q) { (None, None) => {} (Some(p), Some(q)) => { if p.r.iter().flatten().chain(&p.t).zip(q.r.iter().flatten().chain(&q.t)).any(|(x, y)| x.to_bits() != y.to_bits()) { return Err("a pose differs".into()); } } _ => return Err("registration differs".into()) } }
    if a.points.iter().flatten().zip(b.points.iter().flatten()).any(|(x, y)| x.to_bits() != y.to_bits()) || a.points.len() != b.points.len() { return Err("points differ".into()); }
    if a.frame_rms.iter().zip(&b.frame_rms).any(|(x, y)| x.to_bits() != y.to_bits()) { return Err("frame rms differs".into()); }
    Ok(())
}
/// The `lens_distortion` arm of effects::apply as it was before this change, verbatim, so the old effect is pinned bit for bit.
fn legacy_lens_distortion(f: &mut Frame, a: f32) {
    let src = f.d.clone(); let (w, h) = (f.w as f32, f.h as f32);
    for y in 0..f.h { for x in 0..f.w { let (nx, ny) = ((x as f32 / w - 0.5) * 2.0, (y as f32 / h - 0.5) * 2.0); let r2 = nx*nx + ny*ny; let k = 1.0 + a * r2; let (sx, sy) = (((nx * k) * 0.5 + 0.5) * w, ((ny * k) * 0.5 + 0.5) * h);
        let o = (y*f.w+x)*4; if sx < 0.0 || sy < 0.0 || sx >= w || sy >= h { f.d[o..o+4].copy_from_slice(&[0.0, 0.0, 0.0, 1.0]); continue; } let s = (sy as usize * f.w + sx as usize) * 4; for j in 0..4 { f.d[o+j] = src[s+j]; } } }
}
#[test] fn i_defaults_are_unchanged() {
    let f = 1100.0; let tracks = sfm_synthetic_scene(60, 0.4, 0.03, 42, f);
    let a = solve(&tracks, 60, 1280.0, 720.0, &SolveOpts { focal_px: Some(f), ..Default::default() }).expect("solve");
    let b = solve(&tracks, 60, 1280.0, 720.0, &SolveOpts { focal_px: Some(f), tangential: false, ..Default::default() }).expect("solve");
    let same = same_solution(&a, &b); println!("CASE defaults: tangential absent vs false: {same:?}; rms {:.9} f {:.6} p1 {} p2 {} registered {}", a.rms, a.intr.f, a.intr.p1, a.intr.p2, a.registered);
    assert!(same.is_ok(), "i) {same:?}"); assert!(a.intr.p1.to_bits() == 0 && a.intr.p2.to_bits() == 0 && !a.notes.iter().any(|n| n.contains("tangential")), "i) radial-only solve touched p1/p2");
    // old lens_distortion: same pixels as the pre-change code
    for amount in [0.3f64, -0.2] { let src = smooth_image(97, 61); let mut got = Frame { w: src.w, h: src.h, d: src.d.clone() }; let mut p = Params::new(); p.insert("amount".into(), Prop::new(amount));
        apply("lens_distortion", &p, &BTreeMap::new(), &mut got, 0.0, 0).unwrap(); let mut want = Frame { w: src.w, h: src.h, d: src.d.clone() }; legacy_lens_distortion(&mut want, amount as f32);
        assert!(got.d.iter().zip(&want.d).all(|(x, y)| x.to_bits() == y.to_bits()), "i) lens_distortion amount {amount} changed"); }
    // lens_undistort / lens_redistort with all-zero coefficients are the identity (alpha varies, never 0)
    let (w, h) = (83usize, 47usize); let mut src = smooth_image(w, h); for i in 0..w * h { src.d[i * 4 + 3] = 0.2 + 0.8 * ((i * 37 % 101) as f32 / 100.0); }
    for kind in ["lens_undistort", "lens_redistort"] { let mut f = Frame { w, h, d: src.d.clone() }; apply(kind, &lens_params(0.0, 700.0), &BTreeMap::new(), &mut f, 0.0, 0).unwrap();
        let e = f.d.iter().zip(&src.d).map(|(x, y)| (x - y).abs()).fold(0f32, f32::max); println!("CASE zero_lens {kind}: max abs diff {e:.3e}"); assert!(e <= 1e-6, "i) {kind} with zero coefficients is not the identity: {e}"); }
}

fn line_solution() -> Solution {
    let poses = vec![Some(Pose { r: ID3, t: [0.0, 1.0, 5.0] }), Some(Pose { r: ID3, t: [0.5, 1.0, 5.0] })]; let points: Vec<V3> = (0..6).map(|i| [i as f64, 0.0, 2.0 * i as f64]).collect();
    Solution { mode: "full", k1k2: [0.0; 2], poses, intr: intr(0.0, 0.0, 0.0, 0.0), points, point_track: (0..6).collect(), rms: 0.1, frame_rms: vec![0.1; 2], registered: 2, notes: vec![] }
}
#[test] fn refusals_leave_project_and_solve_unchanged() {
    let c = radial(); let (mut ss, id) = session_with(&c.sol, false); let snap = |ss: &Session| serde_json::to_string(&ss.project).unwrap(); let before = snap(&ss);
    let pts = |id: &str| camera_solve::solution(id).unwrap().points; let p0 = pts(id.as_str()); let tri = spread_triple(c);
    let mut errs = vec![];
    let mut expect = |ss: &mut Session, tool: &str, a: Value, needle: &str| { let e = dispatch(ss, tool, &a).expect_err(&format!("{tool} {a} must be refused")); assert!(e.contains(needle), "{tool}: error '{e}' should mention '{needle}'"); errs.push(format!("{tool}: {e}")); };
    expect(&mut ss, "set_ground_plane", json!({"solve_id": id, "track_ids": [tri[0], tri[1]]}), "at least 3");
    expect(&mut ss, "set_ground_plane", json!({"solve_id": "solve_does_not_exist", "track_ids": tri}), "no solve");
    expect(&mut ss, "set_ground_plane", json!({"solve_id": id, "track_ids": [tri[0], tri[1], 999999]}), "no point 999999");
    expect(&mut ss, "set_ground_plane", json!({"solve_id": id, "track_ids": tri, "up": "z"}), "up must be y");
    let lid = camera_solve::store_solution("none", 0, FPS, 0.0, 1280, 720, line_solution(), 6, false);
    expect(&mut ss, "set_ground_plane", json!({"solve_id": lid, "track_ids": [0, 2, 5]}), "collinear");
    expect(&mut ss, "place_at_track", json!({"solve_id": id, "track_id": 999999}), "no point 999999");
    expect(&mut ss, "place_at_track", json!({"solve_id": id, "track_id": 0, "kind": "cube"}), "kind must be");
    let path = std::env::temp_dir().join(format!("kimo_k5_refused_{}.png", std::process::id())); let _ = std::fs::remove_file(&path);
    expect(&mut ss, "export_st_map", json!({"k1": -0.08, "focal_px": 700, "width": 64, "height": 36, "direction": "sideways", "path": path.to_str().unwrap()}), "direction must be undistort or redistort");
    for e in &errs { println!("REFUSED {e}"); }
    assert!(!path.exists(), "a refused export must not write a file"); assert_eq!(snap(&ss), before, "a refused call changed the project");
    assert!(pts(id.as_str()).iter().flatten().zip(p0.iter().flatten()).all(|(a, b)| a.to_bits() == b.to_bits()), "a refused call changed the stored solve");
    assert_eq!(pts(lid.as_str()), line_solution().points);
}

#[test] fn st_map_export_matches_the_lens_mapping() {
    let (w, h) = (64usize, 36usize); let k = Intr { f: 700.0, cx: 0.5 * w as f64, cy: 0.5 * h as f64, k1: -0.08, k2: 0.0, p1: 0.0005, p2: 0.0 };
    let path = std::env::temp_dir().join(format!("kimo_k5_stmap_{}.png", std::process::id())); let mut ss = Session::new();
    let r = call(&mut ss, "export_st_map", json!({"k1": k.k1, "p1": k.p1, "focal_px": k.f, "width": w, "height": h, "direction": "undistort", "path": path.to_str().unwrap()}));
    let png = std::fs::read(&path).unwrap(); let _ = std::fs::remove_file(&path);
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n"); assert_eq!(&png[12..16], b"IHDR"); assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()) as usize, w); assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()) as usize, h); assert_eq!(png[24], 16u8); assert_eq!(png[25], 2u8);
    // our writer emits one IDAT of zlib stored blocks: unwrap them
    let n = u32::from_be_bytes(png[33..37].try_into().unwrap()) as usize; assert_eq!(&png[37..41], b"IDAT"); let z = &png[41..41 + n]; let mut off = 2; let mut raw = vec![];
    loop { let fin = z[off] & 1; let len = u16::from_le_bytes([z[off + 1], z[off + 2]]) as usize; off += 5; raw.extend_from_slice(&z[off..off + len]); off += len; if fin == 1 { break; } }
    assert_eq!(raw.len(), h * (1 + w * 6)); let mut worst = 0.0f64;
    for (x, y) in [(0usize, 0usize), (w - 1, 0), (w / 2, h / 2), (w - 1, h - 1), (7, 30)] { let row = &raw[y * (1 + w * 6)..]; let px = &row[1 + x * 6..]; let s = u16::from_be_bytes([px[0], px[1]]) as f64 / 65535.0; let t = u16::from_be_bytes([px[2], px[3]]) as f64 / 65535.0;
        let p = distort_px((x as f64 + 0.5, y as f64 + 0.5), &k); let (es, et) = (p.0 / w as f64, 1.0 - p.1 / h as f64); worst = worst.max((s - es).abs().max((t - et).abs())); }
    println!("CASE st_map: {r} worst |s,t - mapping| {worst:.3e}"); assert!(worst <= 1.0 / 65535.0, "ST map value off by {worst}");
}
