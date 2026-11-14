//! Automatic background proxies for heavy imports (docs/PROXY_WORKFLOW.md, "Automatic proxies").
//!
//! When a video is imported (or a project is loaded), heavy sources are queued for two disposable proxies, built one
//! source at a time on a single background worker so `import_media` returns at once:
//! - an EDIT proxy: a smaller intra-frame H.264 copy (long side `EDIT_MAX_SIDE`) that editing previews decode instead
//!   of the original (`proxy::preview_path` falls back to it when no explicit, enabled proxy applies);
//! - an ANALYSIS proxy: the short-GOP copy from analysis_proxy.rs, built ahead of time so random-access analysis and
//!   observe's draft renders find it ready instead of starting it on first use.
//!
//! "Heavy" means: about 4K or more pixels (`HEAVY_PIXELS`), long files (`LONG_SECONDS`), or long-GOP media whose own
//! keyframes are more than two seconds apart (phone footage), measured from packet flags without decoding.
//!
//! Nothing here touches the project: proxies are cache files keyed by source path + size + mtime, never undo steps.
//! Export, audio and high-bit stills never look at them (export prepares originals; see render::prepare_export_range).
//! Progress and state changes go out as `{"type":"proxy",...}` engine events (telemetry::event), which the desktop app
//! shows as a badge on each asset in the media bin. Off unless the host calls `set_enabled(true)` (kimo-mcp does);
//! `KIMO_AUTO_PROXY=off` turns it off, `KIMO_AUTO_PROXY=always` treats every video as heavy.
use crate::model::Project;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Long side of an edit proxy, in pixels (aspect ratio kept exactly, never upscaled).
pub const EDIT_MAX_SIDE: u32 = 1280;
/// Sources with at least this many pixels per frame (about 4K UHD and up) get proxies.
pub const HEAVY_PIXELS: u64 = 6_000_000;
/// Sources at least this long (seconds) get proxies.
pub const LONG_SECONDS: f64 = 600.0;
/// Bound on one source's frame-timestamp inspection (it decodes the whole file once, in the background).
const INSPECT_TIMEOUT: Duration = Duration::from_secs(4 * 3600);
/// The ffmpeg arguments that make it report progress on stdout, for `run_ffmpeg`. Put them before the output path.
pub const PROGRESS_ARGS: [&str; 3] = ["-progress", "pipe:1", "-nostats"];

static ON: AtomicBool = AtomicBool::new(false);

/// Turn automatic proxies on or off for this process (kimo-mcp turns them on at startup).
pub fn set_enabled(on: bool) {
    ON.store(on, Ordering::SeqCst);
}
fn mode() -> String {
    std::env::var("KIMO_AUTO_PROXY").unwrap_or_default().to_ascii_lowercase()
}
/// True when imports queue proxies and previews use the finished ones.
pub fn enabled() -> bool {
    ON.load(Ordering::SeqCst) && !matches!(mode().as_str(), "off" | "0" | "false")
}

/// Why a source with these properties is heavy, from what import already probed (no file access). Long-GOP is found
/// later, in the background, from the file itself.
pub fn heavy_reasons(width: u32, height: u32, duration: f64) -> Vec<&'static str> {
    let mut r = vec![];
    if width as u64 * height as u64 >= HEAVY_PIXELS {
        r.push("4k");
    }
    if duration >= LONG_SECONDS {
        r.push("long");
    }
    r
}

/// Where a source's background proxy work stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Waiting for the worker.
    Queued,
    /// Measuring keyframe spacing to decide whether the source is heavy.
    Checking,
    /// Encoding (`stage` says which proxy).
    Building,
    /// At least one proxy is ready; previews and analysis use it.
    Ready,
    /// Not heavy: previews decode the original, which is already cheap.
    Skipped,
    /// Heavy, but no proxy could be made for it (for example variable frame rate); previews use the original.
    Unsupported,
    /// A build failed; previews use the original.
    Failed,
}

