//! Lazy, disposable analysis-only proxy for long-GOP source media.
//!
//! `render::decode()` always issues `-ss {clip.in_point} -t {span}` and reaches the requested
//! frame by decoding-and-discarding everything in between (see render.rs). For sequential/export
//! access this is fine (the window usually already covers the next request). For *random-access*
//! sampling deep into a clip - get_scopes/lint_edit's picture scan, lint_frame, scene_split - the
//! cost grows with how far into the clip the sample sits, and on media whose own keyframes are
//! sparse (phone footage is often one keyframe every several seconds, sometimes only one for the
//! whole clip) there is no cheaper point in the ORIGINAL file to seek to.
//!
//! This module builds, once per asset (cached by source path + size + mtime + project fps), a
//! visually near-lossless copy of the asset re-encoded with a short, fixed GOP (`-g`, `-sc_threshold
//! 0`, so keyframes land exactly every `gop` frames with no adaptive scene-cut insertion). Analysis
//! decode (render.rs, when asked) can then seek to the nearest preceding keyframe of *this* proxy
//! instead of the clip's fixed in_point, bounding the decode-and-discard cost by `gop` frames
//! instead of by how deep the sample is.
//!
//! Never used by export: export always resolves the original asset path and never looks at this
//! cache. Never mutates the project or the source media; the proxy is pure cache, safe to delete.
use crate::model::Project;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A ready, cached analysis proxy for one asset.
pub struct AnalysisProxy {
    pub path: String,
    pub gop: usize,
    pub fps: f64,
}

/// Minimum native max-keyframe-gap (seconds) before building a proxy is worth it at all.
const MIN_GAP_SECONDS: f64 = 2.0;
/// Target keyframe spacing in the proxy itself.
const GOP_SECONDS: f64 = 0.5;

/// Disk cache folder: KIMO_ANALYSIS_PROXY_CACHE ("off" = disabled), else <project dir>/.kimo_cache/analysis_proxy,
/// else the temp dir. Mirrors codeclip::cache_dir's convention.
fn cache_dir(base_dir: Option<&Path>) -> Option<PathBuf> {
    match std::env::var("KIMO_ANALYSIS_PROXY_CACHE") {
        Ok(v) if v == "off" || v == "0" => return None,
        Ok(v) if !v.is_empty() => return Some(PathBuf::from(v)),
        _ => {}
    }
    Some(match base_dir {
        Some(b) => b.join(".kimo_cache").join("analysis_proxy"),
        None => std::env::temp_dir().join("kimo_analysis_proxy_cache"),
    })
}

fn stamp_key(meta: &std::fs::Metadata, fps: f64) -> String {
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}-{:.6}", meta.len(), mtime_ns, fps)
}

fn proxy_filename(source_path: &str, key: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source_path.hash(&mut h);
    key.hash(&mut h);
    format!("{:016x}.mp4", h.finish())
}

fn sidecar_path(proxy_path: &Path) -> PathBuf {
    proxy_path.with_extension("json")
}

