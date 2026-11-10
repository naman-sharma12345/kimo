//! M3: peak live heap bytes while ONE 1080p frame is rendered, on three scenes, vs the export planner's assumption of 12 full-frame f32 buffers
//! (16 bytes per pixel each). Single render, sampled every 200 us from a second thread; the baseline (live before the render, including the Cache and
//! the project) is subtracted. Prints the numbers (run with --nocapture) and fails when a scene exceeds the planner's per-frame budget.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
#[global_allocator] static A: kimo_core::guard::Counting = kimo_core::guard::Counting;
static mut WH: (u32, u32) = (1920, 1080); fn wh() -> (u32, u32) { unsafe { WH } }
fn c(s: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn base() -> Session { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": wh().0, "height": wh().1, "fps": 24})); s }
fn simple() -> Session { let mut s = base(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.3, 0.5], "start": 0, "duration": 2})); s }
fn glass() -> Session { let mut s = simple(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_shard_burst", json!({"track_id": t, "start": 0, "duration": 2, "config": {"count": 120, "seed": 3, "env_reflection": 0.6}}));
    let a = c(&mut s, "add_track", json!({}))["track_id"].clone(); let adj = c(&mut s, "add_adjustment_layer", json!({"track_id": a, "start": 0, "duration": 2}))["clip_id"].clone();
    c(&mut s, "add_effect", json!({"clip_id": adj, "kind": "glow"})); s }
fn adj_mask() -> Session { let mut s = base(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let cl = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.8, 0.3, 0.2], "start": 0, "duration": 2}))["clip_id"].clone();
    c(&mut s, "add_mask", json!({"clip_id": cl, "kind": "ellipse", "feather": 40, "params": {"cx": 0.5, "cy": 0.5, "w": 0.6, "h": 0.6}}));
    c(&mut s, "add_effect", json!({"clip_id": cl, "kind": "blur"}));
    let a = c(&mut s, "add_track", json!({}))["track_id"].clone(); let adj = c(&mut s, "add_adjustment_layer", json!({"track_id": a, "start": 0, "duration": 2}))["clip_id"].clone();
    c(&mut s, "add_effect", json!({"clip_id": adj, "kind": "blur"})); s }

fn moving(s: &mut Session, t: &serde_json::Value) -> serde_json::Value {
    let cl = c(s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.9, 0.7, 0.2], "start": 0, "duration": 2}))["clip_id"].clone();
    c(s, "set_property", json!({"clip_id": cl, "property": "scale", "value": 0.4}));
    c(s, "set_keyframe", json!({"clip_id": cl, "property": "x", "time": 0.0, "value": -300.0})); c(s, "set_keyframe", json!({"clip_id": cl, "property": "x", "time": 2.0, "value": 300.0}));
    c(s, "set_keyframe", json!({"clip_id": cl, "property": "rotation", "time": 0.0, "value": 0.0})); c(s, "set_keyframe", json!({"clip_id": cl, "property": "rotation", "time": 2.0, "value": 90.0})); cl }
fn motion_blur() -> Session { let mut s = simple(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); let cl = moving(&mut s, &t); c(&mut s, "set_motion_blur", json!({"clip_id": cl, "shutter_angle": 360})); s }
fn three_d() -> Session { let mut s = simple(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); let cl = moving(&mut s, &t);
    c(&mut s, "set_property", json!({"clip_id": cl, "property": "rot_y", "value": 40.0})); c(&mut s, "set_property", json!({"clip_id": cl, "property": "rot_x", "value": 20.0})); c(&mut s, "set_camera", json!({"cam_fov": 60})); s }
fn precomp3() -> Session { let mut s = base(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let a = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.6, 0.4], "start": 0, "duration": 2}))["clip_id"].clone();
    let mut g = a; for _ in 0..3 { g = c(&mut s, "group_clips", json!({"clip_ids": [g]}))["clip_id"].clone(); } s }
