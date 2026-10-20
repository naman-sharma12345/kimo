//! K4: Source::Particles3D, the 3D particle layer seen through the project camera. The gates are fixed. Each case prints one raw line.
//! Conventions under test: world units are project pixels on the focus plane (origin at the frame centre, +y down, +z away from the camera).
//! The project camera maps to camera_project_points as eye = cam_pos - [0,0,f], f = (h/2)/tan(fov/2), pixel focal f, near 1.
//! Pixel X covers [X, X+1) and is sampled at its centre X + 0.5.
use kimo_core::commands::{dispatch, Session};
use kimo_core::particles3d::{live_particles, position_at, project_point};
use kimo_core::render;
use serde_json::{json, Value};

fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn track(ss: &mut Session) -> String { c(ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string() }
fn session(w: u32, h: u32) -> Session { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": w, "height": h, "fps": 25})); ss }
fn p3d(ss: &mut Session, args: Value) -> String { let t = track(ss); let mut a = json!({"track_id": t, "start": 0.0, "duration": 5.0}); for (k, v) in args.as_object().unwrap() { a[k] = v.clone(); } c(ss, "add_particles3d", a)["clip_id"].as_str().unwrap().to_string() }
fn bits(f: &kimo_core::effects::Frame) -> Vec<u32> { f.d.iter().map(|v| v.to_bits()).collect() }
fn fnv(f: &kimo_core::effects::Frame) -> u64 { let mut h = 0xcbf29ce484222325u64; for v in &f.d { for b in v.to_le_bytes() { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); } } h }
/// The spec formula, written out independently of particles3d::position_at.
fn oracle(x0: [f64; 3], v0: [f64; 3], g: [f64; 3], k: f64, tau: f64) -> [f64; 3] {
    let mut o = [0.0; 3]; for j in 0..3 { o[j] = if k == 0.0 { x0[j] + v0[j] * tau + g[j] * tau * tau / 2.0 } else { x0[j] + (g[j] / k) * tau + (v0[j] - g[j] / k) * (1.0 - (-k * tau).exp()) / k }; } o
}

#[test] fn a_closed_form_trajectory() {
    let mut ss = session(320, 180);
    let mut ids = vec![];
    for (drag, grav) in [(0.7, [0.0, 980.0, 0.0]), (0.0, [0.0, 980.0, 0.0])] {
        ids.push((p3d(&mut ss, json!({"emitter": {"kind": "sphere", "radius": 40, "position": [10, -20, 30], "rate": 100, "lifetime": 0.25, "speed": 300, "speed_var": 0.5, "direction": [0.3, -1, 0.2], "spread_deg": 60, "seed": 7}, "physics": {"gravity": grav, "drag": drag}})), drag, grav));
    }
    for t in [0.3, 1.1, 2.7] { for (id, drag, grav) in &ids {
        let live = live_particles(&ss.project, id, t).unwrap(); assert!(live.len() >= 20, "need 20 live particles, got {}", live.len());
        let mut err: f64 = 0.0;
        for l in live.iter().take(20) {
            assert_eq!(l.birth.to_bits(), (0.0 + l.index as f64 / 100.0).to_bits(), "birth = start + i/rate exactly"); assert_eq!(l.age.to_bits(), (t - l.birth).to_bits(), "age = t - birth");
            let want = oracle(l.x0, l.v0, *grav, *drag, t - l.birth); for j in 0..3 { err = err.max((l.position[j] - want[j]).abs()); }
            let pa = position_at(l.x0, l.v0, *grav, *drag, l.age); for j in 0..3 { err = err.max((pa[j] - want[j]).abs()); }
        }
        println!("a_closed_form t={t} drag={drag} live={} checked=20 max_err={err:.3e}", live.len());
        assert!(err <= 1e-6, "closed-form error {err} > 1e-6 at t={t} drag={drag}");
    } }
}

#[test] fn b_sub_frame_birth_spacing() {
    let mut ss = session(320, 180);
    let id = p3d(&mut ss, json!({"emitter": {"kind": "point", "position_keys": [{"t": 0.0, "position": [0, 0, 0]}, {"t": 1.0, "position": [1000, 0, 0]}], "rate": 1000, "lifetime": 5, "speed": 0, "spread_deg": 0}, "physics": {"gravity": [0, 0, 0], "drag": 0}}));
    for t in [0.5, 0.9] {
        let live = live_particles(&ss.project, &id, t).unwrap(); let mut xs: Vec<f64> = live.iter().map(|l| l.position[0]).collect(); xs.sort_by(|a, b| a.total_cmp(b));
        let d: Vec<f64> = xs.windows(2).map(|w| w[1] - w[0]).collect(); let (lo, hi) = d.iter().fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(*x), b.max(*x)));
        println!("b_spacing t={t} particles={} min_gap={lo:.9} max_gap={hi:.9}", xs.len());
        assert!(xs.len() > 400, "expected hundreds of particles, got {}", xs.len());
        assert!(d.iter().all(|g| (g - 1.0).abs() <= 0.01), "neighbour spacing out of 1 +- 0.01: min {lo} max {hi}");
    }
}