/// One source's proxy job, as reported by events and `proxy_jobs`.
#[derive(Clone, Debug, Serialize)]
pub struct Job {
    /// Assets of the current project that use this source.
    pub asset_ids: BTreeSet<String>,
    /// The source file, resolved.
    pub path: String,
    pub state: State,
    /// 0..1 over the whole job (both proxies).
    pub progress: f64,
    /// "check", "edit" or "analysis" while working; "" otherwise.
    pub stage: &'static str,
    /// Why the source is heavy: "4k", "long", "long_gop" (and "forced" under KIMO_AUTO_PROXY=always).
    pub reasons: Vec<&'static str>,
    pub edit_proxy: Option<String>,
    pub analysis_proxy: Option<String>,
    /// Why one of the proxies was not made, when that is expected (variable frame rate, already small...).
    pub note: Option<String>,
    pub error: Option<String>,
    /// Wall-clock milliseconds the job took, once finished.
    pub ms: Option<u64>,
    #[serde(skip)]
    emitted: Option<(Instant, f64)>,
}

impl Job {
    fn new(path: &str, id: &str, reasons: Vec<&'static str>) -> Job {
        Job { asset_ids: BTreeSet::from([id.to_string()]), path: path.into(), state: State::Queued, progress: 0.0, stage: "", reasons, edit_proxy: None, analysis_proxy: None, note: None, error: None, ms: None, emitted: None }
    }
    fn active(&self) -> bool {
        matches!(self.state, State::Queued | State::Checking | State::Building)
    }
    /// The job as JSON (the event payload without its `type`).
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

struct Task {
    path: String,
    width: u32,
    height: u32,
    duration: f64,
    project_fps: f64,
    base_dir: Option<PathBuf>,
}

static JOBS: LazyLock<Mutex<BTreeMap<String, Job>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));
static QUEUE: LazyLock<(Mutex<VecDeque<Task>>, Condvar)> = LazyLock::new(|| (Mutex::new(VecDeque::new()), Condvar::new()));
static WORKER: OnceLock<bool> = OnceLock::new();
/// ffmpeg/ffprobe processes this module is running, so `shutdown` can stop them.
static RUNNING: LazyLock<Mutex<BTreeSet<u32>>> = LazyLock::new(|| Mutex::new(BTreeSet::new()));
static CACHE_OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);

fn jobs() -> std::sync::MutexGuard<'static, BTreeMap<String, Job>> {
    JOBS.lock().unwrap_or_else(|e| e.into_inner())
}

fn emit(j: &Job) {
    let mut v = j.to_json();
    v["type"] = json!("proxy");
    v["ts_ms"] = json!(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0));
    crate::telemetry::event(&v);
}

/// Update one job and send its event (progress-only updates at most every 250 ms or 2%).
fn update(path: &str, f: impl FnOnce(&mut Job)) {
    let mut g = jobs();
    let Some(j) = g.get_mut(path) else { return };
    let (state, stage) = (j.state, j.stage);
    f(j);
    let now = Instant::now();
    let changed = j.state != state || j.stage != stage;
    let due = match j.emitted { None => true, Some((t, p)) => now.duration_since(t) >= Duration::from_millis(250) || j.progress - p >= 0.02 };
    if changed || due {
        j.emitted = Some((now, j.progress));
        emit(j);
    }
}

/// Edit-proxy cache folder: `set_cache_dir`, else KIMO_PROXY_CACHE ("off" = no edit proxies), else the temp dir.
fn edit_cache_dir() -> Option<PathBuf> {
    if let Some(d) = CACHE_OVERRIDE.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Some(d);
    }
    match std::env::var("KIMO_PROXY_CACHE") {
        Ok(v) if v == "off" || v == "0" => None,
        Ok(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => Some(std::env::temp_dir().join("kimo_edit_proxy_cache")),
    }
}

/// Use this folder for edit proxies in this process (tests; hosts normally set KIMO_PROXY_CACHE). None = default.
pub fn set_cache_dir(dir: Option<PathBuf>) {
    *CACHE_OVERRIDE.lock().unwrap_or_else(|e| e.into_inner()) = dir;
}

