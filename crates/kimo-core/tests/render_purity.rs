//! Unit 6a A: two renders with different color pipelines in one process must not see each other's state.
//! Scene M (scene-linear pipeline on) and scene U (off) each draw a plane-fracture shard burst with an sRGB texture.
//! Sequential hashes are recorded, then M and U are rendered concurrently from 4 threads, interleaved; every result must equal its sequential hash.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::json;
fn fnv(f: &kimo_core::effects::Frame) -> u64 { let mut h = 0xcbf29ce484222325u64; for v in &f.d { h ^= v.to_bits() as u64; h = h.wrapping_mul(0x100000001b3); } h }
fn scene(tex: &str, linear: bool) -> kimo_core::model::Project {
    let mut s = Session::new(); let c = |s: &mut Session, n: &str, a: serde_json::Value| dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}"));
    c(&mut s, "project_set", json!({"width": 320, "height": 180, "fps": 24}));
    if linear { c(&mut s, "set_color_pipeline", json!({"settings": {"enabled": true, "linear_resample": true}})); }
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.3, 0.5], "start": 0, "duration": 2}));
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_shard_burst", json!({"track_id": t2, "start": 0, "duration": 2, "config": {"count": 60, "seed": 3, "env_reflection": 0.6, "birth": "plane_fracture", "plane": {"texture": tex, "region": [120.0, 80.0], "cells": 12}}}));
    s.project.clone()
}
#[test] fn pipeline_flag_is_not_shared_between_concurrent_renders() {
    if std::process::Command::new("ffmpeg").arg("-version").output().is_err() { eprintln!("SKIPPED render_purity: ffmpeg not found (load_tex needs it)"); return; }
    let tex = std::env::temp_dir().join(format!("kimo-purity-{}.png", std::process::id())); let tex = tex.to_str().unwrap().to_string();
    assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "gradients=s=120x80:c0=0x203040:c1=0xf0e0a0:duration=1:r=1", "-frames:v", "1", &tex]).status().unwrap().success());
    let (m, u) = (scene(&tex, true), scene(&tex, false));
    let r = |p: &kimo_core::model::Project| fnv(&render_frame(p, 0.8, &Cache::new()).unwrap());
    let (hm, hu) = (r(&m), r(&u)); assert_ne!(hm, hu, "scenes M and U must render differently or the test proves nothing");
    let bad = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|sc| { for k in 0..4 { let (m, u, bad) = (&m, &u, &bad); sc.spawn(move || { for i in 0..100 { let (p, want) = if (i + k) % 2 == 0 { (m, hm) } else { (u, hu) }; if r_on(p) != want { bad.fetch_add(1, std::sync::atomic::Ordering::Relaxed); } } }); } });
    let _ = std::fs::remove_file(&tex);
    assert_eq!(bad.load(std::sync::atomic::Ordering::Relaxed), 0, "concurrent renders saw each other's WORK_LINEAR");
}
fn r_on(p: &kimo_core::model::Project) -> u64 { fnv(&render_frame(p, 0.8, &Cache::new()).unwrap()) }