#[test] fn c_reprojection_through_the_project_camera() {
    let (w, h) = (320u32, 180u32); let mut ss = session(w, h);
    let cam = json!({"cam_x": 20.0, "cam_y": -10.0, "cam_z": 30.0, "cam_pan": 8.0, "cam_tilt": -5.0, "cam_roll": 3.0, "cam_fov": 55.0}); c(&mut ss, "set_camera", cam.clone());
    let id = p3d(&mut ss, json!({"emitter": {"kind": "point", "position_keys": [{"t": 0.0, "position": [-80, -30, 40]}, {"t": 0.5, "position": [80, 30, -40]}], "rate": 10, "lifetime": 10, "speed": 20, "direction": [0, 1, 0], "spread_deg": 0},
        "physics": {"gravity": [0, 50, 0], "drag": 0.5}, "sprite": {"kind": "disc", "size": 10, "hardness": 5, "color": [1, 1, 1]}, "blend": "additive"}));
    let t = 0.45; let live = live_particles(&ss.project, &id, t).unwrap(); assert_eq!(live.len(), 5, "5 isolated particles");
    // independent camera: the documented mapping with a different sensor (24 mm) and the same pixel focal
    let g = |k: &str| cam[k].as_f64().unwrap(); let f = (h as f64 / 2.0) / (g("cam_fov").to_radians() / 2.0).tan();
    let camj = json!({"eye": [g("cam_x"), g("cam_y"), g("cam_z") - f], "pan": g("cam_pan"), "tilt": g("cam_tilt"), "roll": g("cam_roll"), "sensor_height": 24.0, "focal_length": 24.0 * f / h as f64, "near": 1.0});
    let pts: Vec<[f64; 3]> = live.iter().map(|l| l.position).collect();
    let pr = c(&mut ss, "camera_project_points", json!({"camera": camj, "points": pts, "width": w, "height": h}))["projected"].clone();
    let tcam = kimo_core::three_d::Camera::from_props(&ss.project.camera, t); let cv = kimo_core::three_d::CamView::new(w as usize, h as usize, &tcam);
    let fr = render::render_frame(&ss.project, t, &render::Cache::new()).unwrap(); let (wu, hu) = (w as usize, h as usize);
    let (mut worst_c, mut worst_p): (f64, f64) = (0.0, 0.0);
    for (i, l) in live.iter().enumerate() {
        let q: Vec<f64> = pr[i].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let mine = project_point(&ss.project, t, l.position).unwrap(); let cvp = cv.project(l.position).unwrap();
        let dp = (mine[0] - q[0]).abs().max((mine[1] - q[1]).abs()).max((cvp.0 - q[0]).abs()).max((cvp.1 - q[1]).abs()); worst_p = worst_p.max(dp);
        let rad = 0.5 * 10.0 * f / q[2] + 2.0; let (mut s, mut sx, mut sy) = (0.0, 0.0, 0.0);
        let (x0, x1, y0, y1) = ((q[0] - rad).floor().max(0.0) as usize, ((q[0] + rad).ceil() as usize).min(wu - 1), (q[1] - rad).floor().max(0.0) as usize, ((q[1] + rad).ceil() as usize).min(hu - 1));
        for y in y0..=y1 { for x in x0..=x1 { let v = fr.d[(y * wu + x) * 4] as f64; s += v; sx += v * (x as f64 + 0.5); sy += v * (y as f64 + 0.5); } }
        assert!(s > 0.5, "particle {i} not drawn"); let dc = (sx / s - q[0]).hypot(sy / s - q[1]); worst_c = worst_c.max(dc);
        println!("c_reprojection particle={} world=[{:.3},{:.3},{:.3}] cam_project_points=[{:.4},{:.4}] centroid=[{:.4},{:.4}] err_px={dc:.5} helper_err_px={dp:.3e}", l.index, l.position[0], l.position[1], l.position[2], q[0], q[1], sx / s, sy / s);
    }
    assert!(worst_c <= 0.05, "rendered centroid off by {worst_c} px"); assert!(worst_p <= 1e-6, "projection helper off by {worst_p} px");
}