/// Queue proxies for one just-imported asset when it is a heavy video. Returns the job (for the import result), or None
/// when automatic proxies are off or the asset is not a video. Never blocks on the media: it only reads what import
/// probed; the worker does the rest.
pub fn on_import(p: &Project, asset_id: &str) -> Option<Value> {
    if !enabled() {
        return None;
    }
    let a = p.assets.get(asset_id)?;
    if a.width == 0 || a.height == 0 || a.fps <= 0.0 || crate::render::is_still_path(&a.path) || a.path.ends_with(".kimopx") {
        return None;
    }
    let path = p.resolve(&a.path);
    let mut reasons = heavy_reasons(a.width, a.height, a.duration);
    if reasons.is_empty() && mode() == "always" {
        reasons.push("forced");
    }
    let mut g = jobs();
    if let Some(j) = g.get_mut(&path).filter(|j| j.state != State::Failed) {
        j.asset_ids.insert(asset_id.to_string());
        emit(j);
        return Some(j.to_json());
    }
    let j = Job::new(&path, asset_id, reasons);
    let out = j.to_json();
    emit(&j);
    g.insert(path.clone(), j);
    drop(g);
    let task = Task { path, width: a.width, height: a.height, duration: a.duration, project_fps: p.fps, base_dir: p.base_dir.clone() };
    let (q, cv) = &*QUEUE;
    q.lock().unwrap_or_else(|e| e.into_inner()).push_back(task);
    cv.notify_one();
    start_worker();
    Some(out)
}

/// After a project load: jobs now belong to this project's assets (ids are per project), and its heavy media is queued
/// (sources with finished proxies are found in the cache within a moment).
pub fn on_project_loaded(p: &Project) {
    if !enabled() {
        return;
    }
    for j in jobs().values_mut() {
        j.asset_ids.clear();
    }
    let ids: Vec<String> = p.assets.keys().cloned().collect();
    for id in ids {
        on_import(p, &id);
    }
}

/// The current project's proxy jobs, one per source, with this project's asset ids.
pub fn list(p: &Project) -> Vec<Value> {
    let mut by_path: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, a) in &p.assets {
        by_path.entry(p.resolve(&a.path)).or_default().push(id.clone());
    }
    let g = jobs();
    by_path
        .into_iter()
        .filter_map(|(path, mut ids)| {
            let j = g.get(&path)?;
            ids.sort();
            let mut v = j.to_json();
            v["asset_ids"] = json!(ids);
            Some(v)
        })
        .collect()
}

/// Block until no proxy job is waiting or running, or `timeout` passes. Returns true when idle (tests, shutdown).
pub fn wait_idle(timeout: Duration) -> bool {
    let t0 = Instant::now();
    loop {
        let busy = !QUEUE.0.lock().unwrap_or_else(|e| e.into_inner()).is_empty() || jobs().values().any(Job::active);
        if !busy {
            return true;
        }
        if t0.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Stop: drop queued work and kill the encoder that is running (its temp file is removed when the worker sees it fail).
/// kimo-mcp calls this on its way out so no ffmpeg outlives the engine.
pub fn shutdown() {
    QUEUE.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let pids: Vec<u32> = RUNNING.lock().unwrap_or_else(|e| e.into_inner()).iter().copied().collect();
    for pid in pids {
        kill(pid);
    }
}

fn kill(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill").args(["/T", "/F", "/PID", &pid.to_string()]).creation_flags(0x0800_0000).status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
}

fn start_worker() {
    WORKER.get_or_init(|| {
        // The proxy worker. Purpose: build queued proxies one source at a time, off every tool call's thread.
        // Lifecycle: started at most once per process, on the first heavy import; it sleeps on the queue's condvar when
        // idle and lives until the process exits. Join/cancel owner: detached by design, nothing joins it; `shutdown`
        // empties the queue and kills its running encoder. It runs under its own never-cancelled token (not the importing
        // call's), so a tool call's time limit cannot cancel it; a panic in one job (memory budget) fails only that job.
        let tok = crate::guard::new_token();
        crate::guard::with_token(&tok, || {
            crate::guard::spawn_builder_with_token(std::thread::Builder::new().name("kimo-proxy-queue".into()), worker).is_ok()
        })
    });
}

fn worker() {
    loop {
        let task = {
            let (q, cv) = &*QUEUE;
            let mut g = q.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(t) = g.pop_front() {
                    break t;
                }
                g = cv.wait(g).unwrap_or_else(|e| e.into_inner());
            }
        };
        let path = task.path.clone();
        let t0 = Instant::now();
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&task))).is_err() {
            update(&path, |j| {
                j.state = State::Failed;
                j.stage = "";
                j.error = Some("the proxy build stopped unexpectedly".into());
            });
        }
        update(&path, |j| j.ms = Some(t0.elapsed().as_millis() as u64));
    }
}

enum EditErr {
    /// Expected: this source cannot or need not have an edit proxy.
    Unsupported(String),
    Failed(String),
}