/// Probe the source's own keyframe times and return the largest gap between consecutive ones,
/// including the lead-in from 0 and the tail to `duration`. ffprobe-only; never writes anything.
pub fn max_keyframe_gap(path: &str, duration: f64) -> Result<f64, String> {
    let out = Command::new("ffprobe")
        .args([
            "-v", "error", "-select_streams", "v:0", "-show_entries", "frame=key_frame,pkt_dts_time",
            "-of", "default=noprint_wrappers=1:nokey=1", path,
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let mut times: Vec<f64> = vec![0.0];
    while let (Some(kf), Some(t)) = (lines.next(), lines.next()) {
        if kf.trim() == "1" {
            if let Ok(v) = t.trim().parse::<f64>() {
                times.push(v);
            }
        }
    }
    let tail = duration.max(*times.last().unwrap_or(&0.0));
    times.push(tail);
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let mut max_gap = 0.0f64;
    for w in times.windows(2) {
        max_gap = max_gap.max(w[1] - w[0]);
    }
    Ok(max_gap)
}

struct Slot {
    dir: PathBuf,
    key: String,
    proxy_path: PathBuf,
    side: PathBuf,
}

/// Where this source's proxy lives in the cache, or None when the cache is off / the source is unusable.
fn slot(base_dir: Option<&Path>, original_path: &str, fps: f64) -> Option<Slot> {
    if crate::render::is_still_path(original_path) || original_path.ends_with(".kimopx") {
        return None;
    }
    if std::env::var("KIMO_ANALYSIS_PROXY").ok().as_deref() == Some("off") {
        return None;
    }
    if !(1.0..=240.0).contains(&fps) {
        return None;
    }
    let dir = cache_dir(base_dir)?;
    let meta = std::fs::metadata(original_path).ok()?;
    let key = stamp_key(&meta, fps);
    let proxy_path = dir.join(proxy_filename(original_path, &key));
    let side = sidecar_path(&proxy_path);
    Some(Slot { dir, key, proxy_path, side })
}

/// A finished proxy for this slot, when its sidecar matches the current source stamp. A stale or corrupt sidecar
/// reads as "not cached", so the caller rebuilds cleanly.
fn cached(s: &Slot, fps: f64) -> Option<AnalysisProxy> {
    if !s.proxy_path.is_file() {
        return None;
    }
    let j = std::fs::read_to_string(&s.side).ok()?;
    let v = serde_json::from_str::<serde_json::Value>(&j).ok()?;
    if v.get("source_key").and_then(|x| x.as_str()) != Some(s.key.as_str()) {
        return None;
    }
    let gop = v.get("gop").and_then(|x| x.as_u64())?;
    Some(AnalysisProxy { path: s.proxy_path.to_string_lossy().into_owned(), gop: gop as usize, fps })
}

/// Lazily build (if missing or stale) and return a short-GOP analysis proxy for one video asset,
/// when its own keyframe spacing is long enough to make random-access decode expensive. Returns
/// None (never an error) for anything that makes the fast path unavailable: stills, disabled cache,
/// an asset whose keyframes are already frequent, a build already running in the background,
/// or any ffprobe/ffmpeg failure - callers always fall back to decoding the original, exactly as
/// before this module existed.
pub fn ensure(p: &Project, asset_id: &str, original_path: &str) -> Option<AnalysisProxy> {
    let fps = p.fps;
    let s = slot(p.base_dir.as_deref(), original_path, fps)?;
    if let Some(ap) = cached(&s, fps) {
        return Some(ap);
    }
    let asset = p.assets.get(asset_id)?;
    let key = s.proxy_path.to_string_lossy().into_owned();
    // Someone (the import-time proxy queue in auto_proxy.rs, or an earlier draft render) is already building this
    // file: decode the original this time instead of encoding the same file twice into the same temp path.
    if !claim(&key) {
        return None;
    }
    // Observe's draft renders must never wait for a proxy (building one for a 10-minute file takes about a minute):
    // start the build in the background once and decode the original this time; later renders find it ready.
    if NONBLOCKING.with(|n| n.get()) {
        let (orig, dur, k2) = (original_path.to_string(), asset.duration, key.clone());
        // A detached one-shot builder. Lifecycle: ends when its ffmpeg ends; nothing joins it; it releases its claim on
        // the way out so a later render can retry. Started through the guarded path (inherits the caller's token).
        if crate::guard::spawn_builder_with_token(std::thread::Builder::new().name("kimo-analysis-proxy".into()), move || { let _ = build(&orig, dur, fps, &s, None, &mut |_| {}); release(&k2); }).is_err() {
            release(&key);
        }
        return None;
    }
    let r = build(original_path, asset.duration, fps, &s, None, &mut |_| {});
    release(&key);
    r
}

/// Build (or find cached) the analysis proxy for one source on the calling thread, reporting progress 0..1 while
/// ffmpeg encodes. The import-time proxy queue (auto_proxy.rs) calls this so later analysis and draft renders find the
/// proxy ready. `known_gap` skips the keyframe probe when the caller already measured it. `Err(reason)` says why there
/// is no proxy (keyframes already frequent, cache off, another build in flight, ffmpeg failure); every one of those
/// leaves callers decoding the original, as before.
pub fn prebuild(base_dir: Option<&Path>, original_path: &str, duration: f64, fps: f64, known_gap: Option<f64>, progress: &mut dyn FnMut(f64)) -> Result<AnalysisProxy, String> {
    let s = slot(base_dir, original_path, fps).ok_or("analysis proxies are off for this source")?;
    if let Some(ap) = cached(&s, fps) {
        return Ok(ap);
    }
    let key = s.proxy_path.to_string_lossy().into_owned();
    if !claim(&key) {
        return Err("another analysis proxy build for this source is in flight".into());
    }
    let r = build(original_path, duration, fps, &s, known_gap, progress);
    release(&key);
    r.ok_or_else(|| "keyframes are already frequent, or the analysis encode failed".into())
}

// thread-local-ok: a scoped "do not block on a proxy build" flag set only by observe's draft renders (NonBlocking restores it on drop); it holds no render state
thread_local! {
    /// Set (scoped, via `NonBlocking`) by draft renders that must not wait for a proxy build.
    static NONBLOCKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// Proxy files being built right now (by any thread), so no two builders ever write the same temp file.
static BUILDING: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<String>>> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));
fn claim(key: &str) -> bool {
    BUILDING.lock().unwrap_or_else(|e| e.into_inner()).insert(key.to_string())
}
fn release(key: &str) {
    BUILDING.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
}

/// While alive, `ensure` on this thread never builds a proxy synchronously (it starts one in the background instead).
/// Restores the previous setting when dropped, so it cannot leak into later renders on the same thread.
pub struct NonBlocking(bool);
impl NonBlocking { pub fn new() -> NonBlocking { NonBlocking(NONBLOCKING.with(|n| n.replace(true))) } }
impl Drop for NonBlocking { fn drop(&mut self) { NONBLOCKING.with(|n| n.set(self.0)); } }

/// True when a source whose largest keyframe gap is `gap_seconds` is slow enough to seek that a proxy pays for itself.
pub fn worth_building(gap_seconds: f64) -> bool {
    gap_seconds > MIN_GAP_SECONDS
}

fn build(original_path: &str, duration: f64, fps: f64, s: &Slot, known_gap: Option<f64>, progress: &mut dyn FnMut(f64)) -> Option<AnalysisProxy> {
    let gap = match known_gap { Some(g) => g, None => max_keyframe_gap(original_path, duration).ok()? };
    if !worth_building(gap) {
        return None; // already cheap to seek on the original; a proxy would only cost disk and time
    }
    std::fs::create_dir_all(&s.dir).ok()?;
    let gop = ((fps * GOP_SECONDS).round() as usize).max(1);
    let tmp = s.proxy_path.with_extension("mp4.part");
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
            "-y", "-v", "error", "-nostdin", "-i", original_path,
            "-vf", &format!("fps={fps}"),
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "14",
            "-g", &gop.to_string(), "-keyint_min", &gop.to_string(), "-sc_threshold", "0",
            "-pix_fmt", "yuv420p", "-an", "-f", "mp4",
        ])
        .arg(&tmp);
    if crate::auto_proxy::run_ffmpeg(&mut cmd, crate::auto_proxy::Measure::Seconds(duration), progress).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    if std::fs::rename(&tmp, &s.proxy_path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    let _ = std::fs::write(&s.side, serde_json::json!({"source_key": s.key, "gop": gop, "fps": fps}).to_string());
    Some(AnalysisProxy { path: s.proxy_path.to_string_lossy().into_owned(), gop, fps })
}

