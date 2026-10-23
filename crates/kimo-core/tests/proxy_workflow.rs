use kimo_core::{
    commands::{dispatch, Session},
    framecache, proxy, render,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    s: Session,
    id: String,
    dir: PathBuf,
    source: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn call(s: &mut Session, n: &str, a: Value) -> Value {
    dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}"))
}
fn ff(args: &[&str], out: &Path) {
    let o = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-y", "-filter_threads", "1"])
        .args(args)
        .args(["-threads", "1"])
        .arg(out)
        .output()
        .expect("ffmpeg on PATH");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}
fn fixture() -> Fixture {
    let dir = std::env::temp_dir().join(format!(
        "kimo-proxy-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    let source = dir.join("original.mp4");
    ff(
        &[
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=128x72:r=10:d=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ],
        &source,
    );
    let mut s = Session::new();
    call(
        &mut s,
        "project_set",
        json!({"width":64,"height":36,"fps":10}),
    );
    let id = call(&mut s, "import_media", json!({"path":source}))["asset_id"]
        .as_str()
        .unwrap()
        .to_string();
    let tr = call(&mut s, "add_track", json!({}))["track_id"].clone();
    call(
        &mut s,
        "add_clip",
        json!({"track_id":tr,"source_type":"media","asset_id":id,"start":0,"duration":1}),
    );
    Fixture { s, id, dir, source }
}
fn generate(f: &mut Fixture) -> PathBuf {
    let path = f.dir.join("proxy.mp4");
    let r = call(
        &mut f.s,
        "proxy_generate",
        json!({"asset_id":f.id,"path":path,"max_side":64}),
    );
    assert_eq!(r["proxy"]["status"], "ready");
    path
}
fn enable(f: &mut Fixture) {
    call(&mut f.s, "proxy_set_enabled", json!({"enabled":true}));
}
fn state(f: &mut Fixture, deep: bool) -> Value {
    call(
        &mut f.s,
        "proxy_validate",
        json!({"asset_id":f.id,"deep":deep}),
    )["proxies"][0]
        .clone()
}
fn blue_proxy(f: &mut Fixture) {
    let blue = f.dir.join("blue.mp4");
    ff(
        &[
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=64x36:r=10:d=1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &blue,
    );
    let q = f.s.project.proxies.entries.get_mut(&f.id).unwrap();
    q.path = blue.to_string_lossy().into_owned();
    q.proxy = proxy::fingerprint(&blue).unwrap();
}
fn is_red(bytes: &[u8]) {
    assert!(bytes[0] > 200 && bytes[2] < 30, "{:?}", &bytes[..4]);
}
fn is_blue(bytes: &[u8]) {
    assert!(bytes[2] > 200 && bytes[0] < 30, "{:?}", &bytes[..4]);
}

#[test]
fn generation_lifecycle_is_undoable_and_never_rewrites_originals() {
    let mut f = fixture();
    let before = serde_json::to_value(&f.s.project).unwrap();
    let original = std::fs::read(&f.source).unwrap();
    let path = generate(&mut f);
    let after = serde_json::to_value(&f.s.project).unwrap();
    let q = &f.s.project.proxies.entries[&f.id];
    assert_eq!((q.width, q.height, q.frames), (64, 36, 10));
    assert!(!f.s.project.proxies.enabled);
    call(&mut f.s, "undo", json!({}));
    assert_eq!(serde_json::to_value(&f.s.project).unwrap(), before);
    assert!(path.is_file());
    call(&mut f.s, "redo", json!({}));
    assert_eq!(serde_json::to_value(&f.s.project).unwrap(), after);
    enable(&mut f);
    call(&mut f.s, "proxy_remove", json!({"asset_id":f.id}));
    assert!(f.s.project.proxies.entries.is_empty());
    assert!(path.is_file());
    call(&mut f.s, "undo", json!({}));
    assert_eq!(state(&mut f, false)["status"], "ready");
    assert_eq!(std::fs::read(&f.source).unwrap(), original);
}
#[test]
fn preview_cache_and_export_keep_distinct_media() {
    let mut f = fixture();
    generate(&mut f);
    blue_proxy(&mut f);
    enable(&mut f);
    let p = &f.s.project;
    let mut cache = render::Cache::new();
    let mut fc = framecache::FrameCache::new(1 << 20);
    render::prepare_at(p, &mut cache, Some(0.2)).unwrap();
    is_red(&render::to_rgba8(
        &framecache::render_cached(&mut fc, p, 0.2, &cache).unwrap(),
    ));
    render::prepare_preview_range(p, &mut cache, 0.2, 0.2).unwrap();
    assert_eq!(cache.proxy_assets(), vec![f.id.clone()]);
    is_blue(&render::to_rgba8(
        &framecache::render_cached(&mut fc, p, 0.2, &cache).unwrap(),
    ));
    render::prepare_export_range(p, &mut cache, 0.2, 0.2).unwrap();
    assert!(cache.proxy_assets().is_empty());
    is_red(&render::export_frame_rgba8(p, 0.2, &cache).unwrap());
    let export = f.dir.join("delivery.mp4");
    render::export_range_opts(p, export.to_str().unwrap(), 18, 0., 0.3, false, false).unwrap();
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(export)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .output()
        .unwrap();
    assert!(out.status.success());
    is_red(&out.stdout);
    let png = f.dir.join("preview.png");
    let r = call(&mut f.s, "render_frame", json!({"time":0.2,"path":png}));
    assert_eq!(r["proxy_assets"], json!([f.id]));
    assert!(png.is_file());
}
#[test]
fn range_preview_can_force_originals_and_audio_is_identical() {
    let mut f = fixture();
    generate(&mut f);
    let audio = kimo_core::audio::mix(&f.s.project);
    blue_proxy(&mut f);
    enable(&mut f);
    assert_eq!(audio, kimo_core::audio::mix(&f.s.project));
    let a = call(
        &mut f.s,
        "preview_range",
        json!({"start":0,"end":0.3,"scale":1,"hashes":true,"path":f.dir.join("draft.mp4")}),
    );
    assert_eq!(a["proxy_assets"], json!([f.id]));
    let b = call(
        &mut f.s,
        "preview_range",
        json!({"start":0,"end":0.3,"scale":1,"hashes":true,"use_proxies":false,"path":f.dir.join("reference.mp4")}),
    );
    assert_eq!(b["proxy_assets"], json!([]));
    assert_ne!(a["frame_hashes"], b["frame_hashes"]);
}
#[test]
fn missing_and_stale_proxies_fall_back_to_originals() {
    let mut f = fixture();
    let path = generate(&mut f);
    enable(&mut f);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(state(&mut f, false)["status"], "proxy_missing");
    render::prepare_preview_range(&f.s.project, &mut f.s.cache, 0.2, 0.2).unwrap();
    is_red(&render::export_frame_rgba8(&f.s.project, 0.2, &f.s.cache).unwrap());
    std::fs::write(&path, b"changed").unwrap();
    assert_eq!(state(&mut f, false)["status"], "proxy_changed");
    render::prepare_preview_range(&f.s.project, &mut f.s.cache, 0.2, 0.2).unwrap();
    assert!(f.s.cache.proxy_assets().is_empty());
}
#[test]
fn source_changes_disable_proxy_and_deep_validation_detects_content() {
    use std::io::{Seek, SeekFrom, Write};
    let mut f = fixture();
    generate(&mut f);
    enable(&mut f);
    let old = std::fs::metadata(&f.source).unwrap().modified().unwrap();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(&f.source)
        .unwrap();
    file.seek(SeekFrom::Start(100)).unwrap();
    file.write_all(b"UNTRUSTED").unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(old))
        .unwrap();
    drop(file);
    assert_eq!(state(&mut f, false)["status"], "ready");
    assert_eq!(state(&mut f, true)["status"], "source_changed");
    std::fs::OpenOptions::new()
        .append(true)
        .open(&f.source)
        .unwrap()
        .write_all(b"changed")
        .unwrap();
    assert_eq!(state(&mut f, false)["status"], "source_changed");
    assert!(proxy::preview_path(&f.s.project, &f.id).is_none());
}
#[test]
fn relink_checks_both_hashes_and_refreshes_copy_timestamps() {
    let mut f = fixture();
    let path = generate(&mut f);
    let moved = f.dir.join("moved.mp4");
    std::fs::copy(&path, &moved).unwrap();
    std::fs::remove_file(path).unwrap();
    let r = call(
        &mut f.s,
        "proxy_relink",
        json!({"asset_id":f.id,"path":moved}),
    );
    assert_eq!(r["proxy"]["status"], "ready");
    assert_eq!(state(&mut f, true)["status"], "ready");
    let before = serde_json::to_value(&f.s.project).unwrap();
    let bad = f.dir.join("bad.mp4");
    std::fs::write(&bad, b"no").unwrap();
    assert!(dispatch(
        &mut f.s,
        "proxy_relink",
        &json!({"asset_id":f.id,"path":bad})
    )
    .unwrap_err()
    .contains("byte-identical"));
    assert_eq!(serde_json::to_value(&f.s.project).unwrap(), before);
    call(&mut f.s, "undo", json!({}));
    assert_eq!(state(&mut f, false)["status"], "proxy_missing");
}
#[test]
fn persistence_and_external_reference_walker_include_proxies() {
    let mut f = fixture();
    generate(&mut f);
    enable(&mut f);
    let path = f.dir.join("edit.kimo");
    call(&mut f.s, "save_project", json!({"path":path}));
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("proxies"));
    let mut loaded = Session::new();
    call(&mut loaded, "load_project", json!({"path":path}));
    assert!(loaded.project.proxies.enabled);
    assert_eq!(
        proxy::status(&loaded.project, &f.id, true)["status"],
        "ready"
    );
    let field = format!("proxies[{}].path", f.id);
    assert!(loaded
        .project
        .external_refs()
        .iter()
        .any(|r| r.field == field));
    loaded.project.set_ref(&field, "relative.mp4").unwrap();
    assert_eq!(loaded.project.proxies.entries[&f.id].path, "relative.mp4");
    assert!(serde_json::to_value(kimo_core::model::Project::default())
        .unwrap()
        .get("proxies")
        .is_none());
}
#[test]
fn invalid_bounds_and_overwrite_leave_project_and_files_intact() {
    let mut f = fixture();
    let before = serde_json::to_value(&f.s.project).unwrap();
    let existing = std::fs::read(&f.source).unwrap();
    for a in [
        json!({"asset_id":f.id,"path":f.source}),
        json!({"asset_id":f.id,"path":f.dir.join("new.mp4"),"max_side":0}),
        json!({"asset_id":f.id,"path":f.dir.join("new.mp4"),"max_side":64.5}),
        json!({"asset_id":f.id,"path":f.dir.join("new.mp4"),"timeout_seconds":0}),
    ] {
        assert!(dispatch(&mut f.s, "proxy_generate", &a).is_err());
    }
    assert_eq!(std::fs::read(&f.source).unwrap(), existing);
    assert_eq!(serde_json::to_value(&f.s.project).unwrap(), before);
    assert_eq!(std::fs::read_dir(&f.dir).unwrap().count(), 1);
    assert!(dispatch(&mut f.s, "proxy_set_enabled", &json!({})).is_err());
    assert!(dispatch(&mut f.s, "proxy_set_enabled", &json!({"enabled":"yes"})).is_err());
}
#[test]
fn generation_is_forbidden_in_atomic_batches() {
    let mut s = Session::new();
    let before = serde_json::to_value(&s.project).unwrap();
    let e = dispatch(
        &mut s,
        "batch",
        &json!({"ops":[{"tool":"proxy_generate","args":{"asset_id":"a","path":"x.mp4"}}]}),
    )
    .unwrap_err();
    assert!(e.contains("forbidden_in_batch"));
    assert_eq!(serde_json::to_value(&s.project).unwrap(), before);
}
#[test]
fn fractional_cfr_keeps_frame_count_and_rate() {
    let mut f = fixture();
    let fractional = f.dir.join("fractional.mp4");
    ff(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=128x72:r=30000/1001:d=0.4",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &fractional,
    );
    let id = call(&mut f.s, "import_media", json!({"path":fractional}))["asset_id"]
        .as_str()
        .unwrap()
        .to_string();
    let out = f.dir.join("fractional-proxy.mp4");
    let r = call(
        &mut f.s,
        "proxy_generate",
        json!({"asset_id":id,"path":out,"max_side":64}),
    );
    assert_eq!(r["proxy"]["frames"], 12);
    let q = &f.s.project.proxies.entries[&id];
    assert!((q.fps_num as f64 / q.fps_den as f64 - 30000.0 / 1001.0).abs() < 1e-6);
}
#[test]
fn vfr_and_high_bit_sources_are_rejected_without_partial_files() {
    let mut f = fixture();
    let vfr = f.dir.join("vfr.mp4");
    ff(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=128x72:r=10:d=1",
            "-vf",
            "select='not(eq(n,3))'",
            "-fps_mode",
            "vfr",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &vfr,
    );
    assert!(proxy::inspect(&vfr, Duration::from_secs(20))
        .unwrap_err()
        .contains("frame rate"));
    let hdr = f.dir.join("highbit.mp4");
    ff(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=128x72:r=10:d=0.3",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p10le",
        ],
        &hdr,
    );
    assert!(proxy::inspect(&hdr, Duration::from_secs(20))
        .unwrap_err()
        .contains("8-bit SDR"));
    let id = call(&mut f.s, "import_media", json!({"path":vfr}))["asset_id"]
        .as_str()
        .unwrap()
        .to_string();
    let before = serde_json::to_value(&f.s.project).unwrap();
    assert!(dispatch(
        &mut f.s,
        "proxy_generate",
        &json!({"asset_id":id,"path":f.dir.join("bad-proxy.mp4")})
    )
    .is_err());
    assert_eq!(serde_json::to_value(&f.s.project).unwrap(), before);
    assert!(!f.dir.join("bad-proxy.mp4").exists());
    assert!(!std::fs::read_dir(&f.dir)
        .unwrap()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with(".kimo-proxy-")));
}
#[test]
fn managed_color_bypasses_proxy_selection() {
    let mut f = fixture();
    generate(&mut f);
    enable(&mut f);
    f.s.project.color_pipeline.enabled = true;
    assert!(proxy::preview_path(&f.s.project, &f.id).is_none());
    assert_eq!(state(&mut f, false)["managed_color_bypass"], true);
    assert_eq!(state(&mut f, false)["preview_active"], false);
}
#[test]
fn aspect_ratio_is_exact_and_upscaling_is_prevented() {
    assert_eq!(proxy::dimensions(1920, 1080, 960).unwrap(), (960, 540));
    assert_eq!(proxy::dimensions(128, 72, 4096).unwrap(), (128, 72));
    assert_eq!(proxy::dimensions(4096, 2160, 960).unwrap(), (512, 270));
    assert!(proxy::dimensions(101, 37, 64).is_err());
    assert!(proxy::dimensions(0, 72, 960).is_err());
}
#[test]
fn corrupt_proxy_decode_falls_back_to_source() {
    let mut f = fixture();
    let path = generate(&mut f);
    enable(&mut f);
    std::fs::write(&path, b"not a video").unwrap();
    f.s.project.proxies.entries.get_mut(&f.id).unwrap().proxy = proxy::fingerprint(&path).unwrap();
    render::prepare_preview_range(&f.s.project, &mut f.s.cache, 0.2, 0.2).unwrap();
    assert!(f.s.cache.proxy_assets().is_empty());
    is_red(&render::export_frame_rgba8(&f.s.project, 0.2, &f.s.cache).unwrap());
}

