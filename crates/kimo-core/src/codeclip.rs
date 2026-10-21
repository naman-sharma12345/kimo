//! Code clips: a small web project (index.html + JS, e.g. three.js vendored inside the folder) that draws the exact frame for time
//! `t` through `window.renderFrame(t, params)`. Kimo renders it per frame in a sandboxed headless browser (the sidecar in
//! codeclip_sidecar.py, embedded in this binary), caches each frame under a key of (every project file, params, t, size), and
//! from then on the frames are footage: cut, keyed, graded, masked, glowed, blended, track-matted and composited in 3D like media.
//!
//! Contract for the project: `window.renderFrame(t, params)` (may return a Promise) draws the frame for clip time `t` seconds
//! for any `t` in any order; no wall clock, no unseeded randomness, no state carried between frames. Optional `kimo.json`
//! declares `{"entry": "index.html", "params": {"speed": {"default": 1, "min": 0, "max": 4}}}`. `check_code_clip_determinism`
//! verifies the contract. Rendering needs python3 with playwright + Pillow and a Chromium (KIMO_CHROMIUM, else Playwright's
//! headless shell); KIMO_PYTHON picks the interpreter, KIMO_CODECLIP_CACHE the disk cache dir ("off" disables it),
//! KIMO_CODECLIP_CACHE_MAX_MB its size cap (default 256 MB, least recently used frames evicted), KIMO_CODECLIP_LOCK a file
//! that serializes browser launches, KIMO_CODECLIP_FRAME_TIMEOUT_MS the per-frame timeout (default 20000).
use crate::keyframe::Prop;
use crate::model::{Clip, Project};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SIDECAR: &str = include_str!("codeclip_sidecar.py");
/// Most files / bytes a code clip folder may hold (the whole folder is hashed into every frame key).
pub const MAX_FILES: usize = 4000;
pub const MAX_BYTES: u64 = 256 * 1024 * 1024;

fn d_entry() -> String { "index.html".into() }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeClip {
    /// Project folder: absolute, or relative to the project file.
    pub dir: String,
    /// Page inside `dir` that defines `window.renderFrame`.
    #[serde(default = "d_entry")] pub entry: String,
    /// Viewport the page renders at, in pixels. A size other than the canvas is scaled to fit it (aspect kept, transparent bars).
    pub width: u32,
    pub height: u32,
    /// Declared numeric params, keyframeable on the code clip's own clock (the t renderFrame gets), handed to renderFrame(t, params).
    #[serde(default)] pub params: BTreeMap<String, Prop>,
    /// Declared [min, max] per param (from kimo.json); values outside are refused.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")] pub ranges: BTreeMap<String, [f64; 2]>,
}

/// kimo.json, the optional manifest inside a code clip folder.
#[derive(Clone, Debug, Default)]
pub struct Manifest { pub entry: String, pub params: BTreeMap<String, (f64, Option<[f64; 2]>)> }

pub fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let mut m = Manifest { entry: d_entry(), params: BTreeMap::new() };
    let f = dir.join("kimo.json");
    if !f.exists() { return Ok(m); }
    let txt = std::fs::read_to_string(&f).map_err(|e| format!("invalid_source: cannot read {}: {e}", f.display()))?;
    let v: serde_json::Value = serde_json::from_str(&txt).map_err(|e| format!("invalid_source: {} is not valid JSON: {e}", f.display()))?;
    if let Some(e) = v.get("entry").and_then(|x| x.as_str()) { m.entry = e.to_string(); }
    if let Some(ps) = v.get("params").and_then(|x| x.as_object()) {
        for (k, d) in ps {
            check_param_name(k)?;
            let (def, r) = match d {
                serde_json::Value::Number(n) => (n.as_f64().unwrap_or(0.0), None),
                serde_json::Value::Object(o) => {
                    let g = |key: &str| o.get(key).and_then(|x| x.as_f64());
                    let r = match (g("min"), g("max")) { (Some(a), Some(b)) if a <= b => Some([a, b]), (None, None) => None, _ => return Err(format!("invalid_source: kimo.json param '{k}' needs both min and max with min <= max")) };
                    (g("default").unwrap_or(r.map(|r| r[0]).unwrap_or(0.0)), r) }
                _ => return Err(format!("invalid_source: kimo.json param '{k}' must be a number or {{default, min, max}}")),
            };
            if !def.is_finite() { return Err(format!("invalid_source: kimo.json param '{k}' default is not finite")); }
            m.params.insert(k.clone(), (def, r));
        }
    }
    Ok(m)
}

