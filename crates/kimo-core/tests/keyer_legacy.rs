//! Tests for the existing Keylight-style `keyer` (keying.rs), which had none (color_hostile.rs skipped it), plus the render-path
//! wiring of the keyer's clean plate (strings.ref_clip / aux_path) and garbage / core mattes (strings.garbage_mask / core_mask:
//! ids of mask layers in the clip's stack, mode none).
use kimo_core::color::apply3;
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{Frame, Params};
use kimo_core::keyframe::Prop;
use serde_json::{json, Value};

fn params(kv: &[(&str, f64)]) -> Params { let mut p = Params::new(); for (k, v) in kv { p.insert(k.to_string(), Prop::new(*v)); } p }
fn solid(w: usize, h: usize, c: [f32; 3]) -> Frame { let mut f = Frame { w, h, d: vec![1.0; w * h * 4] }; for i in 0..w * h { f.d[i * 4..i * 4 + 3].copy_from_slice(&c); } f }
fn key(pr: &Params, f: &mut Frame) { apply3("keyer", pr, f, 0.0, 0).expect("keyer"); }
fn fnv(d: &[f32]) -> u64 { let mut h = 0xcbf29ce484222325u64; for v in d { for b in v.to_bits().to_le_bytes() { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); } } h }

/// A deterministic plate: green screen, an opaque skin disc, an anti-aliased edge and a spilled stripe.
fn legacy_plate() -> Frame {
    let (w, h) = (48usize, 32usize); let mut f = solid(w, h, [0.1, 0.75, 0.1]);
    for y in 0..h { for x in 0..w { let d = (((x as f32 + 0.5) - 20.0).powi(2) + ((y as f32 + 0.5) - 16.0).powi(2)).sqrt(); let a = (10.0 - d + 0.5).clamp(0.0, 1.0);
        let fg = if x > 26 { [0.6, 0.7, 0.55] } else { [0.8, 0.6, 0.5] }; let a = if x > 26 && x < 34 { 1.0 } else { a };
        let o = (y * w + x) * 4; for j in 0..3 { f.d[o + j] = a * fg[j] + (1.0 - a) * f.d[o + j]; } } }
    f
}

#[test]
fn legacy_keyer_output_is_pinned() {
    // Captured from b160f994 (the keyer before lane K1). Default parameters and every view must stay byte-identical.
    let mut got = vec![];
    for view in [0.0, 1.0, 2.0] { let mut f = legacy_plate(); key(&params(&[("view", view)]), &mut f); got.push(format!("{:016x}", fnv(&f.d))); }
    let mut f = legacy_plate(); key(&params(&[("core_shrink", 1.0), ("edge_soften", 0.0), ("screen_balance", 0.2)]), &mut f); got.push(format!("{:016x}", fnv(&f.d)));
    let mut f = legacy_plate(); key(&params(&[("core_shrink", -2.0), ("despill", 0.5), ("spill_replace", 1.0), ("edge_unmix", 0.0)]), &mut f); got.push(format!("{:016x}", fnv(&f.d)));
    eprintln!("legacy hashes {got:?}");
    assert_eq!(got, PINNED);
}
const PINNED: [&str; 5] = ["80629b1dc9c71426", "7ea65fba918a4feb", "584fd660ae6d4ccc", "81783bb7d7228ba9", "85eabe906b24667f"];

