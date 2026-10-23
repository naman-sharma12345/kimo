//! Golden-hash regression harness (architecture unit 1). Builds a table of scenes through the public tool surface, renders each at 3 times at
//! 320x180 and hashes the f32 frame buffer and the 8-bit RGBA bytes (FNV-1a 64). Also hashes the tool registry (name, description, schema, mutates)
//! and each scene's project JSON. Baselines live in tests/golden/ and are compared on every run. `KIMO_UPDATE_GOLDEN=1` rewrites them.
//! Determinism guard: every scene is rendered twice with a fresh Session and Cache, at 1 worker thread and at the default count; all must match,
//! otherwise the scene is nondeterministic, which is a finding and never a baseline.
use kimo_core::{commands::{dispatch, registry_ref, Session}, render};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn pin_fonts() { static O: std::sync::Once = std::sync::Once::new(); O.call_once(|| { std::env::set_var("KIMO_FONTS", std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts")); let r = kimo_core::text::resolve_font("Montserrat"); assert!(r.replace('\\', "/").ends_with("tests/fonts/Montserrat-Var.ttf"), "text goldens need the repo-owned font tests/fonts/Montserrat-Var.ttf but resolve_font(\"Montserrat\") returned {r:?} (silent system-font fallback is not allowed in tests)"); }); }

fn fnv(bytes: &[u8]) -> u64 { let mut h = 0xcbf29ce484222325u64; for b in bytes { h ^= *b as u64; h = h.wrapping_mul(0x100000001b3); } h }
fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("scene setup: {name} failed: {e}")) }
fn solid(ss: &mut Session, col: [f64; 3]) -> String { let t = call(ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); call(ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": col, "start": 0, "duration": 1})); t }
fn track(ss: &mut Session) -> String { call(ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string() }
fn shape(ss: &mut Session, t: &str, kind: &str, w: u32, h: u32, fill: [f64; 4]) -> String { call(ss, "add_shape", json!({"track_id": t, "shape": kind, "width": w, "height": h, "fill": fill, "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string() }
fn fx(ss: &mut Session, clip: &str, kind: &str, ps: &[(&str, f64)]) { let e = call(ss, "add_effect", json!({"clip_id": clip, "kind": kind}))["effect_id"].as_str().unwrap().to_string(); for (k, v) in ps { call(ss, "set_effect_param", json!({"clip_id": clip, "effect_id": e, "param": k, "value": v})); } }

type Build = fn(&mut Session);
fn scenes() -> Vec<(&'static str, Build)> { vec![
    ("shapes", |ss| { solid(ss, [0.1, 0.12, 0.2]); let t = track(ss); shape(ss, &t, "rect", 120, 70, [0.9, 0.3, 0.2, 1.0]); let t2 = track(ss); shape(ss, &t2, "ellipse", 90, 90, [0.2, 0.8, 0.4, 0.8]); }),
    ("text", |ss| { solid(ss, [0.05, 0.05, 0.08]); let t = track(ss); call(ss, "add_text", json!({"track_id": t, "text": "KIMO", "start": 0, "duration": 1, "size": 60, "color": [0.95, 0.95, 1.0]})); }),
    ("fx_stack", |ss| { solid(ss, [0.2, 0.25, 0.4]); let t = track(ss); let c = shape(ss, &t, "rect", 200, 100, [0.95, 0.8, 0.5, 1.0]); fx(ss, &c, "exposure", &[("stops", 0.4)]); fx(ss, &c, "glow", &[("amount", 0.8), ("radius", 10.0), ("threshold", 0.5)]); fx(ss, &c, "blur", &[("radius", 3.0)]); fx(ss, &c, "vignette", &[]); fx(ss, &c, "teal_orange", &[]); fx(ss, &c, "bloom", &[("threshold", 0.5), ("amount", 0.7)]); }),
    ("color", |ss| { solid(ss, [0.4, 0.3, 0.25]); let t = track(ss); let c = shape(ss, &t, "rect", 280, 150, [0.7, 0.55, 0.4, 1.0]); fx(ss, &c, "primaries", &[("lift_r", 0.02), ("gain_b", 1.1), ("saturation", 1.2)]); fx(ss, &c, "film_print", &[]); }),
    ("masks", |ss| { solid(ss, [0.1, 0.1, 0.1]); let t = track(ss); let c = shape(ss, &t, "rect", 300, 160, [0.9, 0.9, 0.2, 1.0]); call(ss, "set_mask", json!({"clip_id": c, "shape": "ellipse", "cx": 0.5, "cy": 0.5, "w": 0.5, "h": 0.6, "feather": 0.2})); }),
    ("particles", |ss| { solid(ss, [0.02, 0.02, 0.05]); let t = track(ss); call(ss, "add_particles", json!({"track_id": t, "kind": "sparks", "start": 0, "duration": 1, "count": 120, "seed": 5})); }),
    ("scene3d", |ss| { solid(ss, [0.05, 0.06, 0.1]); let t = track(ss); call(ss, "add_scene3d", json!({"track_id": t, "start": 0, "duration": 1, "config": {"objects": [{"kind": "sphere", "size": [70, 70, 70], "pos": [-50, 0, 0], "color": [0.8, 0.3, 0.2]}, {"kind": "box", "size": [60, 60, 60], "pos": [60, 0, 0], "color": [0.2, 0.5, 0.9]}], "lights": [{"kind": "point", "pos": [0, -150, 200], "color": [1, 1, 1], "intensity": 1.2}]}})); }),
    ("precomp", |ss| { solid(ss, [0.15, 0.1, 0.2]); let t = track(ss); let a = shape(ss, &t, "rect", 100, 60, [1.0, 0.5, 0.1, 1.0]); let t2 = track(ss); let b = shape(ss, &t2, "ellipse", 70, 70, [0.1, 0.6, 1.0, 1.0]); call(ss, "group_clips", json!({"clip_ids": [a, b]})); }),
    ("shards", |ss| { solid(ss, [0.5, 0.5, 0.55]); let t = track(ss); call(ss, "add_shard_burst", json!({"track_id": t, "start": 0, "duration": 1, "config": {"count": 40, "size_min": 10, "size_max": 40, "origin": [160, 90, 0], "speed": 120, "dof": 0, "seed": 3, "material": "glass", "refract": 30, "shutter_deg": 0}})); }),
    ("glass_post", |ss| { solid(ss, [0.3, 0.35, 0.5]); let t = track(ss); let c = shape(ss, &t, "rect", 140, 90, [1.0, 1.0, 1.0, 1.0]); fx(ss, &c, "frosted_glass", &[("blur", 8.0), ("slab_thickness", 80.0), ("fresnel", 1.0), ("reflect_scene", 1.0)]); fx(ss, &c, "light_wrap", &[]); fx(ss, &c, "cast_shadow", &[("use_scene_light", 0.0), ("height", 0.1)]); call(ss, "set_property", json!({"clip_id": c, "property": "rot_y", "value": 25})); }),
] }

const TIMES: [f64; 3] = [0.1, 0.5, 0.9];
fn build(b: Build) -> Session { let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 320, "height": 180, "fps": 25})); b(&mut ss); ss }
/// Hashes for one scene: (project json hash, [(f32 hash, rgba8 hash) per time]).
fn run_scene(b: Build) -> (u64, Vec<(u64, u64)>) {
    let ss = build(b); assert!(ss.project.dangling_refs().is_empty(), "golden scene has dangling clip refs: {:?}", ss.project.dangling_refs()); let pj = fnv(serde_json::to_string(&ss.project).unwrap().as_bytes()); let cache = render::Cache::new();
    let fr = TIMES.iter().map(|t| { let f = render::render_frame(&ss.project, *t, &cache).expect("render"); let mut raw = Vec::with_capacity(f.d.len() * 4); for v in &f.d { raw.extend_from_slice(&v.to_le_bytes()); } (fnv(&raw), fnv(&render::to_rgba8(&f))) }).collect(); (pj, fr) }

fn registry_lines() -> Vec<String> { let mut v: Vec<String> = registry_ref().iter().map(|c| format!("{} {:016x}", c.name, fnv(format!("{}|{}|{}", c.description, serde_json::to_string(&c.schema).unwrap(), c.mutates).as_bytes()))).collect(); v.sort(); v }
fn dir() -> std::path::PathBuf { std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden") }
fn compare(file: &str, got: &[String], update: bool, fails: &mut Vec<String>) {
    let p = dir().join(file); let text = got.join("\n") + "\n";
    if update { std::fs::create_dir_all(dir()).unwrap(); std::fs::write(&p, &text).unwrap(); return; }
    let want = std::fs::read_to_string(&p).unwrap_or_else(|_| panic!("missing baseline {file}; run with KIMO_UPDATE_GOLDEN=1"));
    let w: BTreeMap<&str, &str> = want.lines().filter_map(|l| l.split_once(' ')).collect(); let g: BTreeMap<&str, &str> = text.lines().filter_map(|l| l.split_once(' ')).collect();
    for (k, v) in &g { match w.get(k) { None => fails.push(format!("{file}: new entry {k}")), Some(x) if x != v => fails.push(format!("{file}: {k} changed: baseline {x}, now {v}")), _ => {} } }
    for k in w.keys() { if !g.contains_key(k) { fails.push(format!("{file}: entry {k} missing now")); } }
}

/// Allows intentional tool-contract updates without rewriting any frame/project goldens.
#[test] fn golden_registry_contract() {
    let update = std::env::var("KIMO_UPDATE_GOLDEN").map(|v| v == "1").unwrap_or(false);
    let mut fails = vec![];
    compare("golden_registry.txt", &registry_lines(), update, &mut fails);
    assert!(fails.is_empty(), "registry mismatch:\n{}", fails.join("\n"));
}

#[test] fn golden_frames_registry_and_projects() {
    pin_fonts(); let t0 = std::time::Instant::now(); let update = std::env::var("KIMO_UPDATE_GOLDEN").map(|v| v == "1").unwrap_or(false);
    let (mut frames, mut projects, mut nondet) = (vec![], vec![], vec![]); let mut nframes = 0;
    for (name, b) in scenes() {
        kimo_core::par::set_threads(1); let a1 = run_scene(b); let a2 = run_scene(b); kimo_core::par::set_threads(0); let d1 = run_scene(b);
        if a1 != a2 { nondet.push(format!("{name}: two single-thread renders differ")); } if a1 != d1 { nondet.push(format!("{name}: 1 thread vs default threads differ")); }
        projects.push(format!("{name} {:016x}", a1.0));
        for (i, (f, r)) in a1.1.iter().enumerate() { frames.push(format!("{name}@{:.1} f32={:016x} rgba8={:016x}", TIMES[i], f, r).replacen(' ', "_", 0)); nframes += 1; }
    }
    kimo_core::par::set_threads(0);
    assert!(nondet.is_empty(), "NONDETERMINISTIC scenes (a finding, not a baseline): {nondet:?}");
    // baseline lines are "key value": the key is scene@t, the value the two hashes
    let frames: Vec<String> = frames.iter().map(|l| { let (k, v) = l.split_once(' ').unwrap(); format!("{k} {v}") }).collect();
    let mut fails = vec![]; compare("golden_frames.txt", &frames, update, &mut fails); compare("golden_registry.txt", &registry_lines(), update, &mut fails); compare("golden_project.txt", &projects, update, &mut fails);
    eprintln!("golden: {} scenes, {} frames, {} tools, {:.1}s{}", scenes().len(), nframes, registry_ref().len(), t0.elapsed().as_secs_f64(), if update { " (baselines rewritten)" } else { "" });
    assert!(fails.is_empty(), "golden mismatch:\n{}", fails.join("\n"));
}

/// V3-6a: for every golden scene and time, the frame cache must return exactly the uncached pixels: cold (miss), warm (hit), and after the entry was evicted by a cap of zero and refilled.
#[test] fn golden_scenes_cold_warm_cleared_equal_uncached() {
    pin_fonts();
    let mut n = 0;
    for (name, b) in scenes() {
        let ss = build(b); let cache = render::Cache::new(); let mut fc = kimo_core::framecache::FrameCache::new(256 << 20);
        for t in TIMES { let truth = render::render_frame(&ss.project, t, &cache).expect("render");
            let cold = kimo_core::framecache::render_cached(&mut fc, &ss.project, t, &cache).unwrap(); assert!(cold.d == truth.d, "{name}@{t}: cold cached frame differs");
            let h = fc.hits; let warm = kimo_core::framecache::render_cached(&mut fc, &ss.project, t, &cache).unwrap(); assert_eq!(fc.hits, h + 1, "{name}@{t}: second render was not a hit"); assert!(warm.d == truth.d, "{name}@{t}: warm cached frame differs");
            fc.set_cap(0); fc.set_cap(256 << 20); let again = kimo_core::framecache::render_cached(&mut fc, &ss.project, t, &cache).unwrap(); assert!(again.d == truth.d, "{name}@{t}: refill after clear differs"); n += 1; }
    }
    eprintln!("golden cache equality: {n} frames cold/warm/refilled bit-identical");
}