pub fn check_param_name(k: &str) -> Result<(), String> {
    if k.is_empty() || k.len() > 32 || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') { return Err(format!("invalid_source: param name '{k}' must be 1-32 characters of A-Z a-z 0-9 _")); }
    Ok(())
}

/// The folder a stored `dir` points at, resolved against the project file.
pub fn resolve_dir(p: &Project, cc: &CodeClip) -> PathBuf { PathBuf::from(p.resolve(&cc.dir)) }

/// Refuse a folder that cannot be a code clip (missing, no entry page, entry escaping the folder).
pub fn validate_dir(dir: &Path, entry: &str) -> Result<(), String> {
    if !dir.is_dir() { return Err(format!("invalid_source: code clip folder {} does not exist", dir.display())); }
    let root = dir.canonicalize().map_err(|e| format!("invalid_source: {}: {e}", dir.display()))?;
    let f = root.join(entry);
    let ok = f.canonicalize().map(|c| c.starts_with(&root) && c.is_file()).unwrap_or(false);
    if !ok { return Err(format!("invalid_source: code clip folder {} has no entry page '{entry}' inside it", dir.display())); }
    Ok(())
}

fn fnv128(h: &mut u128, b: &[u8]) { for x in b { *h ^= *x as u128; *h = h.wrapping_mul(0x0000000001000000000000000000013B); } }
const FNV128_OFF: u128 = 0x6c62272e07bb014262b821756295c58d;

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf, std::fs::Metadata)>) -> Result<(), String> {
    let mut ents: Vec<_> = std::fs::read_dir(dir).map_err(|e| format!("invalid_source: {}: {e}", dir.display()))?.filter_map(|e| e.ok()).collect();
    ents.sort_by_key(|e| e.file_name());
    for e in ents {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') { continue; } // .git, .kimo_cache, editor files: not part of the page
        let p = e.path(); let md = std::fs::metadata(&p).map_err(|e| format!("invalid_source: {}: {e}", p.display()))?;
        if md.is_dir() { walk(root, &p, out)?; } else if md.is_file() {
            let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            out.push((rel, p, md));
            if out.len() > MAX_FILES { return Err(format!("limit_exceeded: code clip folder {} holds more than {MAX_FILES} files", root.display())); }
        }
    }
    Ok(())
}