#[test] fn d_scrub_identity() {
    let mut ss = session(160, 90); c(&mut ss, "set_camera", json!({"cam_pan": 6.0, "cam_z": 20.0}));
    p3d(&mut ss, json!({"emitter": {"kind": "box", "size": [120, 40, 80], "rate": 300, "lifetime": 1.2, "speed": 90, "spread_deg": 40, "seed": 3}, "physics": {"gravity": [0, 200, 0], "drag": 0.8}, "sprite": {"size": 4, "size_end": 1, "color": [1, 0.6, 0.2], "color_end": [0.2, 0.4, 1], "opacity_curve": [[0, 0], [0.1, 1], [1, 0]]}, "blend": "normal", "motion_blur": {"samples": 4, "shutter_angle": 180}}));
    let direct = render::render_frame(&ss.project, 1.7, &render::Cache::new()).unwrap();
    let cache = render::Cache::new(); for k in 0..=15 { render::render_frame(&ss.project, 0.5 + k as f64 * 0.1, &cache).unwrap(); } let fwd = render::render_frame(&ss.project, 1.7, &cache).unwrap();
    let cache = render::Cache::new(); for k in 0..=15 { render::render_frame(&ss.project, 2.0 - k as f64 * 0.1, &cache).unwrap(); } let bwd = render::render_frame(&ss.project, 1.7, &cache).unwrap();
    let lit = direct.d.chunks(4).filter(|p| p[0] > 0.0).count();
    println!("d_scrub direct={:016x} forward={:016x} backward={:016x} lit_pixels={lit}", fnv(&direct), fnv(&fwd), fnv(&bwd));
    assert!(lit > 0, "the scrub scene draws particles"); assert!(bits(&direct) == bits(&fwd) && bits(&direct) == bits(&bwd), "scrub order changed the frame");
}

#[test] fn e_checker_sprite_identity_at_scale_one() {
    // scale 1: sprite size == clip width (160), particle at world origin = the default camera's focus plane (projection scale f/z = 1),
    // so the sprite's frame centre (80, 45) lands on the projected point (80, 45) and every pixel centre maps onto itself
    let mut ss = session(160, 90); let t1 = track(&mut ss);
    c(&mut ss, "add_clip", json!({"track_id": t1, "source_type": "solid", "color": [0.2, 0.3, 0.5], "start": 0, "duration": 5}));
    let chk = ss.project.tracks.iter().find(|t| t.id == t1).unwrap().clips[0].id.clone();
    let e = c(&mut ss, "add_effect", json!({"clip_id": chk, "kind": "checkerboard"}))["effect_id"].as_str().unwrap().to_string();
    for (k, v) in [("size", 7.0), ("opacity", 1.0), ("r", 0.9), ("g", 0.7), ("b", 0.1)] { c(&mut ss, "set_effect_param", json!({"clip_id": chk, "effect_id": e, "param": k, "value": v})); }
    p3d(&mut ss, json!({"emitter": {"kind": "point", "position": [0, 0, 0], "rate": 1, "lifetime": 0.5, "speed": 0, "spread_deg": 0}, "sprite": {"kind": "layer", "clip_id": chk, "texture_time": "current", "size": 160, "opacity_curve": [[0, 1], [1, 1]], "color": [1, 1, 1]}, "blend": "normal"}));
    let cache = render::Cache::new(); let t = 0.25;
    let got = render::render_frame(&ss.project, t, &cache).unwrap(); let want = render::render_clip_alone(&ss.project, &chk, t, &cache).unwrap();
    let mut md: f32 = 0.0; let mut distinct = std::collections::BTreeSet::new();
    for i in 0..160 * 90 { for j in 0..3 { md = md.max((got.d[i * 4 + j] - want.d[i * 4 + j]).abs()); } distinct.insert((want.d[i * 4] * 1000.0) as i32); }
    let gone = render::render_frame(&ss.project, 0.75, &cache).unwrap(); let hidden_max = gone.d.chunks(4).map(|p| p[0].max(p[1]).max(p[2])).fold(0f32, f32::max);
    println!("e_checker footprint=160x90 max_channel_diff={md:.3e} (gate 1/255={:.3e}) distinct_levels={} hidden_source_max_when_no_particle={hidden_max}", 1.0 / 255.0, distinct.len());
    assert!(distinct.len() >= 2, "checker clip is not a checker"); assert!(md <= 1.0 / 255.0, "sprite at scale 1 differs from the clip by {md}"); assert_eq!(hidden_max, 0.0, "sprite source drawn on its own");
}

