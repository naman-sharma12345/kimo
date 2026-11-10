//! K3: 3D compositor (src/comp3d.rs, tool set_3d_compositor). Gates are fixed in the code before any run; every test prints its raw numbers.
//! Camera maths used for the predictions (three_d.rs): default camera, f = (H/2)/tan(fov/2), eye at (0, 0, -f), pixel (x, y) looks along
//! d = (x - W/2, y - H/2, f); a plane through p0 with normal n is hit at t = ((p0 - c).n)/(d.n), camera depth z = t f, and for a plane 1/z is affine
//! in screen space. A fronto-parallel plane at pos_z = Z maps world (X, Y) to screen (W/2 + X f/(Z+f), H/2 + Y f/(Z+f)).
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 320;
const H: usize = 240;
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn sid(v: &Value) -> String { v["clip_id"].as_str().unwrap().to_string() }
fn f_px() -> f64 { (H as f64 / 2.0) / (50f64.to_radians() / 2.0).tan() }
fn session() -> Session { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": W, "height": H, "fps": 25})); ss }
/// A rect shape on its own new (top) track, with base props.
fn layer(ss: &mut Session, w: f64, h: f64, fill: [f64; 3], props: &[(&str, f64)]) -> String {
    let t = c(ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = sid(&c(ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": w, "height": h, "fill": fill, "start": 0.0, "duration": 5.0})));
    for (k, v) in props { c(ss, "set_property", json!({"clip_id": id, "property": k, "value": v})); }
    id
}
fn frame(ss: &Session) -> Frame { render::render_frame(&ss.project, 1.0, &render::Cache::new()).unwrap() }
fn bytes(ss: &Session) -> Vec<u8> { render::to_rgba8(&frame(ss)) }
fn bits(f: &Frame) -> Vec<u32> { f.d.iter().map(|v| v.to_bits()).collect() }
fn px(b: &[u8], x: usize, y: usize) -> [u8; 4] { let i = (y * W + x) * 4; [b[i], b[i + 1], b[i + 2], b[i + 3]] }
fn on(ss: &mut Session) { c(ss, "set_3d_compositor", json!({"enabled": true})); }
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

// a) depth sort: the farther plane is LATER in the stack. comp3d on: the nearer one wins every overlap pixel; off: stack order wins.
#[test] fn depth_sort_nearer_plane_wins_only_when_enabled() {
    let build = |enable: bool| { let mut ss = session();
        layer(&mut ss, 120.0, 120.0, [1.0, 0.0, 0.0], &[("pos_z", 50.0), ("x", -30.0)]);   // near, red, bottom track
        layer(&mut ss, 120.0, 120.0, [0.0, 1.0, 0.0], &[("pos_z", 300.0), ("x", 30.0)]);   // far, green, top track
        if enable { on(&mut ss); } bytes(&ss) };
    let (b_on, b_off) = (build(true), build(false)); let f = f_px();
    // analytic screen boxes: centre W/2 + X k, half size 60 k, k = f/(f+Z)
    let bx = |x0: f64, z: f64| { let k = f / (f + z); (W as f64 / 2.0 + (x0 - 60.0) * k, W as f64 / 2.0 + (x0 + 60.0) * k, H as f64 / 2.0 - 60.0 * k, H as f64 / 2.0 + 60.0 * k) };
    let (a, b) = (bx(-30.0, 50.0), bx(30.0, 300.0)); let m = 1.5;
    let (mut n, mut bad_on, mut bad_off) = (0, 0, 0);
    for y in 0..H { for x in 0..W { let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
        let inside = |r: (f64, f64, f64, f64)| cx > r.0 + m && cx < r.1 - m && cy > r.2 + m && cy < r.3 - m;
        if inside(a) && inside(b) { n += 1; if px(&b_on, x, y) != RED { bad_on += 1; } if px(&b_off, x, y) != GREEN { bad_off += 1; } } } }
    println!("depth sort: overlap pixels {n}, comp3d on mismatches (want red) {bad_on}, comp3d off mismatches (want green) {bad_off}");
    assert!(n > 500, "overlap too small: {n}"); assert_eq!(bad_on, 0); assert_eq!(bad_off, 0);
}

// b) intersection: B is rotated (rot_x 12, rot_y 30) through A's depth; the colour switches on the analytic line 1/z_A = 1/z_B.
#[test] fn crossing_planes_swap_order_on_the_analytic_intersection_line() {
    let (za, zb, xb, rxd, ryd) = (200.0, 200.0, 20.0, 12.0f64, 30.0f64);
    let build = |with_a: bool, with_b: bool| { let mut ss = session();
        if with_a { layer(&mut ss, W as f64, H as f64, [1.0, 0.0, 0.0], &[("pos_z", za), ("scale", 2.0)]); }
        if with_b { layer(&mut ss, W as f64, H as f64, [0.0, 1.0, 0.0], &[("pos_z", zb), ("x", xb), ("rot_x", rxd), ("rot_y", ryd), ("scale", 2.0)]); }
        on(&mut ss); bytes(&ss) };
    let (both, only_a, only_b) = (build(true, true), build(true, false), build(false, true)); let f = f_px();
    // normals n = R (0,0,-1), R = Rx Ry: n = (-sin ry, sin rx cos ry, -cos rx cos ry)
    let (sx, cx) = rxd.to_radians().sin_cos(); let (sy, cy) = ryd.to_radians().sin_cos();
    let na = [0.0, 0.0, -1.0]; let nb = [-sy, sx * cy, -cx * cy];
    let cam = [0.0, 0.0, -f]; let dotp = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let nna = dotp([0.0 - cam[0], 0.0 - cam[1], za - cam[2]], na); let nnb = dotp([xb - cam[0], 0.0 - cam[1], zb - cam[2]], nb);
    // 1/z = (d.n)/(N f), affine in (x, y)
    let invz = |n: [f64; 3], nn: f64, x: f64, y: f64| dotp([x - W as f64 / 2.0, y - H as f64 / 2.0, f], n) / (nn * f);
    let (gx, gy) = (na[0] / (nna * f) - nb[0] / (nnb * f), na[1] / (nna * f) - nb[1] / (nnb * f)); let gl = (gx * gx + gy * gy).sqrt();
    let (mut ta, mut tb, mut bad, mut near) = (0, 0, 0, 0);
    for y in 0..H { for x in 0..W { let (px_, py_) = (x as f64 + 0.5, y as f64 + 0.5);
        if px(&only_a, x, y) != RED || px(&only_b, x, y) != GREEN { continue; }
        let hh = invz(na, nna, px_, py_) - invz(nb, nnb, px_, py_); let dist = hh.abs() / gl;
        if dist <= 1.5 { near += 1; continue; }
        let want = if hh > 0.0 { ta += 1; RED } else { tb += 1; GREEN };
        if px(&both, x, y) != want { bad += 1; } } }
    // where the line crosses the middle row: solve h(x, H/2) = 0
    let h0 = invz(na, nna, 0.0, H as f64 / 2.0) - invz(nb, nnb, 0.0, H as f64 / 2.0); let x_line = -h0 / gx;
    println!("intersection: line crosses row H/2 at x = {x_line:.3}, slope dx/dy = {:.4}; tested A-front {ta}, B-front {tb}, within 1.5 px skipped {near}, mismatches {bad}", -gy / gx);
    assert!(ta > 1000 && tb > 1000, "both sides must be tested: {ta} {tb}"); assert_eq!(bad, 0, "pixels with the wrong front layer");
}

// c) shadow geometry: 40 px opaque square caster parallel to a wall, point light at a known position.
struct ShadowScene { lw: [f64; 3], xc: f64, zc: f64, zw: f64, side: f64 }
fn shadow_session(sc: &ShadowScene, caster_op: f64, shadows: Value, wall_accepts: bool) -> Session {
    let mut ss = session();
    let wall = layer(&mut ss, W as f64, H as f64, [1.0, 1.0, 1.0], &[("pos_z", sc.zw), ("scale", 2.0)]);
    let cst = layer(&mut ss, sc.side, sc.side, [0.0, 0.0, 1.0], &[("pos_z", sc.zc), ("x", sc.xc), ("opacity", caster_op)]);
    c(&mut ss, "set_3d_layer_shadows", json!({"clip_id": cst, "casts_shadows": true}));
    if !wall_accepts { c(&mut ss, "set_3d_layer_shadows", json!({"clip_id": wall, "accepts_shadows": false})); }
    // lighting.rs convention: x, y fractions of the frame, z frame-heights toward the viewer (world Z = -z H)
    let pos = [sc.lw[0] / W as f64 + 0.5, sc.lw[1] / H as f64 + 0.5, -sc.lw[2] / H as f64];
    c(&mut ss, "set_light", json!({"id": "key", "kind": "point", "pos": pos}));
    c(&mut ss, "set_3d_compositor", json!({"enabled": true, "shadows": shadows})); ss
}
#[test] fn shadow_matches_similar_triangles_and_transmits_through_alpha() {
    let sc = ShadowScene { lw: [-138.0, 0.0, -200.0], xc: -55.0, zc: -100.0, zw: 100.0, side: 40.0 }; let f = f_px();
    // light position exactly as the engine reconstructs it from the stored fractions
    let pos = [sc.lw[0] / W as f64 + 0.5, sc.lw[1] / H as f64 + 0.5, -sc.lw[2] / H as f64]; let l = kimo_core::comp3d::light_world(pos, W as f64, H as f64);
    let k = (sc.zw - l[2]) / (sc.zc - l[2]); let s = [l[0] + (sc.xc - l[0]) * k, l[1] + (0.0 - l[1]) * k]; let ks = f / (sc.zw + f);
    let (pcx, pcy, pside) = (W as f64 / 2.0 + s[0] * ks, H as f64 / 2.0 + s[1] * ks, sc.side * k * ks);
    let off = frame(&shadow_session(&sc, 1.0, json!({"enabled": false}), true));
    let hard = frame(&shadow_session(&sc, 1.0, json!({"enabled": true, "softness": 0}), true));
    let dmap = |g: &Frame| -> Vec<f64> { (0..W * H).map(|i| (off.d[i * 4] - g.d[i * 4]) as f64).collect() };
    let d = dmap(&hard); let (mut sw, mut sx, mut sy, mut partial) = (0.0, 0.0, 0.0, 0);
    for y in 0..H { for x in 0..W { let v = d[y * W + x]; sw += v; sx += v * (x as f64 + 0.5); sy += v * (y as f64 + 0.5); if v > 0.01 && v < 0.99 { partial += 1; } } }
    let (mcx, mcy, mside) = (sx / sw, sy / sw, sw.sqrt());
    // 10-90 % width of the right edge along the centre row
    let width = |d: &[f64]| { let row = pcy as usize; let xs: Vec<f64> = (0..W).map(|x| d[row * W + x]).collect(); let xe = (pcx + pside / 2.0) as usize;
        let cross = |lv: f64| { let mut x = xe.saturating_sub(40); while x + 2 < W && !(xs[x] >= lv && xs[x + 1] < lv) { x += 1; } x as f64 + 0.5 + (xs[x] - lv) / (xs[x] - xs[x + 1]).max(1e-12) }; cross(0.1) - cross(0.9) };
    println!("shadow: predicted centre ({pcx:.3}, {pcy:.3}) side {pside:.3} px; measured centre ({mcx:.3}, {mcy:.3}) side {mside:.3} px; partial pixels {partial}");
    assert!((mcx - pcx).abs() <= 0.5 && (mcy - pcy).abs() <= 0.5, "centroid off");
    assert!((mside - pside).abs() <= 0.01 * pside, "size off by more than 1%");
    let w_hard = width(&d);
    let soft = frame(&shadow_session(&sc, 1.0, json!({"enabled": true, "softness": 20, "samples": 64}), true)); let w_soft = width(&dmap(&soft));
    println!("shadow: 10-90 edge width hard {w_hard:.3} px, softness 20 {w_soft:.3} px");
    assert!(w_hard < 3.5, "hard shadow edge too soft: {w_hard}"); assert!((partial as f64) <= 4.0 * (pside + 4.0) * 4.0, "partial pixels exceed a 4 px band around the edge: {partial}");
    assert!(w_soft > w_hard + 5.0, "softness must widen the penumbra: {w_soft} vs {w_hard}");
    // semi-transparent caster (opacity 0.5): transmittance 0.5 inside the umbra
    let half = frame(&shadow_session(&sc, 0.5, json!({"enabled": true, "softness": 0}), true));
    let (mut worst, mut n) = (0f64, 0); let r = pside / 2.0 - 4.0;
    for y in 0..H { for x in 0..W { let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5); if (cx - pcx).abs() < r && (cy - pcy).abs() < r { let i = (y * W + x) * 4; let t = (half.d[i] / off.d[i]) as f64; worst = worst.max((t - 0.5).abs()); n += 1; } } }
    println!("alpha 0.5 caster: {n} umbra pixels, worst |T - 0.5| = {worst:.3e}");
    assert!(n > 1000); assert!(worst <= 1e-3);
    // accepts_shadows 0: the wall is bit-identical to shadows off
    let refuse = frame(&shadow_session(&sc, 1.0, json!({"enabled": true, "softness": 0}), false));
    let diff = bits(&refuse).iter().zip(bits(&off).iter()).filter(|(a, b)| a != b).count(); println!("accepts_shadows 0: {diff} differing floats");
    assert_eq!(diff, 0);
}