fn run(t: &Task) {
    let path = t.path.as_str();
    update(path, |j| {
        j.state = State::Checking;
        j.stage = "check";
    });
    let gap = packet_keyframe_gap(path, t.duration).ok();
    let long_gop = gap.is_some_and(crate::analysis_proxy::worth_building);
    let mut reasons = jobs().get(path).map(|j| j.reasons.clone()).unwrap_or_default();
    if long_gop && !reasons.contains(&"long_gop") {
        reasons.push("long_gop");
    }
    if reasons.is_empty() {
        update(path, |j| {
            j.state = State::Skipped;
            j.stage = "";
            j.progress = 1.0;
            j.note = Some("not heavy: previews decode the original".into());
        });
        return;
    }
    let edit_share = if long_gop { 0.6 } else { 1.0 };
    update(path, |j| {
        j.reasons = reasons.clone();
        j.state = State::Building;
        j.stage = "edit";
    });
    let mut notes: Vec<String> = vec![];
    let mut error: Option<String> = None;
    let edit = match build_edit(t, &mut |f| update(path, |j| j.progress = (f * edit_share).clamp(0.0, 1.0))) {
        Ok(p) => Some(p),
        Err(EditErr::Unsupported(m)) => {
            notes.push(format!("no edit proxy: {m}"));
            None
        }
        Err(EditErr::Failed(m)) => {
            error = Some(format!("edit proxy: {m}"));
            None
        }
    };
    update(path, |j| {
        j.edit_proxy = edit.clone();
        j.progress = edit_share;
    });
    let mut analysis = None;
    if long_gop {
        update(path, |j| j.stage = "analysis");
        match crate::analysis_proxy::prebuild(t.base_dir.as_deref(), path, t.duration, t.project_fps, gap, &mut |f| update(path, |j| j.progress = (edit_share + f * (1.0 - edit_share)).clamp(0.0, 1.0))) {
            Ok(ap) => analysis = Some(ap.path),
            Err(m) => notes.push(format!("no analysis proxy: {m}")),
        }
    }
    update(path, |j| {
        j.analysis_proxy = analysis.clone();
        j.stage = "";
        j.note = (!notes.is_empty()).then(|| notes.join("; "));
        j.error = error.clone();
        if j.edit_proxy.is_some() || j.analysis_proxy.is_some() {
            j.state = State::Ready;
            j.progress = 1.0;
        } else if j.error.is_some() {
            j.state = State::Failed;
        } else {
            j.state = State::Unsupported;
        }
    });
}

fn cache_name(source: &str, key: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut h);
    key.hash(&mut h);
    format!("{:016x}.mp4", h.finish()) // the shape analysis_proxy::prune_cache recognises
}

/// The edit proxy file for this source as it is now (path + size + mtime + proxy size), or None (cache off, no source).
fn edit_slot(source: &str) -> Option<(PathBuf, String)> {
    let dir = edit_cache_dir()?;
    let st = crate::proxy::stamp(Path::new(source)).ok()?;
    let key = format!("{}-{}-{}", st.bytes, st.modified_ns, EDIT_MAX_SIDE);
    Some((dir.join(cache_name(source, &key)), key))
}

/// Sidecar facts of one finished edit proxy: (proxy bytes, source width, source height).
type Facts = (u64, u32, u32);
static FACTS: LazyLock<Mutex<HashMap<PathBuf, (String, Facts)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The finished edit proxy at `file` for source `key`, when its sidecar matches and the file is intact (same size).
fn edit_cached(file: &Path, key: &str) -> Option<Facts> {
    let bytes = crate::proxy::stamp(file).ok()?.bytes;
    if let Some((k, f)) = FACTS.lock().unwrap_or_else(|e| e.into_inner()).get(file) {
        if k == key && f.0 == bytes {
            return Some(*f);
        }
    }
    let v: Value = serde_json::from_str(&std::fs::read_to_string(file.with_extension("json")).ok()?).ok()?;
    if v["source_key"].as_str() != Some(key) || v["bytes"].as_u64() != Some(bytes) {
        return None;
    }
    let f = (bytes, v["source_width"].as_u64()? as u32, v["source_height"].as_u64()? as u32);
    FACTS.lock().unwrap_or_else(|e| e.into_inner()).insert(file.to_path_buf(), (key.to_string(), f));
    Some(f)
}

