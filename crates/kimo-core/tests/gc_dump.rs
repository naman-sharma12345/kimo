//! G-c helper: renders the golden scenes and dumps hashes plus raw f32 frames to $GC_OUT (never into the repo). Compiles on both main and v3. Used by tests/gc_main_compat.sh.
use kimo_core::{commands::{dispatch, Session}, render};
use serde_json::{json, Value};
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
#[test] fn gc_dump() {
    let out = match std::env::var("GC_OUT") { Ok(o) => std::path::PathBuf::from(o), Err(_) => { eprintln!("GC_OUT not set: skipping"); return; } }; std::fs::create_dir_all(&out).unwrap();
    pin_fonts();
    let mut lines = vec![];
    for (name, b) in scenes() {
        let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 320, "height": 180, "fps": 25}));
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| b(&mut ss))).is_err() { lines.push(format!("{name} SKIP")); continue; }
        lines.push(format!("{name} proj={:016x}", fnv(serde_json::to_string(&ss.project).unwrap().as_bytes()))); let cache = render::Cache::new();
        for t in TIMES { let f = render::render_frame(&ss.project, t, &cache).expect("render"); let mut raw = Vec::with_capacity(f.d.len() * 4); for v in &f.d { raw.extend_from_slice(&v.to_le_bytes()); }
            std::fs::write(out.join(format!("{name}@{t:.1}.f32")), &raw).unwrap(); lines.push(format!("{name}@{t:.1} f32={:016x} rgba8={:016x}", fnv(&raw), fnv(&render::to_rgba8(&f)))); }
    }
    std::fs::write(out.join("hashes.txt"), lines.join("\n") + "\n").unwrap();
}
