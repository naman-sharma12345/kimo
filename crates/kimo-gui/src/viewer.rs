//! The viewer: the engine's rendered frame at the playhead, short draft clips, and the frame before the last change.
//! Frames and drafts live in a small disk cache keyed by project revision (sha256 of project.get_project) and frame
//! index, so repeats, scrubs over rendered frames and undo are cache hits; the webview reads them through kimo://.
//! Engine actions used (shapes from kimo_describe): project.get_project, project.timeline_summary,
//! output.render_frame {time, path, inline}, output.preview_range {start, end, scale, path}.
use crate::agent::Tools;
use crate::engine::{is_error, result_text};
#[cfg(test)]
use crate::engine::Supervisor;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// Frames kept on disk (a 720p PNG is tens of KB to a few MB); drafts kept.
pub const FRAME_CAP: usize = 96;
pub const DRAFT_CAP: usize = 4;
/// Draft clips are half resolution: cheap enough to render on a laptop CPU, sharp enough to judge an edit.
pub const DRAFT_SCALE: f64 = 0.5;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Frame { pub rev: String, pub index: i64, pub time: f64, pub url: String, #[serde(skip)] pub file: PathBuf, pub sha256: String, pub width: u32, pub height: u32, pub ms: u128, pub cached: bool }
#[derive(Clone, Debug, Serialize)]
pub struct Shown { pub empty: bool, pub rev: String, pub duration: f64, pub fps: f64, pub width: u32, pub height: u32, pub frame: Option<Frame>, pub before: Option<Frame>, pub changed: bool }
#[derive(Clone, Debug, Serialize)]
pub struct Draft { pub rev: String, pub start: f64, pub end: f64, pub fps: f64, pub frames: u64, pub width: u32, pub height: u32, pub url: String, #[serde(skip)] pub file: PathBuf, pub ms: u128, pub first_frame_ms: u128, pub cached: bool,
    /// The draft's frames as JPEGs, one per frame: scrubbing inside the draft, and Play where the webview has no H.264 decoder.
    pub frame_urls: Vec<String>, #[serde(skip)] pub frame_files: Vec<PathBuf>, pub extract_ms: u128 }

/// The whole frame at or nearest to `t` seconds.
pub fn frame_index(t: f64, fps: f64) -> i64 { if !(t > 0.0) || !(fps > 0.0) { 0 } else { (t * fps).round() as i64 } }
pub fn frame_time(i: i64, fps: f64) -> f64 { if fps > 0.0 { i as f64 / fps } else { 0.0 } }

/// The URL the webview loads a cache file from (Tauri serves custom schemes as http://<scheme>.localhost on Windows).
pub fn cache_url(name: &str) -> String { if cfg!(windows) { format!("http://kimo.localhost/{name}") } else { format!("kimo://localhost/{name}") } }

/// Map a kimo:// request path to a file in the cache: only the lower-case hex names this module writes, .png, .jpg or .mp4.
pub fn cache_file(dir: &Path, uri_path: &str) -> Option<(PathBuf, &'static str)> {
    let name = uri_path.split(['?', '#']).next()?.trim_start_matches('/');
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || !stem.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) { return None; }
    let mime = match ext { "png" => "image/png", "jpg" => "image/jpeg", "mp4" => "video/mp4", _ => return None };
    let p = dir.join(name);
    p.is_file().then_some((p, mime))
}

/// One HTTP byte range (what a <video> element asks for when it seeks). Ok(None) = send the whole file; Err = 416.
pub fn byte_range(h: Option<&str>, len: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(spec) = h.and_then(|h| h.trim().strip_prefix("bytes=")) else { return Ok(None) };
    let first = spec.split(',').next().unwrap_or("").trim();
    let (a, b) = first.split_once('-').ok_or(())?;
    if len == 0 { return Err(()); }
    let (s, e) = if a.is_empty() {
        let n: u64 = b.parse().map_err(|_| ())?;
        if n == 0 { return Err(()); }
        (len.saturating_sub(n), len - 1)
    } else {
        let s: u64 = a.parse().map_err(|_| ())?;
        let e = if b.is_empty() { len - 1 } else { b.parse::<u64>().map_err(|_| ())?.min(len - 1) };
        (s, e)
    };
    if s >= len || s > e { return Err(()); }
    Ok(Some((s, e)))
}

/// The kimo:// answer for one request: status, headers, body. Only cache files are served; a Range gets a 206.
pub fn respond(dir: &Path, uri_path: &str, range: Option<&str>) -> (u16, Vec<(&'static str, String)>, Vec<u8>) {
    let Some((file, mime)) = cache_file(dir, uri_path) else { return (404, vec![("Content-Type", "text/plain".into())], b"not in the viewer cache".to_vec()) };
    let Ok(bytes) = std::fs::read(&file) else { return (404, vec![("Content-Type", "text/plain".into())], b"gone".to_vec()) };
    let len = bytes.len() as u64;
    let mut h = vec![("Content-Type", mime.to_string()), ("Accept-Ranges", "bytes".to_string()), ("Cache-Control", "no-store".to_string())];
    match byte_range(range, len) {
        Ok(None) => { h.push(("Content-Length", len.to_string())); (200, h, bytes) }
        Ok(Some((s, e))) => {
            h.push(("Content-Range", format!("bytes {s}-{e}/{len}")));
            h.push(("Content-Length", (e - s + 1).to_string()));
            (206, h, bytes[s as usize..=e as usize].to_vec())
        }
        Err(()) => { h.push(("Content-Range", format!("bytes */{len}"))); (416, h, vec![]) }
    }
}

/// Split a draft mp4 into one JPEG per frame (`<stem>f00000.jpg` …), named so the protocol serves them. Empty on failure:
/// the draft still plays as video where the webview can decode it.
pub fn split_frames(ffmpeg: &Path, mp4: &Path, dir: &Path, stem: &str) -> Vec<PathBuf> {
    let pattern = dir.join(format!("{stem}f%05d.jpg"));
    let mut c = std::process::Command::new(ffmpeg);
    c.args(["-v", "error", "-y", "-i"]).arg(mp4).args(["-start_number", "0", "-q:v", "4"]).arg(&pattern)
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); }
    if !c.status().map(|s| s.success()).unwrap_or(false) { return vec![]; }
    (0..).map(|i| dir.join(format!("{stem}f{i:05}.jpg"))).take_while(|p| p.is_file()).collect()
}