/// The ready edit proxy for one asset, for editing previews only. Export, audio, analysis and high-bit stills never
/// call this (proxy::preview_path is only consulted by render::prepare_preview_range). Cheap: two stats and, the first
/// time, one small sidecar read. Managed-color projects and changed sources get None, so previews use the original.
pub fn preview_path(p: &Project, asset_id: &str) -> Option<String> {
    if !enabled() || p.color_pipeline.enabled {
        return None;
    }
    let a = p.assets.get(asset_id)?;
    if a.width == 0 {
        return None;
    }
    let source = p.resolve(&a.path);
    let (file, key) = edit_slot(&source)?;
    let (_, w, h) = edit_cached(&file, &key)?;
    (w == a.width && h == a.height).then(|| file.to_string_lossy().into_owned())
}

fn build_edit(t: &Task, progress: &mut dyn FnMut(f64)) -> Result<String, EditErr> {
    use EditErr::*;
    let src = Path::new(&t.path);
    let (file, key) = edit_slot(&t.path).ok_or_else(|| Unsupported("the edit proxy cache is off (KIMO_PROXY_CACHE=off)".into()))?;
    if edit_cached(&file, &key).is_some() {
        return Ok(file.to_string_lossy().into_owned());
    }
    let before = crate::proxy::stamp(src).map_err(Failed)?;
    let v = crate::proxy::inspect(src, INSPECT_TIMEOUT).map_err(Unsupported)?;
    if v.width != t.width || v.height != t.height {
        return Err(Unsupported("the file no longer matches the imported asset; reimport it".into()));
    }
    let (w, h) = crate::proxy::dimensions(v.width, v.height, EDIT_MAX_SIDE).map_err(Unsupported)?;
    if w >= v.width {
        return Err(Unsupported(format!("the source is already {}x{}, no larger than a proxy", v.width, v.height)));
    }
    let dir = file.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| Failed(format!("proxy cache folder: {e}")))?;
    let tmp = file.with_extension("mp4.part");
    let _ = std::fs::remove_file(&tmp);
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-nostdin", "-y"]).args(PROGRESS_ARGS).args(["-noautorotate", "-i"]).arg(src).args([
        "-map", "0:v:0", "-an", "-sn", "-dn", "-map_metadata", "-1",
        "-vf", &format!("scale={w}:{h}:flags=fast_bilinear,setsar=1"),
        "-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-g", "1", "-bf", "0", "-pix_fmt", "yuv420p",
        "-r", &format!("{}/{}", v.num, v.den), "-fps_mode", "cfr", "-video_track_timescale", &v.num.to_string(), "-f", "mp4",
    ]).arg(&tmp);
    let done = (|| {
        run_ffmpeg(&mut cmd, Measure::Frames(v.frames as u64), progress)?;
        let e = crate::proxy::inspect(&tmp, INSPECT_TIMEOUT)?;
        if e.frames != v.frames || e.width != w || e.height != h || (e.num as f64 / e.den as f64 - v.num as f64 / v.den as f64).abs() > 1e-6 {
            return Err("the encoded proxy failed its frame-count/rate/size check".to_string());
        }
        if crate::proxy::stamp(src)? != before {
            return Err("the source changed while its proxy was built; import it again to retry".to_string());
        }
        std::fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
        let bytes = crate::proxy::stamp(&file)?.bytes;
        let side = json!({"source_key": key, "bytes": bytes, "source_width": v.width, "source_height": v.height, "width": w, "height": h, "frames": v.frames, "fps_num": v.num, "fps_den": v.den});
        std::fs::write(file.with_extension("json"), side.to_string()).map_err(|e| e.to_string())
    })();
    if let Err(e) = done {
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(&file);
        return Err(Failed(e));
    }
    let max_mb = std::env::var("KIMO_PROXY_CACHE_MAX_MB").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(20 * 1024);
    crate::analysis_proxy::prune_cache(dir, max_mb * 1024 * 1024);
    if !file.is_file() {
        return Err(Unsupported("the proxy is larger than the whole proxy cache budget (KIMO_PROXY_CACHE_MAX_MB)".into()));
    }
    Ok(file.to_string_lossy().into_owned())
}

