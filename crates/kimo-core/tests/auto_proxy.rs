//! Automatic background proxies (kimo_core::auto_proxy, docs/PROXY_WORKFLOW.md "Automatic proxies"): a heavy import
//! returns at once, the job goes building -> ready with progress events, previews decode the edit proxy, and export
//! decodes the original. Real FFmpeg media throughout.
use kimo_core::{
    auto_proxy,
    commands::{dispatch, Session},
    framecache, render,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, Once,
    },
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
static EVENTS: Mutex<Vec<Value>> = Mutex::new(Vec::new());
static SETUP: Once = Once::new();

struct Dir(PathBuf);
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One process-wide setup: automatic proxies on (as kimo-mcp does), a private edit-proxy cache, and an event sink.
fn setup() -> Dir {
    SETUP.call_once(|| {
        auto_proxy::set_enabled(true);
        let cache = std::env::temp_dir().join(format!("kimo-auto-proxy-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cache);
        auto_proxy::set_cache_dir(Some(cache));
        kimo_core::telemetry::set_event_hook(Box::new(|v| EVENTS.lock().unwrap().push(v.clone())));
    });
    let dir = std::env::temp_dir().join(format!("kimo-auto-proxy-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    Dir(dir)
}

fn call(s: &mut Session, n: &str, a: Value) -> Value {
    dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}"))
}

fn ff(args: &[&str], out: &Path) {
    let o = Command::new("ffmpeg").args(["-v", "error", "-nostdin", "-y"]).args(args).arg(out).output().expect("ffmpeg on PATH");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

fn session(w: u32, h: u32) -> Session {
    let mut s = Session::new();
    call(&mut s, "project_set", json!({"width": w, "height": h, "fps": 10}));
    s
}

fn job(s: &mut Session, id: &str) -> Value {
    let r = call(s, "proxy_jobs", json!({}));
    r["jobs"].as_array().unwrap().iter().find(|j| j["asset_ids"].as_array().unwrap().iter().any(|x| x == id)).cloned().unwrap_or(Value::Null)
}

fn events_for(path: &str) -> Vec<Value> {
    EVENTS.lock().unwrap().iter().filter(|e| e["type"] == "proxy" && e["path"] == path).cloned().collect()
}

fn add_clip(s: &mut Session, id: &str, dur: f64) {
    let tr = call(s, "add_track", json!({}))["track_id"].clone();
    call(s, "add_clip", json!({"track_id": tr, "source_type": "media", "asset_id": id, "start": 0, "duration": dur}));
}

fn is_red(b: &[u8]) {
    assert!(b[0] > 200 && b[2] < 30, "expected red, got {:?}", &b[..4]);
}
fn is_blue(b: &[u8]) {
    assert!(b[2] > 200 && b[0] < 30, "expected blue, got {:?}", &b[..4]);
}

fn probe_size(p: &str) -> (u64, u64) {
    let o = Command::new("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0", p]).output().unwrap();
    let t = String::from_utf8_lossy(&o.stdout);
    let mut it = t.trim().split(',').map(|x| x.parse::<u64>().unwrap());
    (it.next().unwrap(), it.next().unwrap())
}

#[test]
fn heavy_4k_import_returns_at_once_then_preview_uses_the_proxy_and_export_the_original() {
    let d = setup();
    let source = d.0.join("uhd.mp4");
    ff(&["-f", "lavfi", "-i", "color=c=red:s=3840x2160:r=10:d=2", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"], &source);
    let mut s = session(640, 360);

    let t0 = Instant::now();
    let r = call(&mut s, "import_media", json!({"path": source}));
    let import_ms = t0.elapsed().as_millis();
    let id = r["asset_id"].as_str().unwrap().to_string();
    assert_eq!(r["proxy"]["state"], "queued", "import queues the proxy and does not wait for it: {r}");
    assert_eq!(r["proxy"]["reasons"], json!(["4k"]));
    let path = r["proxy"]["path"].as_str().unwrap().to_string();

    assert!(auto_proxy::wait_idle(Duration::from_secs(300)), "proxy build did not finish");
    let build_ms = t0.elapsed().as_millis();
    eprintln!("import returned in {import_ms} ms; proxy ready after {build_ms} ms");
    assert!(import_ms < build_ms, "import must return before the proxy is built");

    let j = job(&mut s, &id);
    assert_eq!(j["state"], "ready", "{j}");
    assert_eq!(j["progress"], 1.0);
    let edit = j["edit_proxy"].as_str().expect("edit proxy path").to_string();
    assert_eq!(probe_size(&edit), (1280, 720), "4K -> 1280x720, exact aspect, never the original size");
    assert!(j["analysis_proxy"].is_null(), "short-GOP camera media needs no analysis proxy: {j}");

    // events: queued -> checking -> building (progress rising) -> ready, then nothing after ready
    let ev = events_for(&path);
    let states: Vec<&str> = ev.iter().map(|e| e["state"].as_str().unwrap()).collect();
    assert_eq!(states.first(), Some(&"queued"), "{states:?}");
    assert!(states.contains(&"checking") && states.contains(&"building"), "{states:?}");
    assert_eq!(states.iter().rev().find(|s| **s != "ready").copied(), Some("building"), "ready is last: {states:?}");
    let prog: Vec<f64> = ev.iter().filter(|e| e["state"] == "building").map(|e| e["progress"].as_f64().unwrap()).collect();
    assert!(prog.windows(2).all(|w| w[1] >= w[0]), "progress never goes backwards: {prog:?}");
    assert_eq!(ev.last().unwrap()["asset_ids"], json!([id]));

    // previews (render_frame PNG and preview_range) decode the proxy; with use_proxies:false they decode the original
    add_clip(&mut s, &id, 1.0);
    let png = d.0.join("f.png");
    assert_eq!(call(&mut s, "render_frame", json!({"time": 0.2, "path": png}))["proxy_assets"], json!([id]));
    let mp4 = d.0.join("draft.mp4");
    assert_eq!(call(&mut s, "preview_range", json!({"start": 0, "end": 0.5, "path": mp4}))["proxy_assets"], json!([id]));
    let mp4b = d.0.join("draft-orig.mp4");
    assert_eq!(call(&mut s, "preview_range", json!({"start": 0, "end": 0.5, "path": mp4b, "use_proxies": false}))["proxy_assets"], json!([]));

    // Make the proxy's pixels distinguishable (blue; same size/frames) and prove export decodes the original (red).
    let blue = d.0.join("blue.mp4");
    ff(&["-f", "lavfi", "-i", "color=c=blue:s=1280x720:r=10:d=2", "-c:v", "libx264", "-g", "1", "-pix_fmt", "yuv420p"], &blue);
    std::fs::copy(&blue, &edit).unwrap();
    let side = Path::new(&edit).with_extension("json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&side).unwrap()).unwrap();
    v["bytes"] = json!(std::fs::metadata(&edit).unwrap().len());
    std::fs::write(&side, v.to_string()).unwrap();

    let p = &s.project;
    let mut cache = render::Cache::new();
    let mut fc = framecache::FrameCache::new(1 << 20);
    render::prepare_preview_range(p, &mut cache, 0.2, 0.2).unwrap();
    assert_eq!(cache.proxy_assets(), vec![id.clone()]);
    is_blue(&render::to_rgba8(&framecache::render_cached(&mut fc, p, 0.2, &cache).unwrap()));
    render::prepare_export_range(p, &mut cache, 0.2, 0.2).unwrap();
    assert!(cache.proxy_assets().is_empty(), "export never selects a proxy");
    is_red(&render::export_frame_rgba8(p, 0.2, &cache).unwrap());
    let out = d.0.join("delivery.mp4");
    render::export_range_opts(p, out.to_str().unwrap(), 18, 0.0, 0.5, false, false).unwrap();
    let o = Command::new("ffmpeg").args(["-v", "error", "-i"]).arg(&out).args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "-"]).output().unwrap();
    assert!(o.status.success());
    is_red(&o.stdout);

    // a changed original disables the proxy at once (previews fall back to the original)
    std::fs::write(&source, b"not the same media any more").unwrap();
    assert_eq!(auto_proxy::preview_path(&s.project, &id), None);
}

#[test]
fn long_gop_phone_footage_gets_an_analysis_proxy() {
    let d = setup();
    let source = d.0.join("phone.mp4");
    // one keyframe for the whole clip, like much phone footage
    ff(&["-f", "lavfi", "-i", "testsrc2=s=320x180:r=30:d=6", "-c:v", "libx264", "-preset", "ultrafast", "-g", "1000", "-keyint_min", "1000", "-sc_threshold", "0", "-pix_fmt", "yuv420p"], &source);
    let src = source.to_str().unwrap();
    let fast = auto_proxy::packet_keyframe_gap(src, 6.0).unwrap();
    let slow = kimo_core::analysis_proxy::max_keyframe_gap(src, 6.0).unwrap();
    assert!((fast - slow).abs() < 0.05 && fast > 5.9, "packet scan {fast} agrees with the decoding scan {slow}");

    let mut s = session(320, 180);
    let r = call(&mut s, "import_media", json!({"path": source}));
    let id = r["asset_id"].as_str().unwrap().to_string();
    assert_eq!(r["proxy"]["state"], "queued");
    assert_eq!(r["proxy"]["reasons"], json!([]), "not heavy by size or length; the keyframe check runs in the background");
    assert!(auto_proxy::wait_idle(Duration::from_secs(300)));
    let j = job(&mut s, &id);
    assert_eq!(j["state"], "ready", "{j}");
    assert_eq!(j["reasons"], json!(["long_gop"]));
    assert!(Path::new(j["analysis_proxy"].as_str().expect("analysis proxy")).is_file());
    assert!(j["edit_proxy"].is_null(), "320x180 is already smaller than an edit proxy: {j}");
    assert!(j["note"].as_str().unwrap().contains("no edit proxy"), "{j}");
    // no edit proxy, so previews decode the original
    add_clip(&mut s, &id, 1.0);
    let png = d.0.join("f.png");
    assert_eq!(call(&mut s, "render_frame", json!({"time": 0.2, "path": png}))["proxy_assets"], json!([]));
}

#[test]
fn light_media_is_skipped_and_reimport_reuses_the_job() {
    let d = setup();
    let source = d.0.join("small.mp4");
    ff(&["-f", "lavfi", "-i", "color=c=red:s=128x72:r=10:d=1", "-c:v", "libx264", "-pix_fmt", "yuv420p"], &source);
    let mut s = session(128, 72);
    let id = call(&mut s, "import_media", json!({"path": source}))["asset_id"].as_str().unwrap().to_string();
    assert!(auto_proxy::wait_idle(Duration::from_secs(120)));
    let j = job(&mut s, &id);
    assert_eq!(j["state"], "skipped", "{j}");
    assert!(j["edit_proxy"].is_null() && j["analysis_proxy"].is_null());
    // the same file again: no second check, the new asset joins the existing job
    let r = call(&mut s, "import_media", json!({"path": source}));
    assert_eq!(r["proxy"]["state"], "skipped");
    let id2 = r["asset_id"].as_str().unwrap().to_string();
    assert_eq!(job(&mut s, &id2)["asset_ids"], json!([id, id2]));
    // stills and audio never queue anything
    let png = d.0.join("still.png");
    ff(&["-f", "lavfi", "-i", "color=c=red:s=64x36", "-frames:v", "1"], &png);
    assert!(call(&mut s, "import_media", json!({"path": png})).get("proxy").is_none());
}