#[test] fn f_refusals_leave_the_project_unchanged() {
    let mut ss = session(160, 90); let tr = track(&mut ss);
    let id = p3d(&mut ss, json!({}));
    let base = json!({"track_id": tr, "start": 0, "duration": 2});
    let with = |k: &str, v: Value| { let mut a = base.clone(); a[k] = v; a };
    let cases: Vec<(&str, &str, Value, &str)> = vec![
        ("bad_kind", "add_particles3d", with("emitter", json!({"kind": "cone"})), "kind"),
        ("rate_zero", "add_particles3d", with("emitter", json!({"rate": 0})), "rate"),
        ("rate_negative", "add_particles3d", with("emitter", json!({"rate": -5})), "rate"),
        ("rate_too_high", "add_particles3d", with("emitter", json!({"rate": 2e6})), "rate"),
        ("lifetime_zero", "add_particles3d", with("emitter", json!({"lifetime": 0})), "lifetime"),
        ("unknown_sprite_clip", "add_particles3d", with("sprite", json!({"kind": "layer", "clip_id": "clip_nope"})), "clip_id"),
        ("drag_negative", "add_particles3d", with("physics", json!({"drag": -0.1})), "drag"),
        ("set_drag_negative", "set_particles3d", json!({"clip_id": id, "physics": {"drag": -1}}), "drag"),
        ("set_bad_kind", "set_particles3d", json!({"clip_id": id, "emitter": {"kind": "torus"}}), "kind"),
        ("set_rate_zero", "set_particles3d", json!({"clip_id": id, "emitter": {"rate": 0}}), "rate")];
    for (name, tool, args, needle) in cases {
        let before = serde_json::to_string(&ss.project).unwrap(); let r = dispatch(&mut ss, tool, &args); let after = serde_json::to_string(&ss.project).unwrap();
        let msg = r.as_ref().err().cloned().unwrap_or_default(); println!("f_refusal case={name} refused={} unchanged={} error={msg:?}", r.is_err(), before == after);
        assert!(r.is_err(), "{name}: accepted"); assert!(msg.contains(needle), "{name}: error does not name {needle}: {msg}"); assert_eq!(before, after, "{name}: project changed");
    }
    let ok = dispatch(&mut ss, "set_particles3d", &json!({"clip_id": id, "physics": {"drag": 1.5}})); println!("f_refusal case=valid_set ok={}", ok.is_ok()); assert!(ok.is_ok());
}

#[test] fn g_2d_emitter_and_existing_sources_unchanged() {
    fn scene(ss: &mut Session) { let t = track(ss); c(ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.05, 0.05, 0.1], "start": 0, "duration": 2}));
        let t2 = track(ss); c(ss, "add_particles", json!({"track_id": t2, "kind": "sparks", "start": 0, "duration": 2, "count": 150, "seed": 5}));
        let t3 = track(ss); c(ss, "add_emitter", json!({"track_id": t3, "start": 0, "duration": 2, "preset": "sparks", "config": {"shutter_deg": 180}}));
        let t4 = track(ss); c(ss, "add_shape", json!({"track_id": t4, "shape": "ellipse", "width": 60, "height": 40, "fill": [0.2, 0.8, 0.4, 0.7], "start": 0, "duration": 2})); }
    let p3 = json!({"emitter": {"kind": "sphere", "rate": 200}, "physics": {"gravity": [0, 300, 0], "drag": 0.4}});
    let frames = |ss: &Session| -> Vec<u64> { [0.3, 0.7, 1.3].iter().map(|t| fnv(&render::render_frame(&ss.project, *t, &render::Cache::new()).unwrap())).collect() };
    let mut a = session(320, 180); scene(&mut a); let ha = frames(&a);
    let mut b = session(320, 180); scene(&mut b); let pid = p3d(&mut b, p3.clone()); let mid = frames(&b); c(&mut b, "delete_clip", json!({"clip_id": pid})); let hb = frames(&b);
    let mut d = session(320, 180); let pid = p3d(&mut d, p3.clone()); c(&mut d, "delete_clip", json!({"clip_id": pid})); scene(&mut d); let hd = frames(&d);
    let mut e = session(320, 180); scene(&mut e); let t = track(&mut e); let mut q = p3.clone(); q["track_id"] = json!(t); q["start"] = json!(5.0); q["duration"] = json!(1.0); c(&mut e, "add_particles3d", q); let he = frames(&e);
    let old: kimo_core::model::Project = serde_json::from_str(&serde_json::to_string(&a.project).unwrap()).unwrap(); let hr: Vec<u64> = [0.3, 0.7, 1.3].iter().map(|t| fnv(&render::render_frame(&old, *t, &render::Cache::new()).unwrap())).collect();
    println!("g_2d_identity base={ha:016x?} after_add_remove={hb:016x?} add_remove_first={hd:016x?} p3d_outside_range={he:016x?} json_roundtrip={hr:016x?} with_p3d_live={mid:016x?}");
    assert_eq!(ha, hb); assert_eq!(ha, hd); assert_eq!(ha, he); assert_eq!(ha, hr); assert_ne!(ha, mid, "the live 3D layer does draw");
}