// d) depth of field: CoC = A f |1/z_f - 1/z|; LSF sigma of a disc of radius R = CoC/2 is R/2.
fn dof_session(pos_z: f64, dof: Option<Value>) -> Session {
    let mut ss = session(); layer(&mut ss, 200.0, 600.0, [1.0, 1.0, 1.0], &[("pos_z", pos_z)]);
    let mut a = json!({"enabled": true}); if let Some(d) = dof { a["dof"] = d; } c(&mut ss, "set_3d_compositor", a); ss
}
fn edge_sigma2(f: &Frame, xe: f64) -> f64 {
    let row = H / 2; let e: Vec<f64> = (0..W).map(|x| f.d[(row * W + x) * 4] as f64).collect();
    let (a, b) = ((xe - 30.0) as usize, (xe + 30.0) as usize); let (mut s0, mut s1, mut s2) = (0.0, 0.0, 0.0);
    for i in a..b { let de = e[i + 1] - e[i]; let x = i as f64 + 1.0; s0 += de; s1 += de * x; s2 += de * x * x; }
    let mu = s1 / s0; s2 / s0 - mu * mu
}
#[test] fn depth_of_field_focus_plane_is_exact_and_blur_matches_the_thin_lens_coc() {
    let f = f_px(); let zf = f + 100.0; let ap = 80.0;
    let dof = json!({"enabled": true, "aperture": ap, "focus_distance": zf, "samples": 128});
    // at the focus distance: bit-identical to DOF off, and to the compositor off (a one-layer group)
    let sharp = frame(&dof_session(100.0, None)); let focus = frame(&dof_session(100.0, Some(dof.clone())));
    let mut plain = session(); layer(&mut plain, 200.0, 600.0, [1.0, 1.0, 1.0], &[("pos_z", 100.0)]); let plain = frame(&plain);
    let d1 = bits(&focus).iter().zip(bits(&sharp).iter()).filter(|(a, b)| a != b).count(); let d2 = bits(&sharp).iter().zip(bits(&plain).iter()).filter(|(a, b)| a != b).count();
    println!("dof at focus: {d1} floats differ from DOF off, {d2} from compositor off");
    assert_eq!(d1, 0); assert_eq!(d2, 0);
    // off focus: z = f + 300
    let z = f + 300.0; let coc = ap * f * (1.0 / zf - 1.0 / z).abs(); let r = coc / 2.0; let xe = W as f64 / 2.0 - 100.0 * f / z;
    let base = edge_sigma2(&frame(&dof_session(300.0, None)), xe); let blur = edge_sigma2(&frame(&dof_session(300.0, Some(dof))), xe);
    let sigma = (blur - base).max(0.0).sqrt(); let want = r / 2.0;
    println!("dof off focus: CoC {coc:.3} px (R {r:.3}), edge at x {xe:.2}; LSF variance sharp {base:.4}, blurred {blur:.4}; sigma {sigma:.4} vs predicted R/2 = {want:.4} ({:+.2}%)", 100.0 * (sigma / want - 1.0));
    assert!((sigma - want).abs() <= 0.10 * want, "blur width off by more than 10%");
}
#[test] fn coc_is_linear_in_inverse_depth_and_monotone() {
    let f = f_px(); let d = kimo_core::comp3d::Dof { enabled: true, focus_distance: 2.0 * f, aperture: 30.0, max_blur_px: 1e9, samples: 32 };
    let zs: Vec<f64> = (1..=40).map(|k| f * (0.5 + 0.1 * k as f64)).collect(); let cs: Vec<f64> = zs.iter().map(|&z| kimo_core::comp3d::coc_px(&d, f, z)).collect();
    let mut worst = 0f64; for (&z, &cv) in zs.iter().zip(cs.iter()) { let lin = (1.0 / (2.0 * f) - 1.0 / z).abs(); if lin > 1e-9 { worst = worst.max((cv / (lin * 30.0 * f) - 1.0).abs()); } }
    let mut mono = true; for k in 1..zs.len() { let (a, b) = (cs[k - 1], cs[k]); if zs[k] <= 2.0 * f { mono &= b <= a; } else if zs[k - 1] >= 2.0 * f { mono &= b >= a; } }
    println!("coc: at z = f {:.4}, 2f {:.4}, 4f {:.4}; worst |CoC / (A f |1/zf - 1/z|) - 1| = {worst:.2e}; monotone {mono}", kimo_core::comp3d::coc_px(&d, f, f), kimo_core::comp3d::coc_px(&d, f, 2.0 * f), kimo_core::comp3d::coc_px(&d, f, 4.0 * f));
    assert!(worst < 1e-12); assert!(mono); assert_eq!(kimo_core::comp3d::coc_px(&d, f, 2.0 * f), 0.0);
    let capped = kimo_core::comp3d::Dof { max_blur_px: 5.0, ..d }; assert_eq!(kimo_core::comp3d::coc_px(&capped, f, 0.5 * f), 5.0);
}

