//! V3-0 step 0: in-process render bench (no MCP, no ffmpeg). 30 frames per scene at 1280x720 and 1920x1080, one thread, best of 3 passes, ms/frame.
//! Run: cargo test --release --test render_bench -- --ignored --nocapture
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, set_shape_bound, set_shape_hint, Cache};
use serde_json::json;

static mut WH: (u32,u32) = (1920,1080); fn wh()->(u32,u32){unsafe{WH}}
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

fn small(kind: &str) -> Session { let mut s = base(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.3, 0.5], "start": 0, "duration": 2}));
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone(); let cl = c(&mut s, "add_shape", json!({"track_id": t2, "shape": "rect", "width": 200, "height": 100, "fill": [0.9, 0.5, 0.2, 1], "start": 0, "duration": 2}))["clip_id"].clone();
    if kind != "none" { c(&mut s, "add_effect", json!({"clip_id": cl, "kind": kind})); } s }
#[test] #[ignore] fn render_bench() {
    for (w, h) in [(1280u32, 720u32), (1920, 1080)] { unsafe { WH = (w, h); }
        for (name, s) in [("simple", simple()), ("glass+adjustment", glass()), ("adjustment+mask", adj_mask()), ("small 200x100 layer, no effect", small("none")), ("small layer + blur (LOCAL)", small("blur")), ("small layer + glow (LOCAL)", small("glow")), ("small layer + exposure (pixelwise)", small("exposure"))] {
            if let Ok(only) = std::env::var("BENCH_ONLY") { if !format!("{w}x{h} {name}").contains(&only) { continue; } }
            if std::env::var("BENCH_HINT").is_ok() { set_shape_bound(true); for round in 0..3 { for on in [false, true] { set_shape_hint(on); let cache = Cache::new(); let t = std::time::Instant::now(); for i in 0..30 { let f = render_frame(&s.project, i as f64 / 24.0, &cache).unwrap(); std::hint::black_box(&f); } eprintln!("AB {w}x{h} {name} bbox_hint={on} round{round}: {:.1} ms/frame", t.elapsed().as_secs_f64() * 1000.0 / 30.0); } } set_shape_hint(true); continue; }
            if std::env::var("BENCH_AB").is_ok() { for round in 0..3 { for on in [false, true] { set_shape_bound(on); let cache = Cache::new(); let t = std::time::Instant::now(); for i in 0..30 { let f = render_frame(&s.project, i as f64 / 24.0, &cache).unwrap(); std::hint::black_box(&f); } eprintln!("AB {w}x{h} {name} shape_bound={on} round{round}: {:.1} ms/frame", t.elapsed().as_secs_f64() * 1000.0 / 30.0); } } set_shape_bound(true); continue; }
            let mut best = f64::MAX; for _ in 0..std::env::var("BENCH_PASSES").ok().and_then(|v| v.parse().ok()).unwrap_or(3) { let cache = Cache::new(); let t = std::time::Instant::now(); for i in 0..30 { let f = render_frame(&s.project, i as f64 / 24.0, &cache).unwrap(); std::hint::black_box(&f); } best = best.min(t.elapsed().as_secs_f64() * 1000.0 / 30.0); }
            eprintln!("BENCH {w}x{h} {name}: {best:.1} ms/frame");
        }
    }
}

/// V3-6a: frame cache numbers at 1080p (in-process, one session): cold render, warm hit, and scrubbing back and forth over 30 frames.
#[test] #[ignore] fn cache_bench() {
    for (w, h) in [(1280u32, 720u32), (1920, 1080)] {
        let mut s = Session::new(); c(&mut s, "project_set", json!({"width": w, "height": h, "fps": 24}));
        let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.3, 0.5], "start": 0, "duration": 3}));
        let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone(); let cl = c(&mut s, "add_shape", json!({"track_id": t2, "shape": "rect", "width": 600.0, "height": 300.0, "fill": [0.9, 0.5, 0.2, 1], "corner_radius": 20.0, "start": 0, "duration": 3}))["clip_id"].clone();
        c(&mut s, "add_effect", json!({"clip_id": cl, "kind": "glow"})); c(&mut s, "add_effect", json!({"clip_id": cl, "kind": "blur"}));
        let times: Vec<f64> = (0..30).map(|i| i as f64 / 24.0).collect(); let cache = Cache::new();
        let mut fc = kimo_core::framecache::FrameCache::new(2 << 30);
        let t0 = std::time::Instant::now(); for &t in &times { std::hint::black_box(render_frame(&s.project, t, &cache).unwrap()); } let uncached = t0.elapsed().as_secs_f64() * 1000.0 / 30.0;
        let t0 = std::time::Instant::now(); for &t in &times { std::hint::black_box(kimo_core::framecache::render_cached(&mut fc, &s.project, t, &cache).unwrap()); } let cold = t0.elapsed().as_secs_f64() * 1000.0 / 30.0;
        let t0 = std::time::Instant::now(); for &t in &times { std::hint::black_box(kimo_core::framecache::render_cached(&mut fc, &s.project, t, &cache).unwrap()); } let warm = t0.elapsed().as_secs_f64() * 1000.0 / 30.0;
        let tk = std::time::Instant::now(); let k = kimo_core::framecache::key(&s.project, 0.5, false); let key_ms = tk.elapsed().as_secs_f64() * 1000.0; std::hint::black_box(k);
        // scrub: bounce across the 30 cached frames
        let t0 = std::time::Instant::now(); let mut n = 0; for r in 0..4 { for i in 0..30 { let j = if r % 2 == 0 { i } else { 29 - i }; std::hint::black_box(kimo_core::framecache::render_cached(&mut fc, &s.project, times[j], &cache).unwrap()); n += 1; } } let scrub = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
        eprintln!("CACHE {w}x{h}: uncached {uncached:.1} ms/frame, cold (render+store) {cold:.1}, warm hit {warm:.1}, scrub over cached frames {scrub:.1} ms/frame, key {key_ms:.2} ms, cache {} MB, hits {} misses {}", fc.bytes() >> 20, fc.hits, fc.misses);
    }
}
