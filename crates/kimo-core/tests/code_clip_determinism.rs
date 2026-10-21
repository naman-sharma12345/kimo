//! check_code_clip_determinism (codeclip.rs check_determinism): the code clip contract is "renderFrame(t, params) depends only on
//! t and params". The check draws frame t fresh, then earlier frames, then t again, then t on a reloaded page, and compares
//! the bytes. Real headless Chromium through the sidecar; canvas-2D pages that keep or don't keep state, plus the three.js
//! example (WebGL).
use kimo_core::commands::{dispatch, dispatch_ex, Session};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

/// Scratch folder under the temp dir, deleted on drop (panics included).
struct Scratch(PathBuf);
impl Scratch {
    fn new(tag: &str) -> Scratch { let d = std::env::temp_dir().join(format!("kimo_ccd_{tag}_{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap(); Scratch(d) }
    fn page(&self, body: &str) -> PathBuf {
        let d = self.0.join("clip"); std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("index.html"), format!(r#"<!doctype html><html><head><meta charset="utf-8"><style>html,body{{margin:0;background:transparent}}canvas{{display:block}}</style></head>
<body><canvas id="c"></canvas><script>
const c = document.getElementById('c'); c.width = innerWidth; c.height = innerHeight; const g = c.getContext('2d');
{body}
window.__ready = true;
</script></body></html>"#)).unwrap();
        d
    }
}
impl Drop for Scratch { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

fn clip_from(sc: &Scratch, dir: &Path, w: u32, h: u32, fps: u32) -> (Session, String) {
    let mut s = Session::new(); s.project.base_dir = Some(sc.0.clone());
    c(&mut s, "project_set", json!({"width": w, "height": h, "fps": fps}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_code_clip", json!({"track_id": t, "path": dir.to_str().unwrap(), "duration": 4}))["clip_id"].as_str().unwrap().to_string();
    (s, id)
}

#[test]
fn a_pure_function_of_t_passes_and_reports_the_hash() {
    let sc = Scratch::new("pure");
    let d = sc.page("window.renderFrame = (t) => { g.clearRect(0, 0, c.width, c.height); g.fillStyle = 'hsl(' + (t * 90) + ',70%,50%)'; g.fillRect(0, 0, c.width * ((t % 1) + 0.1) / 1.1, c.height); };");
    let (mut s, id) = clip_from(&sc, &d, 64, 36, 10);
    let r = c(&mut s, "check_code_clip_determinism", json!({"clip_id": id, "time": 1.25, "before": 6}));
    assert_eq!(r["deterministic"], true); assert_eq!(r["t"], 1.2); assert_eq!(r["earlier_frames"], 6); assert_eq!(r["frames_rendered"], 9);
    assert_eq!(r["hash"].as_str().unwrap().len(), 16); assert_eq!(r["blank"], false); assert_eq!(r["fresh_page"], true);
}

#[test]
fn state_carried_between_frames_fails_loud() {
    let sc = Scratch::new("state");
    let d = sc.page("let n = 0; window.renderFrame = (t) => { n++; g.clearRect(0, 0, c.width, c.height); g.fillStyle = 'rgb(' + (n * 20 % 256) + ',0,0)'; g.fillRect(0, 0, c.width, c.height); };");
    let (mut s, id) = clip_from(&sc, &d, 64, 36, 10);
    let e = dispatch_ex(&mut s, "check_code_clip_determinism", &json!({"clip_id": id, "time": 2.0, "before": 4})).unwrap_err();
    assert_eq!(e.code, "nondeterministic", "{}", e.message);
    assert!(e.message.contains("after 4 earlier frames") && e.message.contains("t=2 s"), "{}", e.message);
}

#[test]
fn unseeded_randomness_is_caught() {
    // Math.random inside a code clip is a seeded generator restarted on page load, so it is repeatable per page but advances
    // with every call: drawing from it per frame makes frame t depend on what was drawn before, and the check says so.
    let sc = Scratch::new("random");
    let d = sc.page("window.renderFrame = (t) => { g.clearRect(0, 0, c.width, c.height); for (let i = 0; i < 20; i++) { g.fillStyle = 'rgb(255,255,255)'; g.fillRect(Math.random() * c.width, Math.random() * c.height, 3, 3); } };");
    let (mut s, id) = clip_from(&sc, &d, 64, 36, 10);
    let e = dispatch_ex(&mut s, "check_code_clip_determinism", &json!({"clip_id": id, "time": 1.0})).unwrap_err();
    assert_eq!(e.code, "nondeterministic", "{}", e.message);
}

#[test]
fn the_three_js_example_is_deterministic_in_webgl() {
    let ex = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/codeclip_orbit");
    let sc = Scratch::new("orbit");
    let (mut s, id) = clip_from(&sc, &ex, 320, 180, 30);
    c(&mut s, "set_code_clip_param", json!({"clip_id": id, "name": "speed", "value": 2.0}));
    let r = c(&mut s, "check_code_clip_determinism", json!({"clip_id": id, "time": 1.5, "before": 8}));
    assert_eq!(r["deterministic"], true, "{r}"); assert_eq!(r["blank"], false, "WebGL drew pixels");
    assert!(r["gl"].as_str().unwrap_or("").to_lowercase().contains("swiftshader") || !r["gl"].as_str().unwrap_or("").is_empty(), "{r}");
    assert_eq!(r["params"]["speed"], 2.0);
}

#[test]
fn bad_arguments_are_refused_before_any_browser_runs() {
    let sc = Scratch::new("args"); let d = sc.page("window.renderFrame = () => {};");
    let (mut s, id) = clip_from(&sc, &d, 64, 36, 10);
    let e = dispatch_ex(&mut s, "check_code_clip_determinism", &json!({"clip_id": id, "time": 9.0})).unwrap_err(); assert_eq!(e.code, "out_of_range", "{}", e.message);
    let e = dispatch_ex(&mut s, "check_code_clip_determinism", &json!({"clip_id": id, "before": 500})).unwrap_err(); assert_eq!(e.code, "out_of_range", "{}", e.message);
    let t = s.project.tracks[0].id.clone();
    let sh = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "start": 5, "duration": 1}))["clip_id"].clone();
    let e = dispatch_ex(&mut s, "check_code_clip_determinism", &json!({"clip_id": sh})).unwrap_err(); assert!(e.message.contains("is not a code clip"), "{}", e.message);
}