/// ffmpeg for draft frames: the first-run tools dir, else PATH.
pub fn find_ffmpeg(data_dir: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let p = data_dir.join("tools").join(name);
    if p.is_file() { Some(p) } else { crate::system::find("ffmpeg") }
}

/// Least-recently-used frames by (revision, frame index); evicting one deletes its file unless it is pinned.
pub struct FrameCache { cap: usize, items: Vec<Frame> }
impl FrameCache {
    pub fn new(cap: usize) -> Self { FrameCache { cap: cap.max(1), items: Vec::new() } }
    pub fn get(&mut self, rev: &str, i: i64) -> Option<Frame> {
        let pos = self.items.iter().position(|f| f.rev == rev && f.index == i)?;
        let f = self.items.remove(pos);
        self.items.push(f.clone());
        Some(f)
    }
    pub fn put(&mut self, f: Frame, pinned: &[PathBuf]) {
        self.items.retain(|x| !(x.rev == f.rev && x.index == f.index));
        self.items.push(f);
        while self.items.len() > self.cap {
            let Some(pos) = self.items.iter().position(|x| !pinned.contains(&x.file)) else { break };
            let old = self.items.remove(pos);
            if !self.items.iter().any(|x| x.file == old.file) { let _ = std::fs::remove_file(&old.file); }
        }
    }
    pub fn len(&self) -> usize { self.items.len() }
    pub fn is_empty(&self) -> bool { self.items.is_empty() }
}

#[derive(Default)]
struct State { last_rev: Option<String>, last_frame: Option<Frame>, before_rev: Option<String>, before_fallback: Option<Frame>, drafts: Vec<Draft> }

pub struct Viewer { dir: PathBuf, cache: Mutex<FrameCache>, st: Mutex<State>, render: Mutex<()>, latest: AtomicU64, ffmpeg: Option<PathBuf> }

struct Info { rev: String, duration: f64, fps: f64, width: u32, height: u32 }