// e) defaults: off is byte-identical; a 2D-only project ignores the compositor; a 2D layer breaks a group.
fn hash(f: &Frame) -> u64 { let mut h: u64 = 0xcbf29ce484222325; for v in &f.d { for b in v.to_le_bytes() { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); } } h }
#[test] fn compositor_off_and_2d_projects_are_unchanged() {
    let scene3d = || { let mut ss = session(); layer(&mut ss, 120.0, 120.0, [1.0, 0.0, 0.0], &[("pos_z", 50.0)]); layer(&mut ss, 160.0, 90.0, [0.0, 1.0, 0.0], &[("pos_z", 120.0), ("rot_y", 35.0), ("x", 20.0)]); ss };
    let ss = scene3d(); let h0 = hash(&frame(&ss)); assert!(!serde_json::to_string(&ss.project).unwrap().contains("comp3d"), "absent field is not serialized");
    let mut off = scene3d(); c(&mut off, "set_3d_compositor", json!({"enabled": false, "dof": {"enabled": true, "aperture": 50}, "shadows": {"enabled": true, "softness": 4}}));
    let h1 = hash(&frame(&off));
    // a project file without the field loads as None
    let txt = serde_json::to_string(&ss.project).unwrap(); let back: kimo_core::model::Project = serde_json::from_str(&txt).unwrap(); assert!(back.comp3d.is_none());
    let scene2d = || { let mut ss = session(); layer(&mut ss, 120.0, 120.0, [1.0, 0.0, 0.0], &[("x", -20.0)]); layer(&mut ss, 160.0, 90.0, [0.0, 1.0, 0.0], &[("rotation", 20.0), ("opacity", 0.7)]); ss };
    let h2 = hash(&frame(&scene2d())); let mut on2 = scene2d(); c(&mut on2, "set_3d_compositor", json!({"enabled": true, "dof": {"enabled": true, "aperture": 50}, "shadows": {"enabled": true}})); let h3 = hash(&frame(&on2));
    let one = || { let mut ss = session(); layer(&mut ss, 120.0, 120.0, [0.2, 0.5, 0.9], &[("pos_z", 80.0), ("rot_x", 25.0), ("opacity", 0.8)]); ss };
    let h4 = hash(&frame(&one())); let mut one_on = one(); on(&mut one_on); let h5 = hash(&frame(&one_on));
    println!("3D project: absent {h0:016x}, enabled=false {h1:016x}; 2D project: off {h2:016x}, on {h3:016x}; one 3D layer: off {h4:016x}, on {h5:016x}");
    assert_eq!(h0, h1); assert_eq!(h2, h3); assert_eq!(h4, h5);
    // red near (bottom), 2D blue (middle), green far (top): the 2D layer splits the groups, so green stays on top where it covers
    let mut ss = session(); layer(&mut ss, 120.0, 120.0, [1.0, 0.0, 0.0], &[("pos_z", 50.0)]); layer(&mut ss, 40.0, 40.0, [0.0, 0.0, 1.0], &[]); layer(&mut ss, 120.0, 120.0, [0.0, 1.0, 0.0], &[("pos_z", 300.0)]); on(&mut ss);
    let b = bytes(&ss); let centre = px(&b, W / 2, H / 2); println!("2D layer between 3D layers: centre pixel {centre:?}"); assert_eq!(centre, GREEN);
}