#[test]
fn cancellation_during_transcode_removes_partial_file_and_binding() {
    let mut f = fixture();
    let before = serde_json::to_value(&f.s.project).unwrap();
    let tok = kimo_core::guard::new_token();
    let watcher_token = tok.clone();
    let dir = f.dir.clone();
    let watcher = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while std::time::Instant::now() < deadline {
            if std::fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with(".kimo-proxy-"))
            {
                watcher_token.store(true, Ordering::Relaxed);
                return true;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        watcher_token.store(true, Ordering::Relaxed);
        false
    });
    let result = kimo_core::commands::dispatch_with(
        &mut f.s,
        "proxy_generate",
        &json!({"asset_id":f.id,"path":f.dir.join("cancelled.mp4"),"max_side":64}),
        &tok,
    );
    assert!(
        watcher.join().unwrap(),
        "watcher must observe the reserved partial file"
    );
    assert!(result.unwrap_err().contains("cancelled"));
    assert_eq!(serde_json::to_value(&f.s.project).unwrap(), before);
    assert!(!f.dir.join("cancelled.mp4").exists());
    assert!(!std::fs::read_dir(&f.dir)
        .unwrap()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with(".kimo-proxy-")));
}

#[test]
fn source_in_point_and_speed_use_the_same_frame_phase_with_proxies() {
    let mut f = fixture();
    ff(&["-f","lavfi","-i","color=red:s=128x72:r=10:d=0.5[r];color=blue:s=128x72:r=10:d=0.5[b];[r][b]concat=n=2:v=1:a=0","-c:v","libx264","-pix_fmt","yuv420p"],&f.source);
    generate(&mut f);
    enable(&mut f);
    let c = f.s.project.tracks[0].cm(0);
    c.in_point = 0.3;
    c.speed = 2.0;
    c.duration = 0.35;
    let mut original = render::Cache::new();
    let mut draft = render::Cache::new();
    for (t, blue) in [(0.0, false), (0.2, true)] {
        render::prepare_at(&f.s.project, &mut original, Some(t)).unwrap();
        render::prepare_preview_range(&f.s.project, &mut draft, t, t).unwrap();
        assert_eq!(draft.proxy_assets(), vec![f.id.clone()]);
        let a = render::export_frame_rgba8(&f.s.project, t, &original).unwrap();
        let b = render::export_frame_rgba8(&f.s.project, t, &draft).unwrap();
        if blue {
            is_blue(&a);
            is_blue(&b)
        } else {
            is_red(&a);
            is_red(&b)
        }
    }
}

