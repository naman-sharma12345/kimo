//! Manual look at bridged EffectCraft effects: `KIMO_EC_PREVIEW=<dir> cargo test --test ec_preview -- --ignored`
//! renders each listed effect on a test picture to <dir>/<kind>.png. Not part of the gate.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::json;

#[test]
#[ignore]
fn preview_effectcraft_effects() {
    let Ok(dir) = std::env::var("KIMO_EC_PREVIEW") else { return };
    let cases = [("card_dance", json!({})), ("shatter", json!({})), ("wave_world", json!({})), ("particle_emitter_2d", json!({})), ("rainfall", json!({})), ("liquify", json!({})),
        ("page_curl", json!({})), ("hexagonal_tiles", json!({})), ("brush_strokes", json!({})), ("glass_relief", json!({})), ("turbulent_noise", json!({})), ("radial_light_burst", json!({}))];
    for (k, params) in cases {
        let mut ss = Session::new(); dispatch(&mut ss, "project_set", &json!({"width": 640, "height": 360, "fps": 30, "render_quality": "current"})).unwrap();
        let t = dispatch(&mut ss, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
        dispatch(&mut ss, "add_shape", &json!({"track_id": t, "shape": "rect", "width": 640, "height": 360, "fill": [0.1, 0.15, 0.3, 1], "start": 0.0, "duration": 3.0})).unwrap();
        let t2 = dispatch(&mut ss, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
        let c = dispatch(&mut ss, "add_title", &json!({"track_id": t2, "text": "kimo", "start": 0.0, "duration": 3.0})).or_else(|_| dispatch(&mut ss, "add_shape", &json!({"track_id": t2, "shape": "ellipse", "width": 300, "height": 200, "fill": [0.95, 0.55, 0.15, 1], "start": 0.0, "duration": 3.0}))).unwrap();
        let id = c["clip_id"].as_str().or_else(|| c["clip_ids"][0].as_str()).unwrap().to_string();
        if let Err(e) = dispatch(&mut ss, "add_effect", &json!({"clip_id": id, "kind": k, "params": params})) { eprintln!("{k}: {e}"); continue; }
        let mut cache = render::Cache::new(); render::render_png(&ss.project, 1.5, &format!("{dir}/{k}.png"), &mut cache).unwrap();
    }
}
