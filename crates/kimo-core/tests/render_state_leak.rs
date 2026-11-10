//! V3-2 step 1: does any thread-local render state leak from one render into the next on the same thread? For each ordered pair of scenes (A, B),
//! render A then B on one thread; B must hash exactly as B rendered first on a fresh thread. Scenes cover the thread_local groups: shard burst with
//! texture + occluder (shards.rs), frosted_glass post (frosted.rs), relight with project lights (lighting.rs), emitter spawning on a layer silhouette
//! (particles.rs), track matte (render.rs MATTE_DEPTH), adjustment layer, and motion blur. Prints the failing pairs; asserts none.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::{json, Value};
fn fnv(f: &kimo_core::effects::Frame) -> u64 { let mut h = 0xcbf29ce484222325u64; for v in &f.d { h ^= v.to_bits() as u64; h = h.wrapping_mul(0x100000001b3); } h }
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
static DIM: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(320);
fn base() -> (Session, Value) { let w = DIM.load(std::sync::atomic::Ordering::SeqCst); let h = w * 9 / 16; let mut s = Session::new(); c(&mut s, "project_set", json!({"width": w, "height": h, "fps": 24})); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.15, 0.25, 0.45], "start": 0, "duration": 2})); (s, t) }
fn shape(s: &mut Session, w: f64) -> (Value, Value) { let t = c(s, "add_track", json!({}))["track_id"].clone(); let cl = c(s, "add_shape", json!({"track_id": t, "shape": "rect", "width": w, "height": 90, "fill": [0.9, 0.6, 0.2, 1], "start": 0, "duration": 2}))["clip_id"].clone(); (t, cl) }
fn scenes(tex: &str) -> Vec<(&'static str, kimo_core::model::Project)> {
    let mut v = vec![];
    { let (mut s, _) = base(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
      c(&mut s, "add_shard_burst", json!({"track_id": t, "start": 0, "duration": 2, "config": {"count": 40, "seed": 3, "env_reflection": 0.6, "birth": "plane_fracture", "plane": {"texture": tex, "region": [120.0, 80.0], "cells": 8}}})); v.push(("shards+texture", s.project.clone())); }
    { let (mut s, _) = base(); let (_, cl) = shape(&mut s, 160.0); c(&mut s, "add_effect", json!({"clip_id": cl, "kind": "frosted_glass"})); v.push(("frosted_glass", s.project.clone())); }
    { let (mut s, _) = base(); let (_, cl) = shape(&mut s, 200.0); c(&mut s, "set_light", json!({"id": "L1", "kind": "point", "pos": [0.3, 0.3, 1.0], "intensity": 300})); c(&mut s, "set_relight", json!({"clip_id": cl, "strings": {"lights": "L1"}})); v.push(("relight", s.project.clone())); }
    { let (mut s, _) = base(); let (_, src) = shape(&mut s, 120.0); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
      c(&mut s, "add_emitter", json!({"track_id": t, "start": 0, "duration": 2, "config": {"shape": "layer", "emit_clip": src, "rate": 200}})); v.push(("emitter on layer", s.project.clone())); }
    { let (mut s, _) = base(); let (_, m) = shape(&mut s, 150.0); let (_, cl) = shape(&mut s, 280.0); c(&mut s, "set_track_matte", json!({"clip_id": cl, "matte_clip_id": m, "mode": "alpha", "hide_source": true})); v.push(("track matte", s.project.clone())); }
    { let (mut s, _) = base(); let (_, _) = shape(&mut s, 100.0); let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); let a = c(&mut s, "add_adjustment_layer", json!({"track_id": t, "start": 0, "duration": 2}))["clip_id"].clone(); c(&mut s, "add_effect", json!({"clip_id": a, "kind": "glow"})); v.push(("adjustment glow", s.project.clone())); }
    { let (mut s, _) = base(); let (_, cl) = shape(&mut s, 100.0); c(&mut s, "set_keyframe", json!({"clip_id": cl, "property": "x", "time": 0.0, "value": -80.0})); c(&mut s, "set_keyframe", json!({"clip_id": cl, "property": "x", "time": 2.0, "value": 80.0})); c(&mut s, "set_motion_blur", json!({"clip_id": cl, "shutter_angle": 360})); v.push(("motion blur", s.project.clone())); }
    v
}
#[test] fn no_thread_local_state_leaks_between_renders() {
    if std::process::Command::new("ffmpeg").arg("-version").output().is_err() { eprintln!("SKIPPED: ffmpeg not found"); return; }
    let tex = std::env::temp_dir().join(format!("kimo-leak-{}.png", std::process::id())); let tex = tex.to_str().unwrap().to_string();
    assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "gradients=s=120x80:c0=0x203040:c1=0xf0e0a0:duration=1:r=1", "-frames:v", "1", &tex]).status().unwrap().success());
    let sc = scenes(&tex); let r = |p: &kimo_core::model::Project| fnv(&render_frame(p, 0.8, &Cache::new()).unwrap());
    let fresh: Vec<u64> = sc.iter().map(|(_, p)| { let p = p.clone(); std::thread::spawn(move || r(&p)).join().unwrap() }).collect();
    let mut bad = vec![];
    for (i, (na, pa)) in sc.iter().enumerate() { for (j, (nb, pb)) in sc.iter().enumerate() { let (pa, pb) = (pa.clone(), pb.clone()); let (fb, ()) = (fresh[j], ());
        let got = std::thread::spawn(move || { let _ = r(&pa); r(&pb) }).join().unwrap(); if got != fb { bad.push(format!("{na} -> {nb}")); } let _ = i; } }
    let _ = std::fs::remove_file(&tex);
    for (n, h) in sc.iter().zip(&fresh) { eprintln!("fresh {} = {:016x}", n.0, h); }
    assert!(bad.is_empty(), "state leaked between renders on one thread (A -> B): {bad:?}");
}

#[test] fn thread_count_does_not_change_pixels() {
    // par.rs workers are fresh threads: any thread-local render state they would need must not change the result. 1 vs 4 workers, 960x540.
    if std::process::Command::new("ffmpeg").arg("-version").output().is_err() { eprintln!("SKIPPED: ffmpeg not found"); return; }
    let tex = std::env::temp_dir().join(format!("kimo-leak2-{}.png", std::process::id())); let tex = tex.to_str().unwrap().to_string();
    assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "gradients=s=120x80:c0=0x203040:c1=0xf0e0a0:duration=1:r=1", "-frames:v", "1", &tex]).status().unwrap().success());
    DIM.store(960, std::sync::atomic::Ordering::SeqCst); let sc = scenes(&tex); let mut bad = vec![];
    for (n, p) in &sc { kimo_core::par::set_threads(1); let a = fnv(&render_frame(p, 0.8, &Cache::new()).unwrap()); kimo_core::par::set_threads(4); let b = fnv(&render_frame(p, 0.8, &Cache::new()).unwrap());
        eprintln!("threads 1 vs 4 {n}: {a:016x} {b:016x}"); if a != b { bad.push(*n); } }
    kimo_core::par::set_threads(0); let _ = std::fs::remove_file(&tex);
    assert!(bad.is_empty(), "output depends on worker count: {bad:?}");
}
