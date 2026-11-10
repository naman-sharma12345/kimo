//! Code clip render-ahead: sequential playback and export must reach the browser in whole batches, not one launch per frame
//! (each launch costs a browser start and a page load, about a second, against ~0.1 s per drawn frame). Own test binary so the
//! process-wide browser-run counter sees only this test.
use kimo_core::codeclip;
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
struct Scratch(PathBuf);
impl Drop for Scratch { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

#[test]
fn sequential_playback_and_export_render_ahead_in_batches() {
    let sc = Scratch(std::env::temp_dir().join(format!("kimo_ccb_{}", std::process::id()))); let _ = std::fs::remove_dir_all(&sc.0);
    let d = sc.0.join("clip"); std::fs::create_dir_all(&d).unwrap();
    // red channel = 2 x frame number (t * 20 at 10 fps), so every composited frame proves which slot it shows
    std::fs::write(d.join("index.html"), r#"<!doctype html><html><head><style>html,body{margin:0;background:transparent;overflow:hidden}</style></head><body><canvas id="c"></canvas><script>
const c = document.getElementById('c'); c.width = innerWidth; c.height = innerHeight; const g = c.getContext('2d');
window.renderFrame = (t) => { g.fillStyle = 'rgb(' + Math.round(t * 20) + ',0,0)'; g.fillRect(0, 0, c.width, c.height); };
</script></body></html>"#).unwrap();
    let mut s = Session::new(); s.project.base_dir = Some(sc.0.clone());
    c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 10}));
    let tr = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_code_clip", json!({"track_id": tr, "path": d.to_str().unwrap(), "duration": 12}));
    let red = |s: &Session, cache: &kimo_core::render::Cache, t: f64| { let f = kimo_core::render::render_frame(&s.project, t, cache).unwrap(); (f.d[0].clamp(0.0, 1.0) * 255.0).round() as i32 };

    // playback: frame after frame through the 24-frame window
    let mut cache = kimo_core::render::Cache::new(); let r0 = codeclip::browser_runs();
    for n in 0..48 { let t = n as f64 / 10.0; kimo_core::render::prepare_range(&s.project, &mut cache, t, t).unwrap(); let r = red(&s, &cache, t); assert!((r - 2 * n).abs() <= 1, "playback frame {n}: red {r}, want {}", 2 * n); }
    let runs = codeclip::browser_runs() - r0;
    assert!(runs <= 4, "48 sequential playback frames took {runs} browser runs, want at most 4");

    // export: the same from a cold cache (disk cache off), through the export window
    std::env::set_var("KIMO_CODECLIP_CACHE", "off");
    let mut cache = kimo_core::render::Cache::new_export(); let r0 = codeclip::browser_runs();
    for n in 48..96 { let t = n as f64 / 10.0; kimo_core::render::prepare_export_range(&s.project, &mut cache, t, t).unwrap(); let r = red(&s, &cache, t); assert!((r - 2 * n).abs() <= 1, "export frame {n}: red {r}, want {}", 2 * n); }
    let runs = codeclip::browser_runs() - r0;
    std::env::remove_var("KIMO_CODECLIP_CACHE");
    assert!(runs <= 2, "48 sequential export frames took {runs} browser runs, want at most 2");
}