#[test]
fn partial_proxy_fallback_tracks_each_clip_for_frame_cache_isolation() {
    let mut f = fixture();
    generate(&mut f);
    enable(&mut f);
    let short = f.dir.join("short-blue.mp4");
    ff(
        &[
            "-f",
            "lavfi",
            "-i",
            "color=blue:s=64x36:r=10:d=0.3",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &short,
    );
    let q = f.s.project.proxies.entries.get_mut(&f.id).unwrap();
    q.path = short.to_string_lossy().into_owned();
    q.proxy = proxy::fingerprint(&short).unwrap();
    let first = f.s.project.tracks[0].cm(0);
    first.duration = 0.3;
    let mut second = first.clone();
    second.id = "second".into();
    second.start = 0.3;
    second.in_point = 0.5;
    f.s.project.tracks[0]
        .clips
        .push(std::sync::Arc::new(second));
    let mut cache = render::Cache::new();
    let mut fc = framecache::FrameCache::new(1 << 20);
    render::prepare_at(&f.s.project, &mut cache, Some(0.1)).unwrap();
    is_red(&render::to_rgba8(
        &framecache::render_cached(&mut fc, &f.s.project, 0.1, &cache).unwrap(),
    ));
    render::prepare_preview_range(&f.s.project, &mut cache, 0.1, 0.5).unwrap();
    assert_eq!(cache.proxy_assets(), vec![f.id.clone()]);
    is_blue(&render::to_rgba8(
        &framecache::render_cached(&mut fc, &f.s.project, 0.1, &cache).unwrap(),
    ));
    is_red(&render::export_frame_rgba8(&f.s.project, 0.4, &cache).unwrap());
}

#[test]
fn collected_project_can_recover_copied_proxy_and_original_fingerprints() {
    let mut f = fixture();
    generate(&mut f);
    enable(&mut f);
    let dest = f.dir.join("collected");
    let result = call(
        &mut f.s,
        "collect_project",
        json!({"dest_dir":dest,"copy":"copy","rewrite":true}),
    );
    assert_eq!(result["files"], 2);
    let mut loaded = Session::new();
    call(
        &mut loaded,
        "load_project",
        json!({"path":dest.join("project.kimo")}),
    );
    assert!(loaded.project.proxies.enabled);
    let path = loaded
        .project
        .resolve(&loaded.project.proxies.entries[&f.id].path);
    let rebound = call(
        &mut loaded,
        "proxy_relink",
        json!({"asset_id":f.id,"path":path}),
    );
    assert_eq!(rebound["proxy"]["status"], "ready");
    assert_eq!(rebound["proxy"]["preview_active"], true);
    let checked = call(&mut loaded, "proxy_validate", json!({"deep":true}));
    assert_eq!(checked["proxies"][0]["status"], "ready");
}

#[test]
fn nested_compositions_use_proxies_only_in_preview() {
    let mut f = fixture();
    generate(&mut f);
    blue_proxy(&mut f);
    enable(&mut f);
    let inner = f.s.project.tracks.clone();
    let mut outer = (*inner[0].clips[0]).clone();
    outer.id = "outer".into();
    outer.source = kimo_core::model::Source::Precomp { tracks: inner };
    f.s.project.tracks[0].clips = vec![std::sync::Arc::new(outer)];
    render::prepare_preview_range(&f.s.project, &mut f.s.cache, 0.2, 0.2).unwrap();
    is_blue(&render::export_frame_rgba8(&f.s.project, 0.2, &f.s.cache).unwrap());
    render::prepare_export_range(&f.s.project, &mut f.s.cache, 0.2, 0.2).unwrap();
    is_red(&render::export_frame_rgba8(&f.s.project, 0.2, &f.s.cache).unwrap());
}
