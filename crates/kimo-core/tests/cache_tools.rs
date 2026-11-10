//! V3-6a: cache_stats / cache_clear / cache_set_budget contracts, plus "cleared => bit-identical" and the render tools going through the cache.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn setup() -> Session { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 320, "height": 180, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "width": 120.0, "height": 60.0, "fill": [0.9, 0.5, 0.2, 1], "start": 0, "duration": 2})); s }
fn png(s: &mut Session, name: &str, t: f64) -> Vec<u8> { let p = std::env::temp_dir().join(format!("kimo_ct_{}_{name}.png", std::process::id())); c(s, "render_frame", json!({"time": t, "path": p.to_str().unwrap()})); let b = std::fs::read(&p).unwrap(); let _ = std::fs::remove_file(&p); b }
#[test] fn stats_shape_and_counting() {
    let mut s = setup(); let st = c(&mut s, "cache_stats", json!({}));
    for k in ["entries", "bytes", "cap_bytes", "hits", "misses", "evictions", "hit_rate"] { assert!(st.get(k).is_some(), "cache_stats missing {k}: {st}"); }
    assert_eq!(st["entries"], 0); png(&mut s, "a", 0.5); let a = c(&mut s, "cache_stats", json!({})); assert_eq!((a["entries"].as_u64(), a["misses"].as_u64(), a["hits"].as_u64()), (Some(1), Some(1), Some(0)));
    png(&mut s, "b", 0.5); let b = c(&mut s, "cache_stats", json!({})); assert_eq!(b["hits"], 1, "second render of the same frame must hit: {b}"); assert!(b["hit_rate"].as_f64().unwrap() > 0.4);
}
#[test] fn cleared_cache_gives_bit_identical_frames() {
    let mut s = setup(); let cold = png(&mut s, "c", 0.5); let warm = png(&mut s, "w", 0.5); assert_eq!(cold, warm, "warm png differs");
    let r = c(&mut s, "cache_clear", json!({})); assert_eq!(r["cleared_entries"], 1); assert!(r["freed_bytes"].as_u64().unwrap() > 0); assert_eq!(c(&mut s, "cache_stats", json!({}))["entries"], 0);
    let after = png(&mut s, "x", 0.5); assert_eq!(cold, after, "png after cache_clear differs");
    c(&mut s, "cache_set_budget", json!({"megabytes": 0})); let off = png(&mut s, "o", 0.5); assert_eq!(cold, off, "png with the cache off differs"); assert_eq!(c(&mut s, "cache_stats", json!({}))["entries"], 0);
}
#[test] fn budget_errors_and_clamp() {
    let mut s = setup(); let e = dispatch(&mut s, "cache_set_budget", &json!({})).unwrap_err(); assert!(e.contains("megabytes"), "{e}");
    let e = dispatch(&mut s, "cache_set_budget", &json!({"megabytes": -3.0})).unwrap_err(); assert_eq!(e, "megabytes must be >= 0, got -3");
    let old = kimo_core::guard::BUDGET.load(std::sync::atomic::Ordering::Relaxed); kimo_core::guard::BUDGET.store(64 << 20, std::sync::atomic::Ordering::Relaxed);
    let e = dispatch(&mut s, "cache_set_budget", &json!({"megabytes": 100.0})).unwrap_err(); assert_eq!(e, "cache budget 100 MB is above the memory budget of 64 MB (raise KIMO_MEM_MB or choose a smaller cache)");
    let ok = c(&mut s, "cache_set_budget", json!({"megabytes": 64.0})); assert_eq!(ok["cap_bytes"], 64u64 << 20);
    kimo_core::guard::BUDGET.store(old, std::sync::atomic::Ordering::Relaxed);
}