fn stack() -> Session { let mut s = glass(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); let adj = c(&mut s, "add_adjustment_layer", json!({"track_id": t, "start": 0, "duration": 2}))["clip_id"].clone();
    c(&mut s, "add_effect", json!({"clip_id": adj, "kind": "blur"})); c(&mut s, "add_effect", json!({"clip_id": adj, "kind": "bloom"})); s }
fn simple_4k() -> Session { unsafe { WH = (3840, 2160); } let s = simple(); s }
fn peak(s: &Session) -> usize {
    let cache = Cache::new(); let base = kimo_core::guard::LIVE.load(Relaxed); let hi = AtomicUsize::new(base); let stop = AtomicBool::new(false);
    std::thread::scope(|sc| { sc.spawn(|| while !stop.load(Relaxed) { hi.fetch_max(kimo_core::guard::LIVE.load(Relaxed), Relaxed); std::thread::sleep(std::time::Duration::from_micros(200)); });
        let f = render_frame(&s.project, 0.5, &cache).unwrap(); hi.fetch_max(kimo_core::guard::LIVE.load(Relaxed), Relaxed); drop(f); stop.store(true, Relaxed); });
    hi.load(Relaxed) - base
}
#[test] fn peak_live_per_frame_is_within_the_planner_budget() {
    let mut worst = (String::new(), 0.0f64);
    let scenes: Vec<(&str, fn() -> Session, (u32, u32))> = vec![("simple", simple, (1920, 1080)), ("glass+adjustment", glass, (1920, 1080)), ("adjustment+mask", adj_mask, (1920, 1080)), ("motion blur 360", motion_blur, (1920, 1080)), ("3D rot_x/rot_y + camera", three_d, (1920, 1080)), ("precomp depth 3", precomp3, (1920, 1080)), ("glass+glow+blur+bloom stack", stack, (1920, 1080)), ("simple 4K", simple_4k, (3840, 2160))];
    for (name, mk, (w, h)) in scenes { unsafe { WH = (w, h); } let s = mk(); let frame = (w * h) as usize * 16; let p = peak(&s); let b = p as f64 / frame as f64;
        eprintln!("PEAK {name} {w}x{h}: {} MB = {:.2} full-frame buffers (frame buffer {} MB)", p >> 20, b, frame >> 20);
        if b > worst.1 { worst = (name.to_string(), b); }
        let est = kimo_core::render::frame_buffers(&s.project); eprintln!("  planner estimate {est} buffers");
        assert!(b <= est as f64, "{name}: measured peak {b:.2} buffers exceeds the planner estimate {est}");
        if ["motion blur 360", "3D rot_x/rot_y + camera", "precomp depth 3"].contains(&name) { assert_eq!(est, 12, "{name} must keep the safe 12 until measured over more scenes"); }
    }
    eprintln!("WORST {} = {:.2} buffers", worst.0, worst.1);
}

#[test] fn simple_1080p_scene_gets_two_workers_on_the_2gb_box() {
    let s = simple(); assert_eq!(kimo_core::render::frame_buffers(&s.project), 4);
    // A deterministic 2 GB box injected into the planner (the host's own RAM and budget play no part; host/cgroup detection is tested in
    // guard.rs): 1982 MB physical, the default budget of 55% = 1090 MB, nothing live yet, and 1200 MB MemAvailable (the floor this test
    // used to require of the host before asserting).
    let box2g = kimo_core::guard::MemProfile { budget: kimo_core::guard::default_budget(1982 << 20), live: 0, available: Some(1200 << 20) };
    assert_eq!(box2g.budget >> 20, 1090);
    let new = kimo_core::guard::export_plan_with(&box2g, 1920, 1080, 2, false, 4).unwrap().0; let old = kimo_core::guard::export_plan_with(&box2g, 1920, 1080, 2, false, 12).unwrap().0;
    eprintln!("WORKERS simple 1080p, 2 cpus, 1090 MB budget: {old} (12 buffers) -> {new} (4 buffers)");
    assert_eq!(old, 1); assert_eq!(new, 2);
}