// f) refusals leave the project and the undo stack unchanged.
#[test] fn bad_parameters_are_refused_and_change_nothing() {
    let mut ss = session(); let id = layer(&mut ss, 50.0, 50.0, [1.0, 1.0, 1.0], &[("pos_z", 10.0)]); c(&mut ss, "set_light", json!({"id": "key", "kind": "point"}));
    let before = serde_json::to_string(&ss.project).unwrap(); let depth = ss.undo.len();
    let cases: Vec<(&str, Value, &str)> = vec![
        ("set_3d_compositor", json!({"enabled": true, "dof": {"aperture": -1}}), "aperture"),
        ("set_3d_compositor", json!({"enabled": true, "dof": {"samples": 0}}), "samples"),
        ("set_3d_compositor", json!({"enabled": true, "dof": {"samples": 257}}), "samples"),
        ("set_3d_compositor", json!({"enabled": true, "dof": {"samples": 2.5}}), "samples"),
        ("set_3d_compositor", json!({"enabled": true, "shadows": {"samples": 0}}), "samples"),
        ("set_3d_compositor", json!({"enabled": true, "shadows": {"light_id": "nope"}}), "unknown light"),
        ("set_3d_compositor", json!({"enabled": true, "shadows": {"opacity": 2}}), "opacity"),
        ("set_3d_compositor", json!({"enabled": true, "dof": {"focus": 3}}), "unknown field"),
        ("set_3d_compositor", json!({"enabled": "yes"}), "boolean"),
        ("set_3d_layer_shadows", json!({"clip_id": id}), "casts_shadows"),
        ("set_3d_layer_shadows", json!({"clip_id": "clip_missing", "casts_shadows": true}), "clip"),
    ];
    for (tool, a, want) in &cases { let e = dispatch(&mut ss, tool, a).err().unwrap_or_else(|| panic!("{tool} {a} should fail")); println!("{tool} {a} -> {e}");
        assert!(e.contains(want), "error '{e}' lacks '{want}'"); assert_eq!(serde_json::to_string(&ss.project).unwrap(), before, "a refused call changed the project"); assert_eq!(ss.undo.len(), depth); }
    // a valid call does change it, and names the light
    let r = c(&mut ss, "set_3d_compositor", json!({"enabled": true, "shadows": {"enabled": true, "light_id": "key"}})); println!("accepted: {r}"); assert_eq!(r["comp3d"]["shadows"]["light_id"], "key");
}
