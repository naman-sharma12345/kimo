//! V3-6a: whole-frame cache. Every input that decides the pixels must change the key; warm == cold == uncached; edit then revert hits again; the cap holds.
use kimo_core::commands::{dispatch, Session};
use kimo_core::framecache::{key, render_cached, FrameCache};
use kimo_core::render::{render_frame, Cache};
use serde_json::{json, Value};
fn fnv(f: &kimo_core::effects::Frame) -> u64 { let mut h = 0xcbf29ce484222325u64; for v in &f.d { h ^= v.to_bits() as u64; h = h.wrapping_mul(0x100000001b3); } h }
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn scene() -> (Session, Value) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 320, "height": 180, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let cl = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "width": 120.0, "height": 60.0, "fill": [0.9, 0.5, 0.2, 1], "start": 0, "duration": 2}))["clip_id"].clone(); (s, cl) }
#[test] fn each_input_changes_the_key() {
    let (mut s, cl) = scene(); let base = key(&s.project, 0.5, false);
    assert_eq!(base, key(&s.project, 0.5, false), "key must be stable");
    assert_ne!(base, key(&s.project, 0.6, false), "time");
    assert_ne!(base, key(&s.project, 0.5, true), "transparent flag");
    let mut p2 = s.project.clone(); p2.width = 640; assert_ne!(base, key(&p2, 0.5, false), "resolution");
    c(&mut s, "set_property", json!({"clip_id": cl, "property": "opacity", "value": 0.5})); let k1 = key(&s.project, 0.5, false); assert_ne!(base, k1, "clip param");
    c(&mut s, "add_effect", json!({"clip_id": cl, "kind": "glow"})); let k2 = key(&s.project, 0.5, false); assert_ne!(k1, k2, "effect added");
    // a file named in the project: touching it changes the key
    let dir = std::env::temp_dir().join(format!("kimo_fc_{}", std::process::id())); std::fs::create_dir_all(&dir).unwrap(); let f = dir.join("lut.cube"); std::fs::write(&f, "a").unwrap();
    let mut p3 = s.project.clone(); p3.name = f.to_string_lossy().into_owned(); let ka = key(&p3, 0.5, false);
    std::fs::write(&f, "abcd").unwrap(); let kb = key(&p3, 0.5, false); assert_ne!(ka, kb, "file size");
    std::fs::remove_file(&f).unwrap(); let kc = key(&p3, 0.5, false); assert_ne!(kb, kc, "file removed"); let _ = std::fs::remove_dir(&dir);
}
#[test] fn cold_warm_and_after_edit_match_uncached() {
    let (mut s, cl) = scene(); let mut fc = FrameCache::new(64 << 20); let cache = Cache::new();
    let truth = |s: &Session, t: f64| fnv(&render_frame(&s.project, t, &Cache::new()).unwrap());
    for t in [0.0, 0.5, 1.0, 1.5] { let cold = fnv(&render_cached(&mut fc, &s.project, t, &cache).unwrap()); assert_eq!(cold, truth(&s, t), "cold t={t}"); let warm = fnv(&render_cached(&mut fc, &s.project, t, &cache).unwrap()); assert_eq!(warm, cold, "warm t={t}"); }
    assert_eq!(fc.hits, 4); assert_eq!(fc.misses, 4);
    let before = truth(&s, 0.5);
    c(&mut s, "set_property", json!({"clip_id": cl, "property": "x", "value": 40.0}));
    let edited = fnv(&render_cached(&mut fc, &s.project, 0.5, &cache).unwrap()); assert_eq!(edited, truth(&s, 0.5), "after edit"); assert_ne!(edited, before, "edit must change pixels");
    c(&mut s, "set_property", json!({"clip_id": cl, "property": "x", "value": 0.0}));
    let h0 = fc.hits; let back = fnv(&render_cached(&mut fc, &s.project, 0.5, &cache).unwrap()); assert_eq!(back, before, "edit reverted"); assert_eq!(fc.hits, h0 + 1, "revert should hit the original entry");
}
#[test] fn cap_is_never_exceeded_and_lru_evicts_oldest() {
    let (s, _) = scene(); let one = 320 * 180 * 16; let mut fc = FrameCache::new(one * 3 + 10); let cache = Cache::new();
    for i in 0..40 { let t = i as f64 * 0.04; render_cached(&mut fc, &s.project, t, &cache).unwrap(); assert!(fc.bytes() <= fc.cap, "bytes {} > cap {}", fc.bytes(), fc.cap); assert!(fc.len() <= 3); }
    assert!(fc.evictions >= 37); let h = fc.hits; render_cached(&mut fc, &s.project, 39.0 * 0.04, &cache).unwrap(); assert_eq!(fc.hits, h + 1, "most recent must still be cached");
    let h = fc.hits; render_cached(&mut fc, &s.project, 0.0, &cache).unwrap(); assert_eq!(fc.hits, h, "oldest must have been evicted");
    fc.set_cap(0); assert_eq!(fc.len(), 0); assert_eq!(fc.bytes(), 0);
}