/// The nearest-preceding-keyframe seek point for `start` (a clip-relative frame index, same
/// indexing as render::decode's `a`): `(seek_time_seconds, frame_index_at_that_seek_point)`.
/// `frame_index_at_that_seek_point` is clip-relative too (can be negative when the clip's own
/// in_point is not itself gop-aligned; render::decode handles that with signed arithmetic) and is
/// always <= `start`, so the caller only ever decodes forward.
pub fn seek_base_for(ap: &AnalysisProxy, c: &crate::model::Clip, start: usize) -> Option<(f64, i64)> {
    if ap.gop == 0 {
        return None;
    }
    let in_point_frames = (c.in_point * ap.fps).round() as i64;
    let abs_frame = in_point_frames + start as i64;
    if abs_frame < 0 {
        return None;
    }
    let gop = ap.gop as i64;
    let kf_abs = (abs_frame / gop) * gop;
    let frame0 = kf_abs - in_point_frames;
    let seek_time = (kf_abs as f64 / ap.fps).max(0.0);
    Some((seek_time, frame0))
}

/// Prune this process's own cached proxies (and sidecars) down to a byte budget, oldest-used first.
/// KIMO_ANALYSIS_PROXY_CACHE_MAX_MB (default 2048). A hit is not currently timestamped as a use
/// (unlike codeclip's LRU); this is a simple size cap, not a recency cap, intentionally: build cost
/// dominates reuse cost here, so naive mtime-order (oldest build first) is an adequate first cut.
pub fn prune_cache(dir: &Path, max_bytes: u64) -> (usize, u64) {
    let Ok(rd) = std::fs::read_dir(dir) else { return (0, 0) };
    let mut v: Vec<(std::time::SystemTime, u64, PathBuf)> = vec![];
    for e in rd.filter_map(|e| e.ok()) {
        let name = e.file_name().to_string_lossy().into_owned();
        if !(name.len() == 20 && name.ends_with(".mp4") && name[..16].chars().all(|c| c.is_ascii_hexdigit())) {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        let mt = md.modified().unwrap_or(std::time::UNIX_EPOCH);
        v.push((mt, md.len(), e.path()));
    }
    let mut total: u64 = v.iter().map(|x| x.1).sum();
    v.sort();
    let mut left = v.len();
    for (_, n, path) in &v {
        if total <= max_bytes {
            break;
        }
        let side = sidecar_path(path);
        if std::fs::remove_file(path).is_ok() {
            let _ = std::fs::remove_file(&side);
            total -= n;
            left -= 1;
        }
    }
    (left, total)
}

pub fn cache_max_bytes() -> u64 {
    std::env::var("KIMO_ANALYSIS_PROXY_CACHE_MAX_MB").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(2048) * 1024 * 1024
}