fn engine(t: &dyn Tools, tool: &str, action: &str, args: Value) -> Result<Value, String> {
    let r = t.call(tool, &json!({"action": action, "args": args}))?;
    if is_error(&r) { return Err(result_text(&r)); }
    Ok(r)
}
fn num(v: &Value, k: &str) -> f64 { v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) }

impl Viewer {
    /// `dir` is the viewer's own cache directory (created if missing).
    pub fn new(dir: PathBuf) -> Viewer {
        let _ = std::fs::create_dir_all(&dir);
        Viewer { dir, cache: Mutex::new(FrameCache::new(FRAME_CAP)), st: Mutex::new(State::default()), render: Mutex::new(()), latest: AtomicU64::new(0), ffmpeg: None }
    }
    /// The ffmpeg that splits drafts into frames (the same one the engine uses: downloaded tools dir, else PATH).
    pub fn with_ffmpeg(mut self, ffmpeg: Option<PathBuf>) -> Viewer { self.ffmpeg = ffmpeg; self }
    pub fn dir(&self) -> &Path { &self.dir }

    /// The project's revision and timeline facts: two read-only engine calls, no render.
    fn info(&self, t: &dyn Tools) -> Result<Info, String> {
        let p = engine(t, "project", "get_project", json!({}))?;
        let rev: String = Sha256::digest(result_text(&p).as_bytes()).iter().take(8).map(|b| format!("{b:02x}")).collect();
        let s = engine(t, "project", "timeline_summary", json!({}))?;
        let v: Value = serde_json::from_str(&result_text(&s)).map_err(|e| format!("timeline_summary: {e}"))?;
        let has_clips = v.get("tracks_bottom_to_top").and_then(|t| t.as_array()).map(|a| a.iter().any(|tr| tr.get("clips").and_then(|c| c.as_array()).is_some_and(|c| !c.is_empty()))).unwrap_or(false);
        let duration = if has_clips { num(&v, "duration") } else { 0.0 };
        Ok(Info { rev, duration, fps: if num(&v, "fps") > 0.0 { num(&v, "fps") } else { 30.0 }, width: num(&v, "width") as u32, height: num(&v, "height") as u32 })
    }

    /// The frame at the playhead for the project as it is now, plus the frame before the last change.
    /// Renders only on a cache miss; a request overtaken by a newer one while it waited returns Err("superseded").
    pub fn frame(&self, t: &dyn Tools, time: f64) -> Result<Shown, String> {
        let me = self.latest.fetch_add(1, Ordering::SeqCst) + 1;
        let _g = self.render.lock().unwrap_or_else(|e| e.into_inner());
        if self.latest.load(Ordering::SeqCst) != me { return Err("superseded".into()); }
        let info = self.info(t)?;
        let mut shown = Shown { empty: info.duration <= 0.0, rev: info.rev.clone(), duration: info.duration, fps: info.fps, width: info.width, height: info.height, frame: None, before: None, changed: false };
        let last_index = frame_index((info.duration - 1.0 / info.fps).max(0.0), info.fps);
        let idx = frame_index(time, info.fps).min(last_index);
        let frame = if shown.empty { None } else {
            let hit = self.cache.lock().unwrap().get(&info.rev, idx);
            Some(match hit {
                Some(mut f) => { f.cached = true; f }
                None => self.render_frame(t, &info, idx)?,
            })
        };
        let mut st = self.st.lock().unwrap();
        if let Some(prev) = st.last_rev.clone().filter(|p| *p != info.rev) {
            st.before_fallback = st.last_frame.clone();
            st.before_rev = Some(prev);
            shown.changed = true;
        }
        if !shown.empty {
            let mut cache = self.cache.lock().unwrap();
            shown.before = st.before_rev.as_deref().and_then(|br| cache.get(br, idx)).or_else(|| st.before_fallback.clone());
            if let Some(f) = &frame {
                let pinned: Vec<PathBuf> = [Some(f), shown.before.as_ref(), st.before_fallback.as_ref()].into_iter().flatten().map(|x| x.file.clone()).collect();
                cache.put(Frame { cached: false, ..f.clone() }, &pinned);
            }
            st.last_frame = frame.clone();
        }
        st.last_rev = Some(info.rev.clone());
        shown.frame = frame;
        Ok(shown)
    }