#[test]
fn legacy_keyer_basic_properties() {
    // pure screen -> transparent; opaque neutral grey -> opaque, colour untouched
    let mut s = solid(8, 8, [0.0, 0.75, 0.1]); key(&params(&[]), &mut s); assert!(s.d.chunks(4).all(|p| p[3] == 0.0), "screen (= the default key colour) must key out");
    let mut g = solid(8, 8, [0.5, 0.5, 0.5]); key(&params(&[]), &mut g); assert!(g.d.chunks(4).all(|p| p[3] == 1.0 && p[0] == 0.5 && p[1] == 0.5 && p[2] == 0.5));
    // view 1 = matte in rgb, alpha 1; view 2 = despilled colour, alpha 1
    let mut m = legacy_plate(); key(&params(&[("view", 1.0)]), &mut m); assert!(m.d.chunks(4).all(|p| p[3] == 1.0 && p[0] == p[1] && p[1] == p[2]));
    let mut c = legacy_plate(); key(&params(&[("view", 2.0)]), &mut c); assert!(c.d.chunks(4).all(|p| p[3] == 1.0));
    // despill limits green to the average of red and blue on an opaque spilled pixel
    // (clip_white 0.5 makes the spilled pixel opaque, edge_unmix 0 keeps unmixing out of the measurement)
    let mut sp = solid(4, 4, [0.6, 0.7, 0.55]); key(&params(&[("spill_replace", 0.0), ("clip_white", 0.5), ("edge_unmix", 0.0)]), &mut sp); let p = &sp.d[..4]; assert!((p[1] - 0.575).abs() < 1e-6, "{p:?}"); assert_eq!(p[3], 1.0);
    let mut sp0 = solid(4, 4, [0.6, 0.7, 0.55]); key(&params(&[("despill", 0.0), ("clip_white", 0.5), ("edge_unmix", 0.0)]), &mut sp0); assert_eq!(sp0.d[1], 0.7);
    // a higher clip_black removes more of a partially transparent pixel; clip_white makes it opaque sooner
    let edge = |cb: f64, cw: f64| { let mut f = solid(4, 4, [0.45, 0.72, 0.3]); key(&params(&[("clip_black", cb), ("clip_white", cw), ("edge_soften", 0.0)]), &mut f); f.d[3] };
    assert!(edge(0.3, 0.95) < edge(0.05, 0.95) && edge(0.05, 0.6) > edge(0.05, 0.95));
    // core_shrink erodes: the opaque area shrinks
    let area = |s: f64| { let mut f = legacy_plate(); key(&params(&[("core_shrink", s), ("edge_soften", 0.0)]), &mut f); f.d.chunks(4).filter(|p| p[3] > 0.5).count() };
    assert!(area(2.0) < area(0.0) && area(-2.0) > area(0.0));
}

#[test]
fn hostile_shrink_and_soften_finish_and_match_the_frame_bound() {
    // core_shrink / edge_soften of 1e8 px used to loop 2e8 times per pixel (why color_hostile skipped the keyer). A radius past the
    // frame is the same operation as a radius equal to the frame, so the result must equal the frame-bound radius exactly.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || { let mut a = legacy_plate(); key(&params(&[("core_shrink", 1e8)]), &mut a); let mut b = legacy_plate(); key(&params(&[("core_shrink", 48.0)]), &mut b);
        let mut c = legacy_plate(); key(&params(&[("edge_soften", 1e8)]), &mut c); tx.send((a.d == b.d, c.d.iter().all(|v| v.is_finite()))).unwrap(); });
    let (same, finite) = rx.recv_timeout(std::time::Duration::from_secs(20)).expect("keyer with core_shrink/edge_soften 1e8 did not finish in 20 s");
    assert!(same && finite);
}

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
/// Alpha of one clip rendered alone (its effects, no other clips).
fn clip_alpha(s: &Session, clip: &str) -> Vec<f32> { let mut cache = kimo_core::render::Cache::new(); kimo_core::render::prepare_at(&s.project, &mut cache, Some(0.5)).unwrap();
    kimo_core::render::render_clip_alone(&s.project, clip, 0.5, &cache).expect("clip frame").d.chunks(4).map(|p| p[3]).collect() }
/// Alpha of the transparent composite at t=0.5. The keyed clip is the only visible clip: a keyer's clean-plate clip is hidden from the composite.
fn composite_alpha(s: &Session) -> Vec<f32> { let mut cache = kimo_core::render::Cache::new(); kimo_core::render::prepare_at(&s.project, &mut cache, Some(0.5)).unwrap();
    let f = kimo_core::render::render_frame_opts(&s.project, 0.5, &cache, true).expect("composite"); f.d.chunks(4).map(|p| p[3]).collect() }

/// A radial "vignetted" green screen (bright centre, dark corners) and an identical clean-plate clip on a muted track.
fn vignette_session() -> (Session, String, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":64,"height":36,"fps":25}));
    let t0 = c(&mut s, "add_track", json!({}))["track_id"].clone(); let t1 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let g = json!({"source_type":"gradient","gradient":"radial","color":[0.12,0.84,0.18],"color2":[0.08,0.56,0.12],"start":0,"duration":1});
    let mut a = g.clone(); a["track_id"] = t0; let plate = c(&mut s, "add_clip", a)["clip_id"].as_str().unwrap().to_string();
    let mut b = g.clone(); b["track_id"] = t1; let shot = c(&mut s, "add_clip", b)["clip_id"].as_str().unwrap().to_string();
    (s, plate, shot)
}