/// The largest gap between this source's keyframes (lead-in and tail included), from packet flags only: no decoding,
/// so it costs one read of the file. Same measure as analysis_proxy::max_keyframe_gap, which decodes.
pub fn packet_keyframe_gap(path: &str, duration: f64) -> Result<f64, String> {
    let mut cmd = Command::new("ffprobe");
    cmd.args(["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts_time,flags", "-of", "csv=p=0", path]);
    let out = output(&mut cmd)?;
    let mut times: Vec<f64> = vec![0.0];
    for l in out.lines() {
        let mut it = l.split(',');
        if let (Some(t), Some(f)) = (it.next(), it.next()) {
            if f.contains('K') {
                if let Ok(v) = t.trim().parse::<f64>() {
                    times.push(v);
                }
            }
        }
    }
    if times.len() < 2 {
        return Err("no keyframes found".into());
    }
    times.sort_by(f64::total_cmp);
    let tail = duration.max(*times.last().unwrap_or(&0.0));
    times.push(tail);
    Ok(times.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max))
}

fn quiet(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000 | 0x0000_4000); // CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS: editing stays responsive
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

fn output(cmd: &mut Command) -> Result<String, String> {
    quiet(cmd);
    let child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("ffprobe: {e}"))?;
    let pid = child.id();
    RUNNING.lock().unwrap_or_else(|e| e.into_inner()).insert(pid);
    let r = child.wait_with_output();
    RUNNING.lock().unwrap_or_else(|e| e.into_inner()).remove(&pid);
    let o = r.map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// What `run_ffmpeg` measures progress against.
pub enum Measure {
    /// Output frames expected.
    Frames(u64),
    /// Output duration expected, seconds.
    Seconds(f64),
}

/// Run an ffmpeg command that includes `PROGRESS_ARGS`, calling `progress(0..1)` as it reports, at reduced priority.
/// Errors carry ffmpeg's own message. The process is listed for `shutdown` while it runs.
pub fn run_ffmpeg(cmd: &mut Command, m: Measure, progress: &mut dyn FnMut(f64)) -> Result<(), String> {
    quiet(cmd);
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("ffmpeg: {e}"))?;
    let pid = child.id();
    RUNNING.lock().unwrap_or_else(|e| e.into_inner()).insert(pid);
    let (out, mut err) = (child.stdout.take(), child.stderr.take());
    let mut msg = String::new();
    std::thread::scope(|sc| {
        let h = crate::guard::scope_spawn(sc, move || {
            let mut s = String::new();
            if let Some(e) = err.as_mut() {
                let _ = e.read_to_string(&mut s);
            }
            s
        });
        if let Some(o) = out {
            for line in BufReader::new(o).lines().map_while(Result::ok) {
                let frac = match (&m, line.split_once('=')) {
                    (Measure::Frames(n), Some(("frame", v))) if *n > 0 => v.trim().parse::<f64>().ok().map(|f| f / *n as f64),
                    (Measure::Seconds(d), Some(("out_time_us", v))) if *d > 0.0 => v.trim().parse::<f64>().ok().map(|us| us / 1e6 / d),
                    _ => None,
                };
                if let Some(f) = frac.filter(|f| f.is_finite()) {
                    progress(f.clamp(0.0, 1.0));
                }
            }
        }
        msg = h.join().unwrap_or_default();
    });
    let status = child.wait();
    RUNNING.lock().unwrap_or_else(|e| e.into_inner()).remove(&pid);
    match status {
        Ok(s) if s.success() => {
            progress(1.0);
            Ok(())
        }
        Ok(_) => Err(if msg.trim().is_empty() { "ffmpeg failed".into() } else { msg.trim().lines().last().unwrap_or("ffmpeg failed").to_string() }),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn heavy_means_4k_or_long() {
        assert_eq!(heavy_reasons(3840, 2160, 5.0), vec!["4k"]);
        assert_eq!(heavy_reasons(4096, 2160, 5.0), vec!["4k"]);
        assert!(heavy_reasons(2560, 1440, 5.0).is_empty(), "1440p is not heavy by size");
        assert!(heavy_reasons(1920, 1080, 599.0).is_empty());
        assert_eq!(heavy_reasons(1920, 1080, 600.0), vec!["long"]);
        assert_eq!(heavy_reasons(3840, 2160, 900.0), vec!["4k", "long"]);
    }
    #[test]
    fn cache_names_are_prunable_and_keyed_by_stamp() {
        let a = cache_name("/m/a.mp4", "1-2-1280");
        assert_eq!(a.len(), 20);
        assert!(a.ends_with(".mp4") && a[..16].chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, cache_name("/m/a.mp4", "1-3-1280"), "a changed source gets a new proxy");
    }
}