    fn render_frame(&self, t: &dyn Tools, info: &Info, idx: i64) -> Result<Frame, String> {
        let name = format!("{}{:08x}.png", info.rev, idx);
        let file = self.dir.join(&name);
        let at = frame_time(idx, info.fps);
        let t0 = Instant::now();
        let r = engine(t, "output", "render_frame", json!({"time": at, "path": file, "inline": "never"}))?;
        let ms = t0.elapsed().as_millis();
        let sc = r.get("structuredContent").cloned().unwrap_or(Value::Null);
        let sha256 = match sc.get("sha256").and_then(|s| s.as_str()) {
            Some(s) => s.to_string(),
            None => std::fs::read(&file).map(|b| Sha256::digest(&b).iter().map(|b| format!("{b:02x}")).collect()).map_err(|e| format!("frame not written: {e}"))?,
        };
        if !file.is_file() { return Err("the engine reported a frame but wrote no file".into()); }
        let (w, h) = (num(&sc, "width") as u32, num(&sc, "height") as u32);
        Ok(Frame { rev: info.rev.clone(), index: idx, time: at, url: cache_url(&name), file, sha256, width: if w > 0 { w } else { info.width }, height: if h > 0 { h } else { info.height }, ms, cached: false })
    }

    /// A draft mp4 of `secs` seconds from `start` (half resolution, every frame), cached per revision and range.
    pub fn draft(&self, t: &dyn Tools, start: f64, secs: f64) -> Result<Draft, String> {
        let _g = self.render.lock().unwrap_or_else(|e| e.into_inner());
        let info = self.info(t)?;
        let sidx = frame_index(start, info.fps);
        let n = frame_index(secs.clamp(0.1, 30.0), info.fps).max(1);
        let name = format!("{}{:08x}{:04x}.mp4", info.rev, sidx, n);
        {
            let mut st = self.st.lock().unwrap();
            if let Some(pos) = st.drafts.iter().position(|d| d.url == cache_url(&name) && d.file.is_file()) {
                let d = st.drafts.remove(pos);
                st.drafts.push(d.clone());
                return Ok(Draft { cached: true, ..d });
            }
        }
        let file = self.dir.join(&name);
        let s = frame_time(sidx, info.fps);
        let t0 = Instant::now();
        let r = engine(t, "output", "preview_range", json!({"start": s, "end": s + frame_time(n, info.fps), "scale": DRAFT_SCALE, "path": file, "audio": false}))?;
        let ms = t0.elapsed().as_millis();
        let v: Value = serde_json::from_str(&result_text(&r)).map_err(|e| format!("preview_range: {e}"))?;
        if !file.is_file() { return Err("the engine reported a draft but wrote no file".into()); }
        let timing = v.get("timing_ms").cloned().unwrap_or(Value::Null);
        let stem = name.trim_end_matches(".mp4").to_string();
        let t1 = Instant::now();
        let frame_files = self.ffmpeg.as_deref().map(|ff| split_frames(ff, &file, &self.dir, &stem)).unwrap_or_default();
        let d = Draft { rev: info.rev, start: num(&v, "start"), end: num(&v, "end"), fps: num(&v, "fps"), frames: v.get("frames").and_then(|f| f.as_u64()).unwrap_or(0),
            width: num(&v, "width") as u32, height: num(&v, "height") as u32, url: cache_url(&name), file, ms, first_frame_ms: num(&timing, "first_frame") as u128, cached: false,
            frame_urls: frame_files.iter().filter_map(|f| f.file_name()).map(|n| cache_url(&n.to_string_lossy())).collect(), frame_files, extract_ms: t1.elapsed().as_millis() };
        let mut st = self.st.lock().unwrap();
        st.drafts.push(d.clone());
        while st.drafts.len() > DRAFT_CAP {
            let old = st.drafts.remove(0);
            let _ = std::fs::remove_file(&old.file);
            for f in &old.frame_files { let _ = std::fs::remove_file(f); }
        }
        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // ---------- pure parts (no engine) ----------
    #[test]
    fn playhead_snaps_to_whole_frames() {
        assert_eq!(frame_index(1.0, 30.0), 30);
        assert_eq!(frame_index(1.016, 30.0), 30);
        assert_eq!(frame_index(1.02, 30.0), 31);
        assert_eq!(frame_index(-3.0, 30.0), 0);
        assert!((frame_time(31, 30.0) - 31.0 / 30.0).abs() < 1e-9);
    }

    #[test]
    fn cache_urls_name_only_hex_files_inside_the_cache() {
        let dir = std::env::temp_dir().join(format!("kimo-vw-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ab12cd.png"), b"x").unwrap();
        assert_eq!(cache_file(&dir, "/ab12cd.png").map(|(_, m)| m), Some("image/png"));
        assert!(cache_file(&dir, "/../etc/passwd").is_none());
        assert!(cache_file(&dir, "/ab12cd.png/../../x").is_none());
        assert!(cache_file(&dir, "/AB12CD.png").is_none(), "names are lower-case hex we wrote ourselves");
        assert!(cache_file(&dir, "/ffff.png").is_none(), "missing file");
        assert!(cache_file(&dir, "/ab12cd.sh").is_none(), "only png, jpg and mp4");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn byte_ranges_for_video_seeking() {
        assert_eq!(byte_range(None, 100), Ok(None));
        assert_eq!(byte_range(Some("bytes=0-"), 100), Ok(Some((0, 99))));
        assert_eq!(byte_range(Some("bytes=10-19"), 100), Ok(Some((10, 19))));
        assert_eq!(byte_range(Some("bytes=90-500"), 100), Ok(Some((90, 99))));
        assert_eq!(byte_range(Some("bytes=-10"), 100), Ok(Some((90, 99))));
        assert!(byte_range(Some("bytes=100-"), 100).is_err(), "past the end is 416");
        assert_eq!(byte_range(Some("items=1-2"), 100), Ok(None), "unknown unit: whole file");
    }

    #[test]
    fn frame_cache_evicts_oldest_and_deletes_its_file_but_keeps_pinned_ones() {
        let dir = std::env::temp_dir().join(format!("kimo-vw-lru-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut c = FrameCache::new(2);
        let mk = |n: i64| { let p = dir.join(format!("{n:02x}.png")); std::fs::write(&p, b"x").unwrap();
            Frame { rev: "r".into(), index: n, time: n as f64, url: String::new(), file: p, sha256: String::new(), width: 1, height: 1, ms: 0, cached: false } };
        let (a, b, d) = (mk(1), mk(2), mk(3));
        c.put(a.clone(), &[]); c.put(b.clone(), &[]);
        assert!(c.get("r", 1).is_some()); // touch 1: 2 is now the oldest
        c.put(d.clone(), &[]);
        assert!(c.get("r", 2).is_none() && !b.file.exists(), "oldest evicted and its file removed");
        assert!(c.get("r", 1).is_some() && c.get("r", 3).is_some());
        let e = mk(4);
        c.put(e, &[a.file.clone(), d.file.clone()]);
        assert!(a.file.exists() && d.file.exists(), "a pinned frame (shown / before) keeps its file");
        let _ = std::fs::remove_dir_all(&dir);
    }


    #[test]
    fn the_protocol_serves_cache_files_whole_or_by_range() {
        let dir = std::env::temp_dir().join(format!("kimo-vw-resp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("0a.mp4"), b"0123456789").unwrap();
        let (s, h, b) = respond(&dir, "/0a.mp4", None);
        assert_eq!((s, b.as_slice()), (200, &b"0123456789"[..]));
        assert!(h.contains(&("Content-Type", "video/mp4".into())) && h.contains(&("Accept-Ranges", "bytes".into())));
        let (s, h, b) = respond(&dir, "/0a.mp4", Some("bytes=2-4"));
        assert_eq!((s, b.as_slice()), (206, &b"234"[..]));
        assert!(h.contains(&("Content-Range", "bytes 2-4/10".into())));
        assert_eq!(respond(&dir, "/0a.mp4", Some("bytes=50-")).0, 416);
        assert_eq!(respond(&dir, "/../0a.mp4", None).0, 404);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---------- the real engine ----------
    fn bin() -> Option<std::path::PathBuf> { std::env::var_os("KIMO_MCP_BIN").map(std::path::PathBuf::from).filter(|p| p.is_file()) }

    /// Counts engine calls by action, so a test can prove a cache hit made no render.
    struct Counting<'a> { sup: &'a Supervisor, renders: AtomicUsize, previews: AtomicUsize }
    impl Tools for Counting<'_> {
        fn list(&self) -> Vec<Value> { self.sup.tools() }
        fn call(&self, name: &str, args: &Value) -> Result<Value, String> {
            match args.get("action").and_then(|a| a.as_str()) {
                Some("render_frame") => { self.renders.fetch_add(1, Ordering::SeqCst); }
                Some("preview_range") => { self.previews.fetch_add(1, Ordering::SeqCst); }
                _ => {}
            }
            self.sup.call(name, args)
        }
    }
    fn engine() -> Option<Supervisor> {
        let b = bin()?;
        let sup = Supervisor::new(Some(b), vec![("KIMO_SPECTATOR".into(), "off".into()), ("KIMO_INLINE_IMAGES".into(), "0".into())], None);
        sup.start();
        Some(sup)
    }
    fn ok(r: Result<Value, String>) -> Value { let v = r.unwrap(); assert!(!crate::engine::is_error(&v), "engine error: {}", crate::engine::result_text(&v)); v }
    fn gradient(sup: &Supervisor, start: f64, dur: f64, a: &str, b: &str) {
        let tr = ok(sup.call("timeline", &json!({"action": "add_track", "args": {}})));
        let tid: Value = serde_json::from_str(&crate::engine::result_text(&tr)).unwrap();
        ok(sup.call("timeline", &json!({"action": "add_clip", "args": {"track_id": tid["track_id"], "source_type": "gradient",
            "start": start, "duration": dur, "color": a, "color2": b, "angle": 90}})));
    }
    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("kimo-vw-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d); d
    }

    #[test]
    fn an_empty_project_is_the_empty_state_not_an_error() {
        let Some(sup) = engine() else { eprintln!("KIMO_MCP_BIN unset: skipped"); return };
        let dir = tmpdir("empty");
        let v = Viewer::new(dir.clone());
        let s = v.frame(&sup, 0.0).unwrap();
        assert!(s.empty && s.frame.is_none(), "{s:?}");
        let e = v.draft(&sup, 0.0, 2.0).unwrap_err();
        assert!(e.contains("empty"), "the engine's own words: {e}");
        sup.stop(); let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renders_the_frame_at_the_playhead_and_serves_repeats_from_cache() {
        let Some(sup) = engine() else { eprintln!("KIMO_MCP_BIN unset: skipped"); return };
        gradient(&sup, 0.0, 3.0, "#ff5a24", "#141210");
        let dir = tmpdir("frame");
        let v = Viewer::new(dir.clone());
        let t = Counting { sup: &sup, renders: AtomicUsize::new(0), previews: AtomicUsize::new(0) };
        let s = v.frame(&t, 1.0).unwrap();
        let f = s.frame.clone().expect("a frame");
        assert!(!s.empty && (s.duration - 3.0).abs() < 1e-6 && (s.fps - 30.0).abs() < 1e-6, "{s:?}");
        assert_eq!((f.width, f.height, f.index), (1280, 720, 30));
        assert!(f.file.starts_with(&dir) && f.file.is_file() && std::fs::read(&f.file).unwrap().starts_with(b"\x89PNG"));
        let name = f.file.file_name().unwrap().to_string_lossy().to_string();
        assert!(f.url == format!("kimo://localhost/{name}") || f.url == format!("http://kimo.localhost/{name}"), "{}", f.url);
        // the very frame a direct engine render gives at that time
        let direct = dir.join("direct.png");
        let r = ok(sup.call("output", &json!({"action": "render_frame", "args": {"time": 1.0, "path": direct, "inline": "never"}})));
        assert_eq!(r["structuredContent"]["sha256"].as_str().unwrap(), f.sha256);
        // same revision, same frame (1.01 s snaps to frame 30): served from the cache, no engine render
        let n = t.renders.load(Ordering::SeqCst);
        let again = v.frame(&t, 1.01).unwrap().frame.unwrap();
        assert!(again.cached && again.sha256 == f.sha256);
        assert_eq!(t.renders.load(Ordering::SeqCst), n, "cache hit made no engine render");
        eprintln!("viewer: 1080-class frame {} ms, repeat from cache", f.ms);
        sup.stop(); let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_edit_shows_the_new_frame_and_keeps_the_one_before() {
        let Some(sup) = engine() else { eprintln!("KIMO_MCP_BIN unset: skipped"); return };
        gradient(&sup, 0.0, 3.0, "#ff5a24", "#141210");
        let dir = tmpdir("ab");
        let v = Viewer::new(dir.clone());
        let first = v.frame(&sup, 1.5).unwrap();
        assert!(first.before.is_none() && !first.changed);
        let old = first.frame.unwrap();
        // an edit (a different gradient on a track above covers the frame)
        gradient(&sup, 0.0, 3.0, "#2050ff", "#f5f1e7");
        let after = v.frame(&sup, 1.5).unwrap();
        let new = after.frame.clone().unwrap();
        assert!(after.changed && new.rev != old.rev && new.sha256 != old.sha256, "new revision, new pixels");
        let before = after.before.clone().expect("the frame before the last change");
        assert_eq!((before.sha256.as_str(), before.index), (old.sha256.as_str(), old.index));
        assert!(before.file.is_file());
        // still the same revision: the before frame stays, nothing re-rendered
        let again = v.frame(&sup, 1.5).unwrap();
        assert!(!again.changed && again.before.unwrap().sha256 == old.sha256 && again.frame.unwrap().cached);
        // undo goes back to the old revision: its frame comes from the cache
        ok(sup.call("project", &json!({"action": "undo"})));
        ok(sup.call("project", &json!({"action": "undo"})));
        let undone = v.frame(&sup, 1.5).unwrap();
        let f = undone.frame.unwrap();
        assert!(undone.changed && f.rev == old.rev && f.sha256 == old.sha256 && f.cached, "undo is a cache hit");
        assert_eq!(undone.before.unwrap().sha256, new.sha256, "before/after flips: the frame before the undo");
        sup.stop(); let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plays_a_short_draft_clip_cached_per_revision() {
        let Some(sup) = engine() else { eprintln!("KIMO_MCP_BIN unset: skipped"); return };
        gradient(&sup, 0.0, 3.0, "#ff5a24", "#141210");
        let dir = tmpdir("draft");
        let v = Viewer::new(dir.clone()).with_ffmpeg(find_ffmpeg(&dir));
        let t = Counting { sup: &sup, renders: AtomicUsize::new(0), previews: AtomicUsize::new(0) };
        let d = v.draft(&t, 0.5, 2.0).unwrap();
        assert!((d.start - 0.5).abs() < 1e-6 && (d.end - 2.5).abs() < 1e-6, "{d:?}");
        assert_eq!((d.frames, d.width, d.height), (60, 640, 360), "half-res, every frame");
        let bytes = std::fs::read(&d.file).unwrap();
        assert!(d.file.starts_with(&dir) && bytes.len() > 1000 && &bytes[4..8] == b"ftyp", "an mp4 in the cache");
        // and its frames as JPEGs, one per frame, for scrubbing and for webviews without an H.264 decoder
        assert!(find_ffmpeg(&dir).is_some(), "ffmpeg on PATH for this test");
        assert_eq!(d.frame_urls.len(), 60, "{} frames split", d.frame_urls.len());
        assert!(std::fs::read(&d.frame_files[59]).unwrap().starts_with(&[0xFF, 0xD8]));
        let first = d.frame_files[0].file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(cache_file(&dir, &format!("/{first}")).map(|(_, m)| m), Some("image/jpeg"), "the protocol serves draft frames");
        // past the end is clamped by the engine to the timeline
        let tail = v.draft(&t, 2.0, 4.0).unwrap();
        assert!((tail.end - 3.0).abs() < 1e-6, "{tail:?}");
        let n = t.previews.load(Ordering::SeqCst);
        let again = v.draft(&t, 0.5, 2.0).unwrap();
        assert!(again.cached && again.file == d.file && t.previews.load(Ordering::SeqCst) == n, "same revision + range: no new render");
        eprintln!("viewer: 2 s half-res draft in {} ms (first frame {} ms), split into frames in {} ms", d.ms, d.first_frame_ms, d.extract_ms);
        sup.stop(); let _ = std::fs::remove_dir_all(&dir);
    }
}