#[test]
fn clean_plate_clip_keys_an_uneven_screen() {
    let (mut s, plate, shot) = vignette_session();
    let fx = c(&mut s, "add_effect", json!({"clip_id":shot,"kind":"keyer","params":{"r":0.1,"g":0.7,"b":0.15,"matte_model":1,"edge_soften":0}}))["effect_id"].clone();
    let without = clip_alpha(&s, &shot); let worst = without.iter().cloned().fold(0f32, f32::max); eprintln!("vignetted screen, no clean plate: worst alpha {worst}");
    assert!(worst > 0.05, "an uneven screen keyed against one colour must leave residue (got {worst})");
    c(&mut s, "remove_effect", json!({"clip_id":shot,"effect_id":fx}));
    c(&mut s, "add_effect", json!({"clip_id":shot,"kind":"keyer","params":{"r":0.1,"g":0.7,"b":0.15,"matte_model":1,"edge_soften":0},"strings":{"ref_clip":plate}}));
    let with = composite_alpha(&s); let worst = with.iter().cloned().fold(0f32, f32::max); eprintln!("vignetted screen, clean plate clip: worst alpha {worst}");
    assert!(worst <= 0.005, "with the clean plate the screen must key out and the plate clip stays hidden (worst alpha {worst})");
}

#[test]
fn garbage_and_core_mask_layers_drive_the_keyer() {
    let (mut s, _plate, shot) = vignette_session();
    let core = c(&mut s, "add_mask", json!({"clip_id":shot,"kind":"rect","mode":"none","params":{"cx":0.5,"cy":0.5,"w":0.25,"h":0.25}}))["mask_id"].as_str().unwrap().to_string();
    let garbage = c(&mut s, "add_mask", json!({"clip_id":shot,"kind":"rect","mode":"none","params":{"cx":0.1,"cy":0.5,"w":0.2,"h":1.0}}))["mask_id"].as_str().unwrap().to_string();
    // a grey card the screen does not remove, so only the garbage matte can take it out
    c(&mut s, "add_effect", json!({"clip_id":shot,"kind":"grade","params":{"gain_r":0.0,"gain_g":0.0,"gain_b":0.0,"lift_r":0.5,"lift_g":0.5,"lift_b":0.5}}));
    c(&mut s, "add_effect", json!({"clip_id":shot,"kind":"keyer","params":{"matte_model":1,"edge_soften":0},"strings":{"garbage_mask":garbage,"core_mask":core}}));
    let a = clip_alpha(&s, &shot); let at = |x: usize, y: usize| a[y * 64 + x];
    assert_eq!(at(2, 18), 0.0, "inside the garbage matte"); assert_eq!(at(32, 18), 1.0, "grey card outside the garbage stays"); assert_eq!(at(60, 18), 1.0);
    // the core matte forces opaque even over pure screen (a flat screen here, so outside the core keys to 0 without a clean plate)
    let mut s2 = Session::new(); c(&mut s2, "project_set", json!({"width":64,"height":36,"fps":25})); let t2 = c(&mut s2, "add_track", json!({}))["track_id"].clone();
    let shot2 = c(&mut s2, "add_clip", json!({"track_id":t2,"source_type":"solid","color":[0.12,0.84,0.18],"start":0,"duration":1}))["clip_id"].as_str().unwrap().to_string();
    let core2 = c(&mut s2, "add_mask", json!({"clip_id":shot2,"kind":"rect","mode":"none","params":{"cx":0.5,"cy":0.5,"w":0.25,"h":0.25}}))["mask_id"].as_str().unwrap().to_string();
    c(&mut s2, "add_effect", json!({"clip_id":shot2,"kind":"keyer","params":{"r":0.12,"g":0.84,"b":0.18,"matte_model":1,"edge_soften":0},"strings":{"core_mask":core2}}));
    let a2 = clip_alpha(&s2, &shot2); assert_eq!(a2[18 * 64 + 32], 1.0, "core matte centre"); assert_eq!(a2[18 * 64 + 2], 0.0, "screen outside the core");
}