fn tree_memo() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, (u128, u128)>> {
    static M: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<PathBuf, (u128, u128)>>> = std::sync::OnceLock::new();
    M.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Content hash of every (non-dot) file in the folder: relative path + bytes. Memoized on (path, size, mtime) of every file,
/// so an edit to any file changes every frame key and nothing stale is served.
pub fn tree_hash(dir: &Path) -> Result<u128, String> {
    let mut files = vec![]; walk(dir, dir, &mut files)?;
    let mut quick = FNV128_OFF; let mut total = 0u64;
    for (rel, _, md) in &files {
        total += md.len();
        let mt = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
        fnv128(&mut quick, rel.as_bytes()); fnv128(&mut quick, &md.len().to_le_bytes()); fnv128(&mut quick, &mt.to_le_bytes());
    }
    if total > MAX_BYTES { return Err(format!("limit_exceeded: code clip folder {} holds {total} bytes (max {MAX_BYTES})", dir.display())); }
    let key = dir.to_path_buf();
    if let Some((q, h)) = tree_memo().lock().ok().and_then(|m| m.get(&key).copied()) { if q == quick { return Ok(h); } }
    let mut h = FNV128_OFF;
    for (rel, p, _) in &files {
        let b = std::fs::read(p).map_err(|e| format!("invalid_source: {}: {e}", p.display()))?;
        fnv128(&mut h, rel.as_bytes()); fnv128(&mut h, &[0]); fnv128(&mut h, &(b.len() as u64).to_le_bytes()); fnv128(&mut h, &b);
    }
    if let Ok(mut m) = tree_memo().lock() { m.insert(key, (quick, h)); }
    Ok(h)
}

/// One frame to draw: code time and the param values at that frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample { pub t: f64, pub params: BTreeMap<String, f64> }

/// Round to the microsecond so keys and the page see the same t on every machine.
fn q6(t: f64) -> f64 { (t * 1e6).round() / 1e6 }

/// Frame slot `k` of a clip: clip-local display time quantized to the project frame grid (every lt in [k/fps, (k+1)/fps)
/// shows slot k). Its code time follows the clip's in-point, speed, speed ramp or time remap, so split/trim/retime work.
pub fn slot_of(lt: f64, fps: f64) -> usize { (lt.max(0.0) * fps + 1e-6).floor() as usize }
pub fn slot_count(c: &Clip, fps: f64) -> usize { ((c.duration * fps - 1e-6).ceil() as usize).max(1) }
pub fn sample(c: &Clip, cc: &CodeClip, k: usize, fps: f64) -> Sample {
    let lt = k as f64 / fps;
    let src = c.remap_source(lt).unwrap_or_else(|| c.source_time(lt));
    let t = c.in_point + src;
    // Param keys live on the code clip's own clock (the t renderFrame gets), so a split, trim or retime keeps the animation seamless.
    let params = cc.params.iter().map(|(n, p)| (n.clone(), q6(p.at(t)))).collect();
    Sample { t: q6(t), params }
}

/// Frame key: folder content, entry page, code time, params at that frame, render size and canvas size.
pub fn frame_key(tree: u128, cc: &CodeClip, s: &Sample, canvas: (u32, u32)) -> u128 {
    let mut h = FNV128_OFF;
    fnv128(&mut h, b"kimo-codeclip-v1"); fnv128(&mut h, &tree.to_le_bytes()); fnv128(&mut h, cc.entry.as_bytes()); fnv128(&mut h, &[0]);
    fnv128(&mut h, &s.t.to_bits().to_le_bytes());
    for (n, v) in &s.params { fnv128(&mut h, n.as_bytes()); fnv128(&mut h, &[0]); fnv128(&mut h, &v.to_bits().to_le_bytes()); }
    for v in [cc.width, cc.height, canvas.0, canvas.1] { fnv128(&mut h, &v.to_le_bytes()); }
    h
}

/// Disk cache folder: KIMO_CODECLIP_CACHE ("off" = none), else <project dir>/.kimo_cache/codeclip, else the temp dir.
pub fn cache_dir(p: &Project) -> Option<PathBuf> {
    match std::env::var("KIMO_CODECLIP_CACHE") { Ok(v) if v == "off" || v == "0" => return None, Ok(v) if !v.is_empty() => return Some(PathBuf::from(v)), _ => {} }
    Some(match &p.base_dir { Some(b) => b.join(".kimo_cache").join("codeclip"), None => std::env::temp_dir().join("kimo_codeclip_cache") })
}
fn cache_file(dir: &Path, key: u128) -> PathBuf { dir.join(format!("{key:032x}.tif")) }

/// Disk cache cap in bytes: KIMO_CODECLIP_CACHE_MAX_MB (default 256). Past it the least recently used frames are deleted.
pub fn cache_max_bytes() -> u64 { std::env::var("KIMO_CODECLIP_CACHE_MAX_MB").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(256) * 1024 * 1024 }

/// Delete least recently used cache frames (oldest mtime first; a hit refreshes mtime) until the folder is under `max` bytes.
/// Only this cache's own files (32 hex digits + .tif, and stale .part files) are touched. Returns (files, bytes) left.
pub fn prune_cache(dir: &Path, max: u64) -> (usize, u64) {
    let Ok(rd) = std::fs::read_dir(dir) else { return (0, 0) };
    let mut v: Vec<(std::time::SystemTime, u64, PathBuf)> = vec![];
    for e in rd.filter_map(|e| e.ok()) {
        let name = e.file_name().to_string_lossy().into_owned();
        let ours = name.len() == 36 && name.ends_with(".tif") && name[..32].chars().all(|c| c.is_ascii_hexdigit());
        let part = name.len() > 36 && name[..32].chars().all(|c| c.is_ascii_hexdigit()) && name.ends_with(".part");
        let Ok(md) = e.metadata() else { continue };
        let mt = md.modified().unwrap_or(std::time::UNIX_EPOCH);
        if part { if mt.elapsed().map(|d| d.as_secs() > 600).unwrap_or(false) { let _ = std::fs::remove_file(e.path()); } continue; }
        if ours { v.push((mt, md.len(), e.path())); }
    }
    let mut total: u64 = v.iter().map(|x| x.1).sum();
    v.sort();
    let mut left = v.len();
    for (_, n, p) in &v { if total <= max { break; } if std::fs::remove_file(p).is_ok() { total -= n; left -= 1; } }
    (left, total)
}

pub fn read_cached(dir: &Path, key: u128, w: u32, h: u32) -> Option<Vec<u8>> {
    use tiff::decoder::{Decoder, DecodingResult};
    let f = std::fs::File::open(cache_file(dir, key)).ok()?;
    let _ = f.set_modified(std::time::SystemTime::now()); // LRU: a hit is a use
    let mut d = Decoder::new(std::io::BufReader::new(f)).ok()?;
    if d.dimensions().ok()? != (w, h) || !matches!(d.colortype().ok()?, tiff::ColorType::RGBA(8)) { return None; }
    match d.read_image().ok()? { DecodingResult::U8(v) if v.len() == (w * h * 4) as usize => Some(v), _ => None }
}

pub fn write_cached(dir: &Path, key: u128, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    use tiff::encoder::{colortype::RGBA8, Compression, DeflateLevel, TiffEncoder};
    if rgba.len() as u64 > cache_max_bytes() { return Ok(()); } // one frame over the cap: never cached
    std::fs::create_dir_all(dir).map_err(|e| format!("code clip cache {}: {e}", dir.display()))?;
    let fin = cache_file(dir, key); let tmp = fin.with_extension(format!("tif.{}.part", std::process::id()));
    let r = (|| -> Result<(), String> {
        let f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        let mut enc = TiffEncoder::new(std::io::BufWriter::new(f)).map_err(|e| e.to_string())?.with_compression(Compression::Deflate(DeflateLevel::Fast));
        enc.write_image::<RGBA8>(w, h, rgba).map_err(|e| e.to_string())?; Ok(()) })();
    match r { Ok(()) => std::fs::rename(&tmp, &fin).map_err(|e| e.to_string()), Err(e) => { let _ = std::fs::remove_file(&tmp); Err(format!("code clip cache write: {e}")) } }
}

/// Scale a straight-alpha RGBA8 frame of size (w, h) into a (cw, ch) canvas, aspect kept, centred, transparent bars (bilinear
/// on premultiplied values). Same size = unchanged bytes.
pub fn fit(src: &[u8], w: usize, h: usize, cw: usize, ch: usize) -> Vec<u8> {
    if (w, h) == (cw, ch) { return src.to_vec(); }
    let s = (cw as f64 / w as f64).min(ch as f64 / h as f64);
    let (dw, dh) = (w as f64 * s, h as f64 * s); let (ox, oy) = ((cw as f64 - dw) / 2.0, (ch as f64 - dh) / 2.0);
    let mut out = vec![0u8; cw * ch * 4];
    let px = |x: isize, y: isize| -> [f64; 4] { let x = x.clamp(0, w as isize - 1) as usize; let y = y.clamp(0, h as isize - 1) as usize; let i = (y * w + x) * 4; let a = src[i + 3] as f64 / 255.0; [src[i] as f64 * a, src[i + 1] as f64 * a, src[i + 2] as f64 * a, a] };
    for y in 0..ch { for x in 0..cw {
        let (fx, fy) = ((x as f64 + 0.5 - ox) / s - 0.5, (y as f64 + 0.5 - oy) / s - 0.5);
        if fx < -0.5 || fy < -0.5 || fx > w as f64 - 0.5 || fy > h as f64 - 0.5 { continue; }
        let (x0, y0) = (fx.floor(), fy.floor()); let (tx, ty) = (fx - x0, fy - y0); let (x0, y0) = (x0 as isize, y0 as isize);
        let (a, b, c, d) = (px(x0, y0), px(x0 + 1, y0), px(x0, y0 + 1), px(x0 + 1, y0 + 1));
        let mut v = [0.0; 4]; for j in 0..4 { v[j] = (a[j] * (1.0 - tx) + b[j] * tx) * (1.0 - ty) + (c[j] * (1.0 - tx) + d[j] * tx) * ty; }
        let o = (y * cw + x) * 4; let al = v[3];
        if al > 1e-9 { for j in 0..3 { out[o + j] = (v[j] / al).round().clamp(0.0, 255.0) as u8; } }
        out[o + 3] = (al * 255.0).round().clamp(0.0, 255.0) as u8;
    } }
    out
}

/// A step of a sidecar job: draw a frame, or reload the page (fresh JS state).
#[derive(Clone, Debug)]
pub enum Step { Frame(Sample), Reload }

/// What a sidecar run reports besides the frames.
#[derive(Clone, Debug, Default)]
pub struct RunInfo { pub frame_ms: Vec<f64>, pub blocked: Vec<String>, pub gl: String, pub wall_s: f64 }

pub fn frame_timeout_ms() -> u64 { std::env::var("KIMO_CODECLIP_FRAME_TIMEOUT_MS").ok().and_then(|v| v.parse().ok()).filter(|v| *v >= 100).unwrap_or(20_000) }
const LOAD_TIMEOUT_MS: u64 = 60_000;

/// Render steps through the sandboxed headless browser. Returns raw straight-alpha RGBA8 frames at (cc.width, cc.height),
/// one per Frame step, in order. Every page load and every frame has a timeout; past the whole-job deadline the sidecar's
/// process group (python, the driver and Chromium) is killed by pid.
pub fn render_steps(dir: &Path, cc: &CodeClip, steps: &[Step]) -> Result<(Vec<Vec<u8>>, RunInfo), String> {
    validate_dir(dir, &cc.entry)?;
    let nframes = steps.iter().filter(|s| matches!(s, Step::Frame(_))).count();
    let nreload = steps.len() - nframes;
    let fms = frame_timeout_ms();
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let job_dir = std::env::temp_dir().join(format!("kimo_codeclip_{}_{}", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    std::fs::create_dir_all(&job_dir).map_err(|e| format!("renderer_failed: temp dir: {e}"))?;
    let cleanup = |r| { let _ = std::fs::remove_dir_all(&job_dir); r };
    let script = job_dir.join("sidecar.py");
    if let Err(e) = std::fs::write(&script, SIDECAR) { return cleanup(Err(format!("renderer_failed: {e}"))); }
    let mut js = vec![]; let mut outs = vec![];
    for (i, s) in steps.iter().enumerate() { match s {
        Step::Reload => js.push(serde_json::json!({"reload": true})),
        Step::Frame(s) => { let o = job_dir.join(format!("f{i:05}.rgba")); js.push(serde_json::json!({"t": s.t, "params": s.params, "out": o})); outs.push(o); } } }
    let root = match dir.canonicalize() { Ok(r) => r, Err(e) => return cleanup(Err(format!("invalid_source: {e}"))) };
    let job = serde_json::json!({"dir": root, "entry": cc.entry, "width": cc.width, "height": cc.height, "frame_timeout_ms": fms, "load_timeout_ms": LOAD_TIMEOUT_MS,
        "chromium": std::env::var("KIMO_CHROMIUM").unwrap_or_default(), "lock": std::env::var("KIMO_CODECLIP_LOCK").ok(), "steps": js});
    let jf = job_dir.join("job.json");
    if let Err(e) = std::fs::write(&jf, job.to_string()) { return cleanup(Err(format!("renderer_failed: {e}"))); }
    let (so, se) = (job_dir.join("out.jsonl"), job_dir.join("err.log"));
    let (fo, fe) = match (std::fs::File::create(&so), std::fs::File::create(&se)) { (Ok(a), Ok(b)) => (a, b), _ => return cleanup(Err("renderer_failed: cannot create sidecar log files".into())) };
    let py = std::env::var("KIMO_PYTHON").unwrap_or_else(|_| "python3".into());
    let mut cmd = std::process::Command::new(&py);
    cmd.arg(&script).arg(&jf).stdin(std::process::Stdio::null()).stdout(fo).stderr(fe);
    { use std::os::unix::process::CommandExt; cmd.process_group(0); }
    let t0 = std::time::Instant::now();
    let mut child = match cmd.spawn() { Ok(c) => c, Err(e) => return cleanup(Err(format!("renderer_failed: cannot start the code clip renderer ({py}): {e}"))) };
    // Startup and every page load get the load timeout; each frame gets the frame timeout plus capture slack. Waiting on the
    // shared browser lock (KIMO_CODECLIP_LOCK) is extra and bounded by KIMO_CODECLIP_LOCK_WAIT_S (default 1 h).
    let lock_wait = if std::env::var("KIMO_CODECLIP_LOCK").is_ok() { std::env::var("KIMO_CODECLIP_LOCK_WAIT_S").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(3600) } else { 0 };
    let deadline = std::time::Duration::from_millis(30_000 + LOAD_TIMEOUT_MS * (1 + nreload as u64) + (fms + 5_000) * nframes as u64 + lock_wait * 1000);
    let status = loop {
        match child.try_wait() { Ok(Some(s)) => break Some(s), Ok(None) => {}, Err(_) => break None }
        if t0.elapsed() > deadline {
            let pid = child.id();
            let _ = std::process::Command::new("kill").args(["-KILL", &format!("-{pid}")]).status();
            let _ = child.kill(); let _ = child.wait();
            return cleanup(Err(format!("renderer_failed: code clip render hung past {} s ({nframes} frames); killed the renderer (pid {pid})", deadline.as_secs())));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let wall_s = t0.elapsed().as_secs_f64();
    let log = std::fs::read_to_string(&so).unwrap_or_default();
    let mut info = RunInfo { wall_s, ..Default::default() };
    let mut err: Option<String> = None;
    for l in log.lines() { if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
        if let Some(ms) = v.get("ms").and_then(|x| x.as_f64()) { info.frame_ms.push(ms); }
        if let Some(e) = v.get("error").and_then(|x| x.as_str()) { err = Some(e.to_string()); }
        if v.get("ok").is_some() { info.gl = v.get("gl").and_then(|x| x.as_str()).unwrap_or("").to_string(); info.blocked = v.get("blocked").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default(); }
    } }
    if let Some(e) = err { let e = if e.starts_with("renderer_failed:") { e } else { format!("renderer_failed: code clip {}: {e}", dir.display()) }; return cleanup(Err(e)); }
    if !status.map(|s| s.success()).unwrap_or(false) {
        let tail: String = std::fs::read_to_string(&se).unwrap_or_default().lines().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
        return cleanup(Err(format!("renderer_failed: code clip renderer exited with {:?}: {}", status.and_then(|s| s.code()), tail.chars().take(600).collect::<String>())));
    }
    let n = (cc.width * cc.height * 4) as usize; let mut frames = Vec::with_capacity(outs.len());
    for o in &outs { match std::fs::read(o) { Ok(b) if b.len() == n => frames.push(b), Ok(b) => return cleanup(Err(format!("renderer_failed: frame file has {} bytes, expected {n}", b.len()))), Err(e) => return cleanup(Err(format!("renderer_failed: missing frame: {e}"))) } }
    cleanup(Ok((frames, info)))
}

/// Frames for slots `ks` of a code clip at canvas size, from the disk cache or the browser (one batch for all misses).
/// Returns (slot, canvas-size RGBA8) pairs and how many came from the browser.
pub fn frames_for(p: &Project, c: &Clip, cc: &CodeClip, ks: &[usize]) -> Result<(Vec<(usize, Vec<u8>)>, usize), String> {
    let dir = resolve_dir(p, cc);
    validate_dir(&dir, &cc.entry)?;
    let tree = tree_hash(&dir)?;
    let canvas = (p.width, p.height);
    let cdir = cache_dir(p);
    let mut got: Vec<(usize, Vec<u8>)> = vec![]; let mut miss: Vec<(usize, Sample, u128)> = vec![];
    for &k in ks {
        let s = sample(c, cc, k, p.fps); let key = frame_key(tree, cc, &s, canvas);
        match cdir.as_ref().and_then(|d| read_cached(d, key, p.width, p.height)) { Some(b) => got.push((k, b)), None => miss.push((k, s, key)) }
    }
    let rendered = miss.len();
    if !miss.is_empty() {
        let steps: Vec<Step> = miss.iter().map(|(_, s, _)| Step::Frame(s.clone())).collect();
        let (frames, _) = render_steps(&dir, cc, &steps)?;
        for ((k, _, key), f) in miss.into_iter().zip(frames) {
            let f = fit(&f, cc.width as usize, cc.height as usize, p.width as usize, p.height as usize);
            if let Some(d) = &cdir { let _ = write_cached(d, key, p.width, p.height, &f); }
            got.push((k, f));
        }
        if let Some(d) = &cdir { prune_cache(d, cache_max_bytes()); }
    }
    got.sort_by_key(|x| x.0);
    Ok((got, rendered))
}

/// FNV-64 of a frame, as hex (what the determinism check compares and reports).
pub fn frame_hash(b: &[u8]) -> String { let mut h: u64 = 0xcbf29ce484222325; for x in b { h ^= *x as u64; h = h.wrapping_mul(0x100000001b3); } format!("{h:016x}") }

/// Determinism check: draw slot k fresh, then `before` earlier slots in order, then k again, then k after a page reload. All three
/// renders of k must be byte-identical; anything else is an error naming the hashes. Never reads or writes the cache.
pub fn check_determinism(p: &Project, c: &Clip, cc: &CodeClip, k: usize, before: usize) -> Result<serde_json::Value, String> {
    let dir = resolve_dir(p, cc);
    let s = sample(c, cc, k, p.fps);
    let lo = k.saturating_sub(before);
    let mut steps = vec![Step::Frame(s.clone())];
    for j in lo..k { steps.push(Step::Frame(sample(c, cc, j, p.fps))); }
    steps.push(Step::Frame(s.clone())); steps.push(Step::Reload); steps.push(Step::Frame(s.clone()));
    let (frames, info) = render_steps(&dir, cc, &steps)?;
    let n = frames.len();
    let (h0, h1, h2) = (frame_hash(&frames[0]), frame_hash(&frames[n - 2]), frame_hash(&frames[n - 1]));
    let transparent = frames[0].chunks_exact(4).all(|px| px[3] == 0);
    if h0 != h1 || h0 != h2 {
        let diff = |a: &[u8], b: &[u8]| a.iter().zip(b).filter(|(x, y)| x != y).count();
        return Err(format!("nondeterministic: code clip {} draws different pixels for the same frame (t={} s): first render {h0}, after {} earlier frames {h1} ({} bytes differ), after a page reload {h2} ({} bytes differ). renderFrame(t, params) must depend only on t and params: no Date.now/performance.now timing, no unseeded randomness, no state carried from earlier frames.",
            c.id, s.t, k - lo, diff(&frames[0], &frames[n - 2]), diff(&frames[0], &frames[n - 1])));
    }
    let ms = &info.frame_ms; let mean = if ms.is_empty() { 0.0 } else { ms.iter().sum::<f64>() / ms.len() as f64 };
    Ok(serde_json::json!({"deterministic": true, "clip_id": c.id, "t": s.t, "params": s.params, "hash": h0, "frames_rendered": n, "earlier_frames": k - lo,
        "fresh_page": true, "blank": transparent, "ms_per_frame": (mean * 10.0).round() / 10.0, "wall_s": (info.wall_s * 100.0).round() / 100.0, "blocked_requests": info.blocked, "gl": info.gl}))
}
