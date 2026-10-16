//! RV-6: chunked export job with status, cancel, resume and a safe incremental reuse rule.
//! A job renders picture-only chunks (`chunk_00000.mp4`, ...) next to `manifest.json` and `index.json`, then joins them with ONE continuous audio
//! stream (render::join_export). A chunk is reused only when its dependency key (export_manifest) matches the key recorded when it was rendered AND the
//! chunk file's sha256 still matches; duration and frame count are never freshness evidence. `force: true` re-renders everything and says so.
use super::export_manifest as em;
use super::kit::*;
use crate::deps;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

struct State {
    state: &'static str, frames_done: u64, frames_total: u64, started: std::time::Instant, current_range: Option<[u64; 2]>,
    chunks_done: u64, chunks_total: u64, chunks_reused: u64, warnings: Vec<Value>, rendered_secs: Vec<f64>, to_render: u64,
    completed: Vec<[u64; 2]>, error: Option<String>, result: Option<Value>, chunks_at: String, out: String,
}
struct Job { st: Mutex<State>, cancel: crate::guard::Cancel }
fn jobs() -> &'static Mutex<BTreeMap<String, Arc<Job>>> { static J: std::sync::OnceLock<Mutex<BTreeMap<String, Arc<Job>>>> = std::sync::OnceLock::new(); J.get_or_init(Default::default) }
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// ETA from measured chunk render times: null until `MIN_MEASURED` chunks were rendered, never false precision.
pub const MIN_MEASURED: usize = 3;
pub fn eta(measured: &[f64], remaining: u64) -> Value {
    if measured.len() < MIN_MEASURED { return Value::Null; }
    let n = measured.len() as f64; let mean = measured.iter().sum::<f64>() / n; let var = measured.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n; let cv = if mean > 0.0 { var.sqrt() / mean } else { 1.0 };
    json!({"seconds": mean * remaining as f64, "uncertainty": if cv < 0.15 { "low" } else if cv < 0.4 { "medium" } else { "high" }})
}

/// Durable write: temp file fsynced, renamed, then the directory fsynced so the rename itself survives a power cut.
fn write_atomic(path: &std::path::Path, v: &Value) -> Result<(), String> {
    use std::io::Write;
    let tmp = path.with_extension("tmp"); let bytes = serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?;
    { let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?; f.write_all(&bytes).map_err(|e| e.to_string())?; f.sync_all().map_err(|e| e.to_string())?; }
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?; sync_dir(path.parent().unwrap_or(std::path::Path::new(".")));
    Ok(())
}
fn sync_dir(d: &std::path::Path) { if let Ok(f) = std::fs::File::open(d) { let _ = f.sync_all(); } }
fn sync_file(p: &std::path::Path) -> Result<(), String> { std::fs::File::open(p).and_then(|f| f.sync_all()).map_err(|e| format!("cannot fsync {}: {e}", p.display())) }

/// RV-20 job lock: `<dir>/job.lock` created O_EXCL with {pid, host, started_at, start_ticks}. A lock is live only when its host is this host, the pid
/// exists AND the pid's /proc start time equals start_ticks (so a reused pid is not mistaken for the owner). Same-host only: a lock written by another
/// host cannot be verified and is refused. The guard releases the lock on drop (normal end, error, cancel or panic); SIGKILL leaves a stale lock that
/// the next start reclaims.
pub fn host_name() -> String { std::fs::read_to_string("/proc/sys/kernel/hostname").map(|s| s.trim().to_string()).unwrap_or_else(|_| "unknown".into()) }
pub fn start_ticks(pid: u32) -> Option<u64> { let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?; let rest = s.rsplit_once(')')?.1; rest.split_whitespace().nth(19)?.parse().ok() }
struct LockGuard { path: std::path::PathBuf, pid: u32 }
impl Drop for LockGuard { fn drop(&mut self) { if read_json(&self.path).map_or(false, |v| v["pid"] == json!(self.pid)) { let _ = std::fs::remove_file(&self.path); sync_dir(self.path.parent().unwrap_or(std::path::Path::new("."))); } } }
fn lock_live(v: &Value) -> Result<bool, String> {
    let host = v["host"].as_str().unwrap_or(""); if host != host_name() { return Err(format!("job_locked: job.lock was written on host '{host}'; this host is '{}' and a lock from another host cannot be verified (same-host lock only). Remove job.lock by hand if that job is certainly dead.", host_name())); }
    let pid = v["pid"].as_u64().unwrap_or(0) as u32; Ok(pid != 0 && start_ticks(pid).map_or(false, |t| Some(t) == v["start_ticks"].as_u64()))
}
fn acquire(dir: &std::path::Path) -> Result<LockGuard, String> {
    use std::io::Write;
    let path = dir.join("job.lock"); let pid = std::process::id();
    for _ in 0..3 {
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                let body = json!({"pid": pid, "host": host_name(), "started_at": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0), "start_ticks": start_ticks(pid)});
                f.write_all(body.to_string().as_bytes()).map_err(|e| e.to_string())?; f.sync_all().map_err(|e| e.to_string())?; sync_dir(dir); return Ok(LockGuard { path, pid }); }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let v = read_json(&path); match v { Some(v) => { if lock_live(&v)? { return Err(format!("job_locked: another export job (pid {}) is running in {}; wait for it, cancel it, or use a different dir", v["pid"], dir.display())); } }
                    None => { std::thread::sleep(std::time::Duration::from_millis(50)); if read_json(&path).is_none() { /* unreadable: torn write by a dead owner */ } else { continue; } } }
                let _ = std::fs::remove_file(&path); }
            Err(e) => return Err(format!("cannot create job.lock in {}: {e}", dir.display())),
        }
    }
    Err(format!("job_locked: could not take job.lock in {} (another process is racing for it)", dir.display()))
}
fn read_json(path: &std::path::Path) -> Option<Value> { serde_json::from_slice(&std::fs::read(path).ok()?).ok() }
fn file_sha(p: &std::path::Path) -> Option<String> { deps::hash_file(p, true).ok().map(|x| x.0) }
fn chunk_name(i: u64) -> String { format!("chunk_{i:05}.mp4") }

fn status_json(j: &Job) -> Value {
    let s = j.st.lock().unwrap_or_else(|e| e.into_inner()); let remaining = s.to_render.saturating_sub(s.rendered_secs.len() as u64);
    json!({"state": s.state, "frames_done": s.frames_done, "frames_total": s.frames_total, "elapsed_s": s.started.elapsed().as_secs_f64(), "current_range": s.current_range,
        "chunks": {"done": s.chunks_done, "total": s.chunks_total, "reused": s.chunks_reused}, "warnings": s.warnings, "eta": if s.state == "running" { eta(&s.rendered_secs, remaining) } else { Value::Null },
        "completed_ranges": s.completed, "dir": s.chunks_at, "path": s.out, "error": s.error, "result": s.result})
}
fn set<F: FnOnce(&mut State)>(j: &Job, f: F) { f(&mut j.st.lock().unwrap_or_else(|e| e.into_inner())); }

fn packet_count(p: &std::path::Path) -> Option<u64> { let o = std::process::Command::new("ffprobe").args(["-v", "error", "-count_packets", "-select_streams", "v:0", "-show_entries", "stream=nb_read_packets", "-of", "csv=p=0"]).arg(p).output().ok()?; String::from_utf8_lossy(&o.stdout).trim().parse().ok() }
/// Why a chunk must be rendered, or None when the recorded one is verified: it is indexed, key and crf match, the file exists with the recorded size,
/// its sha256 matches and ffprobe counts the recorded number of frames. Duration is never evidence.
fn reuse_check(dir: &std::path::Path, idx: u64, key: &str, crf: u32, old: &Option<Value>, force: bool) -> Option<&'static str> {
    if force { return Some("forced"); }
    let Some(o) = old.as_ref().and_then(|o| o["chunks"].get(idx.to_string())) else { return Some("no_previous") };
    if o["key"] != json!(key) || o["crf"] != json!(crf) { return Some("key_changed"); }
    let f = dir.join(chunk_name(idx)); let Ok(md) = std::fs::metadata(&f) else { return Some("file_missing") }; if !md.is_file() { return Some("file_missing"); }
    if Some(md.len()) != o["bytes"].as_u64() { return Some("size_mismatch"); }
    if file_sha(&f).as_deref() != o["sha256"].as_str() { return Some("file_hash_mismatch"); }
    if packet_count(&f) != o["frames"].as_u64() { return Some("frame_count_mismatch"); }
    None
}
/// Crash recovery: anything on disk that the index does not vouch for is untrusted. A `*.partial.mp4` (an encode that never finished) and a finished-looking
/// chunk with no index entry (written, then killed before the index was) are renamed `<chunk>.mp4.partial` with a marker {reason:"crash_or_unverified"}.
fn quarantine(dir: &std::path::Path, old: &Option<Value>) -> Vec<Value> {
    let mut out = vec![]; let Ok(rd) = std::fs::read_dir(dir) else { return out };
    let mut names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect(); names.sort();
    for n in names {
        let stem = if let Some(s) = n.strip_suffix(".partial.mp4") { Some(s.to_string()) } else if let Some(s) = n.strip_suffix(".mp4").filter(|s| s.starts_with("chunk_")) {
            let i = s.trim_start_matches("chunk_").parse::<u64>().ok(); if i.map_or(false, |i| old.as_ref().map_or(false, |o| o["chunks"].get(i.to_string()).is_some())) { None } else { Some(s.to_string()) } } else { None };
        let Some(stem) = stem else { continue };
        let pf = dir.join(format!("{stem}.mp4.partial")); let _ = std::fs::remove_file(&pf); let _ = std::fs::rename(dir.join(&n), &pf);
        let _ = write_atomic(&dir.join(format!("{stem}.mp4.partial.marker")), &json!({"file": n, "reason": "crash_or_unverified"}));
        out.push(json!({"file": n, "reason": "crash_or_unverified"}));
    }
    out
}


fn ffprobe(args: &[&str], p: &std::path::Path) -> Result<(String, String), String> { let o = std::process::Command::new("ffprobe").args(["-v", "error"]).args(args).arg(p).output().map_err(|e| format!("ffprobe: {e}"))?; Ok((String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).trim().to_string())) }
/// Parse the first row of `ffprobe -show_entries frame=key_frame,pict_type -of csv=p=0` by column: (key_frame, pict_type), in the order requested.
/// ffprobe 5.x appends one empty trailing field for the frame's (empty) side-data section, so `1,I,` and `1,I` are the two documented forms;
/// that single empty trailing field is the only extra accepted. Anything else (a missing column, a non-empty or second extra field, a key_frame that is
/// not 0/1, an empty pict_type) is refused, so the caller still verifies both the key-frame flag and the picture type.
pub fn parse_first_frame(row: &str) -> Result<(bool, String), String> {
    let cols: Vec<&str> = row.split(',').collect();
    let (kf, pt) = match cols.as_slice() { [kf, pt] | [kf, pt, ""] => (*kf, *pt), _ => return Err(format!("unexpected ffprobe frame row '{row}': expected 'key_frame,pict_type' with at most one empty trailing field")) };
    let key = match kf { "1" => true, "0" => false, _ => return Err(format!("unexpected ffprobe key_frame '{kf}' in frame row '{row}'")) };
    if pt.is_empty() { return Err(format!("ffprobe frame row '{row}' has no pict_type")); }
    Ok((key, pt.to_string()))
}
/// Join contract: every chunk must start on a key frame (I) and share codec parameters; a chunk that does not cannot be stream-copied at a seam.
pub fn chunk_contract(parts: &[String]) -> Result<(), String> {
    let mut first: Option<String> = None;
    for (i, f) in parts.iter().enumerate() {
        let pth = std::path::Path::new(f);
        let (fr, _) = ffprobe(&["-select_streams", "v:0", "-read_intervals", "%+#1", "-show_entries", "frame=key_frame,pict_type", "-of", "csv=p=0"], pth)?;
        let l = fr.lines().next().unwrap_or("").trim().to_string();
        match parse_first_frame(&l) { Ok((true, pt)) if pt == "I" => {}, Ok(_) => return Err(format!("join_contract: chunk {i} ({f}) does not start with a key frame (first frame is '{l}'); it cannot be stream-copied into the join")), Err(e) => return Err(format!("join_contract: chunk {i} ({f}): {e}")) }
        let (par, _) = ffprobe(&["-select_streams", "v:0", "-show_entries", "stream=codec_name,profile,width,height,pix_fmt,r_frame_rate", "-of", "csv=p=0"], pth)?; let par = par.trim().to_string();
        match &first { None => first = Some(par), Some(f0) if *f0 != par => return Err(format!("join_contract: chunk {i} has codec parameters '{par}' but chunk 0 has '{f0}'")), _ => {} }
    }
    Ok(())
}
/// Final verification of a joined file: decoded frame count, video and audio duration, monotonic timestamps. `verified` is true only when all hold.
pub fn verify_output(path: &std::path::Path, frames_total: u64, fps: f64, audio_secs: Option<f64>) -> Value {
    let mut problems: Vec<String> = vec![]; let one = 1.0 / fps;
    let (cnt, err) = ffprobe(&["-count_frames", "-select_streams", "v:0", "-show_entries", "stream=nb_read_frames,duration", "-of", "csv=p=0"], path).unwrap_or_default();
    let mut it = cnt.trim().split(','); let vdur: f64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(-1.0); let decoded: u64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    if !err.is_empty() { problems.push(format!("decode errors: {}", err.lines().next().unwrap_or(""))); }
    if decoded != frames_total { problems.push(format!("decoded {decoded} frames, expected {frames_total}")); }
    let want = frames_total as f64 / fps; if (vdur - want).abs() > one + 1e-9 { problems.push(format!("video duration {vdur:.4}s, expected {want:.4}s within one frame ({one:.4}s)")); }
    let (pk, _) = ffprobe(&["-select_streams", "v:0", "-show_entries", "packet=pts_time,dts_time", "-of", "csv=p=0"], path).unwrap_or_default();
    let (mut dts, mut pts): (Vec<f64>, Vec<f64>) = (vec![], vec![]); for l in pk.lines() { let mut c = l.split(','); if let (Some(a), Some(b)) = (c.next().and_then(|x| x.parse::<f64>().ok()), c.next().and_then(|x| x.parse::<f64>().ok())) { pts.push(a); dts.push(b); } }
    let mono = dts.windows(2).all(|w| w[1] > w[0]); pts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); let pmono = pts.windows(2).all(|w| w[1] > w[0]);
    if !mono || !pmono { problems.push("timestamps are not strictly monotonic".into()); }
    let mut adur = Value::Null;
    if let Some(a) = audio_secs { let (o, _) = ffprobe(&["-select_streams", "a:0", "-show_entries", "stream=duration", "-of", "csv=p=0"], path).unwrap_or_default();
        match o.trim().parse::<f64>() { Ok(d) => { adur = json!(d); let tol = 1024.0 / crate::audio::SR as f64; if (d - a).abs() > tol { problems.push(format!("audio duration {d:.4}s, expected {a:.4}s within one AAC frame ({tol:.4}s)")); } } Err(_) => problems.push("expected an audio stream and found none".into()) } }
    json!({"verified": problems.is_empty(), "decoded_frames": decoded, "frames_total": frames_total, "video_duration": vdur, "audio_duration": adur, "pts_monotonic": mono && pmono, "problems": problems})
}
/// RV-20 join: contract check, then concat demuxer -c copy (audio rendered ONCE and muxed in the same ffmpeg step) into `<out>.joining.tmp`, fsync,
/// verify, atomic rename. A failure leaves nothing at `out` (an earlier good output is not touched) and removes the temp file.
pub fn join_verified(p: &crate::model::Project, parts: &[String], out: &str) -> Result<Value, String> {
    if parts.is_empty() { return Err("parts must list at least one picture-only export".into()); }
    chunk_contract(parts)?;
    let fps = if p.fps > 0.0 { p.fps } else { 30.0 }; let mut frames = 0u64; for f in parts { frames += ffprobe(&["-count_packets", "-select_streams", "v:0", "-show_entries", "stream=nb_read_packets", "-of", "csv=p=0"], std::path::Path::new(f))?.0.trim().parse::<u64>().map_err(|_| format!("cannot count frames of {f}"))?; }
    let outp = std::path::Path::new(out); let tmp = std::path::PathBuf::from(format!("{out}.joining.tmp")); let wav = std::path::PathBuf::from(format!("{out}.joining.wav")); let list = std::path::PathBuf::from(format!("{out}.joining.txt"));
    let cleanup = |tmp: &std::path::Path| { let _ = std::fs::remove_file(tmp); let _ = std::fs::remove_file(&wav); let _ = std::fs::remove_file(&list); };
    cleanup(&tmp);
    let sr = crate::audio::SR as f64; let (s1, secs) = (((frames as f64 / fps) * sr) as usize * 2, frames as f64 / fps);
    crate::guard::reserve(s1 * 4); let full = crate::audio::mix(p); let mut pcm = full[..s1.min(full.len())].to_vec(); pcm.resize(s1, 0.0); let audible = pcm.iter().any(|v| v.abs() > 1e-6);
    std::fs::write(&list, parts.iter().map(|f| format!("file '{}'\n", f.replace('\'', "'\\''"))).collect::<String>()).map_err(|e| e.to_string())?;
    if audible { crate::audio::write_wav(wav.to_str().ok_or("invalid path")?, &pcm)?; }
    let fmt = match outp.extension().and_then(|e| e.to_str()).unwrap_or("mp4") { "mov" => "mov", "mkv" => "matroska", _ => "mp4" };
    let mut cmd = std::process::Command::new("ffmpeg"); cmd.args(["-v", "error", "-y", "-f", "concat", "-safe", "0", "-i"]).arg(&list); if audible { cmd.arg("-i").arg(&wav).args(["-map", "0:v:0", "-map", "1:a:0", "-c:a", "aac", "-b:a", "192k"]); } else { cmd.arg("-an"); }
    cmd.args(["-c:v", "copy", "-f", fmt]).arg(&tmp);
    let o = cmd.output().map_err(|e| format!("ffmpeg: {e}"))?; if !o.status.success() { cleanup(&tmp); return Err(format!("join failed: {}", String::from_utf8_lossy(&o.stderr).trim())); }
    if let Err(e) = sync_file(&tmp) { cleanup(&tmp); return Err(e); }
    let v = verify_output(&tmp, frames, fps, if audible { Some(secs) } else { None });
    if v["verified"] != json!(true) { cleanup(&tmp); return Err(format!("join_unverified: the joined file failed verification and was discarded: {}", v["problems"])); }
    std::fs::rename(&tmp, outp).map_err(|e| { cleanup(&tmp); format!("cannot move the verified join into place: {e}") })?; sync_dir(outp.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."))); cleanup(&tmp);
    Ok(json!({"duration": secs, "verified": true, "verify": v}))
}

/// Classify a failure so the caller can act: `disk_full` (free space, then resume) or `encoder_failed` (the ffmpeg/x264 stderr tail is kept).
pub fn classify(e: &str) -> String {
    if e.starts_with("disk_full:") || e.starts_with("encoder_failed:") { return e.to_string(); }
    let l = e.to_lowercase();
    if l.contains("no space left") || l.contains("disk full") || l.contains("disk quota") || l.contains("storage full") || l.contains("enospc") { return format!("disk_full: the disk has no space left; free space and resume_job (finished chunks are kept). Detail: {}", tail(e)); }
    if l.contains("ffmpeg") || l.contains("encoder") || l.contains("libx264") || l.contains("conversion failed") || l.contains("broken pipe") || l.contains("encode") { return format!("encoder_failed: {}", tail(e)); }
    e.to_string()
}
fn tail(e: &str) -> String { let t: Vec<&str> = e.lines().rev().take(6).collect(); let mut v: Vec<&str> = t; v.reverse(); let j = v.join(" | "); if j.len() > 600 { j[j.len() - 600..].to_string() } else { j } }

fn run(j: Arc<Job>, p: crate::model::Project, dir: std::path::PathBuf, out: String, cf: u64, crf: u32, force: bool, _lock: LockGuard) {
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Value, String> {
        let fps = if p.fps > 0.0 { p.fps } else { 30.0 }; let total = (em::project_end(&p) * fps).ceil() as u64; if total == 0 { return Err("nothing to export: the timeline is empty".into()); }
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let m = em::build(&p, 0, total, cf, false); write_atomic(&dir.join("manifest.json"), &m)?;
        let mut old = read_json(&dir.join("index.json")); let quarantined = quarantine(&dir, &old); let chunks = m["chunks"].as_array().cloned().unwrap_or_default();
        let plan: Vec<Option<&'static str>> = chunks.iter().map(|c| reuse_check(&dir, c["index"].as_u64().unwrap_or(0), c["key"].as_str().unwrap_or(""), crf, &old, force)).collect();
        set(&j, |s| { s.state = "running"; s.frames_total = total; s.chunks_total = chunks.len() as u64; s.to_render = plan.iter().filter(|x| x.is_some()).count() as u64; });
        let mut index = json!({"project_hash": m["project_hash"], "chunk_frames": cf, "chunks": {}}); let (mut reused, mut rendered): (Vec<u64>, Vec<Value>) = (vec![], vec![]);
        for (c, why) in chunks.iter().zip(plan.iter()) {
            let idx = c["index"].as_u64().unwrap_or(0); let range = [c["range"][0].as_u64().unwrap_or(0), c["range"][1].as_u64().unwrap_or(0)]; let file = dir.join(chunk_name(idx));
            if j.cancel.load(std::sync::atomic::Ordering::Relaxed) { return Err("cancelled".into()); }
            set(&j, |s| s.current_range = Some(range));
            let entry = match why {
                None => { reused.push(idx); old.as_ref().and_then(|o| o["chunks"].get(idx.to_string()).cloned()).unwrap_or(Value::Null) }
                Some(why) => {
                    let t0 = std::time::Instant::now(); let part = dir.join(format!("chunk_{idx:05}.partial.mp4")); let _ = std::fs::remove_file(&file);
                    let r = crate::render::export_range_opts(&p, part.to_str().unwrap_or(""), crf, range[0] as f64 / fps, range[1] as f64 / fps, true, false);
                    match r { Ok(_) => { sync_file(&part)?; std::fs::rename(&part, &file).map_err(|e| e.to_string())?; sync_dir(&dir); }
                        Err(e) => { let pf = dir.join(format!("{}.partial", chunk_name(idx))); let _ = std::fs::rename(&part, &pf); let _ = write_atomic(&dir.join(format!("{}.partial.marker", chunk_name(idx))), &json!({"range": range, "key": c["key"], "error": e})); return Err(e); } }
                    rendered.push(json!({"index": idx, "reason": why})); set(&j, |s| s.rendered_secs.push(t0.elapsed().as_secs_f64()));
                    json!({"file": chunk_name(idx), "frames": range[1] - range[0], "key": c["key"], "range": range, "crf": crf, "sha256": file_sha(&file), "bytes": std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0)}) }
            };
            index["chunks"][idx.to_string()] = entry; write_atomic(&dir.join("index.json"), &index)?;
            set(&j, |s| { s.chunks_done += 1; s.frames_done += range[1] - range[0]; if why.is_none() { s.chunks_reused += 1; } s.completed.push(range); });
        }
        let parts: Vec<String> = (0..chunks.len() as u64).map(|i| dir.join(chunk_name(i)).to_string_lossy().into_owned()).collect();
        let joined = join_verified(&p, &parts, &out)?; let dur = joined["duration"].clone();
        let mut reasons = serde_json::Map::new(); let mut re = 0u64; for r in &rendered { let (i, why) = (r["index"].as_u64().unwrap_or(0), r["reason"].as_str().unwrap_or("")); if matches!(why, "no_previous" | "forced") || old.as_ref().map_or(true, |o| o["chunks"].get(i.to_string()).is_none()) { continue; } re += 1; let n = reasons.get(why).and_then(|v| v.as_u64()).unwrap_or(0); reasons.insert(why.to_string(), json!(n + 1)); }
        Ok(json!({"path": out, "duration": dur, "verified": joined["verified"], "verify": joined["verify"], "reverified": {"ok": reused.len(), "re_rendered": re, "reasons": reasons}, "quarantined": quarantined, "reused": reused, "rendered": rendered, "project_hash": m["project_hash"], "forced": force}))
    }));
    let cancelled = j.cancel.load(std::sync::atomic::Ordering::Relaxed);
    if !matches!(res, Ok(Ok(_))) { if let Ok(rd) = std::fs::read_dir(&dir) { for e in rd.flatten() { let n = e.file_name().to_string_lossy().into_owned(); if let Some(stem) = n.strip_suffix(".partial.mp4") { let pf = dir.join(format!("{stem}.mp4.partial")); let _ = std::fs::rename(e.path(), &pf); let _ = write_atomic(&dir.join(format!("{stem}.mp4.partial.marker")), &json!({"error": "interrupted before the chunk finished", "cancelled": cancelled})); } } } }
    set(&j, |s| { s.current_range = None; match res { Ok(Ok(v)) => { s.state = "done"; s.result = Some(v); }
        Ok(Err(e)) => { if cancelled || e == "cancelled" { s.state = "cancelled"; } else { s.state = "failed"; s.error = Some(classify(&e)); } }
        Err(_) => { if cancelled { s.state = "cancelled"; } else { s.state = "failed"; s.error = Some("export job panicked".into()); } } } });
}

fn start(ss: &Session, a: &Value, resume: bool) -> Result<Value, String> {
    let out = str_arg(a, "path")?.to_string(); let dir = std::path::PathBuf::from(a.get("dir").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| format!("{out}.chunks")));
    let fps = if ss.project.fps > 0.0 { ss.project.fps } else { 30.0 }; let crf = opt_num(a, "crf", 18.0); if !(0.0..=51.0).contains(&crf) { return Err("crf must be 0..51".into()); }
    let mut cs = opt_num(a, "chunk_s", 4.0); let prev = read_json(&dir.join("manifest.json"));
    if resume { let m = prev.as_ref().ok_or_else(|| format!("resume_mismatch: no manifest.json in {} (nothing to resume; use export_job_start)", dir.display()))?;
        if a.get("chunk_s").is_none() { cs = m["settings"]["chunk_frames"].as_f64().unwrap_or(0.0) / fps; }
        let total = (em::project_end(&ss.project) * fps).ceil() as u64; let cur = em::build(&ss.project, 0, total, ((cs * fps).round() as u64).max(1), false);
        if cur["project_hash"] != m["project_hash"] || cur["settings"]["hash"] != m["settings"]["hash"] {
            let ex = em::explain(m, &cur); let first = ex["chunks"].as_array().and_then(|c| c.iter().find(|x| x["stale"] == true)).map(|c| c["reason"][0].clone()).unwrap_or(Value::Null);
            return Err(format!("resume_mismatch: the project changed since this job was started; first differing dependency: {first}. Start a new job (export_job_start) to reuse the chunks that are still valid.")); } }
    if !cs.is_finite() || cs <= 0.0 { return Err("chunk_s must be a positive number of seconds".into()); }
    let cf = ((cs * fps).round() as u64).max(1); let force = a.get("force").and_then(|v| v.as_bool()).unwrap_or(false); let wait = a.get("wait").and_then(|v| v.as_bool()).unwrap_or(false);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?; let lock = acquire(&dir)?;
    let id = format!("job{}", NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let j = Arc::new(Job { st: Mutex::new(State { state: "queued", frames_done: 0, frames_total: 0, started: std::time::Instant::now(), current_range: None, chunks_done: 0, chunks_total: 0, chunks_reused: 0, warnings: vec![], rendered_secs: vec![], to_render: 0, completed: vec![], error: None, result: None, chunks_at: dir.to_string_lossy().into_owned(), out: out.clone() }), cancel: crate::guard::new_token() });
    jobs().lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), j.clone());
    let (p, j2, tok) = (ss.project.clone(), j.clone(), j.cancel.clone());
    let h = crate::guard::spawn_with_token(move || crate::guard::with_token(&tok, || run(j2, p, dir, out, cf, crf as u32, force, lock)));
    if wait { let _ = h.join(); }
    Ok(json!({"job_id": id, "status": status_json(&j)}))
}
fn get(a: &Value) -> Result<Arc<Job>, String> { let id = str_arg(a, "job_id")?; jobs().lock().unwrap_or_else(|e| e.into_inner()).get(id).cloned().ok_or_else(|| format!("no such job '{id}'")) }

pub fn commands() -> Vec<Command> { vec![
    Command { name: "export_job_start", mutates: false,
        description: "Start a chunked export in the background and return a job_id (poll with job_status). Picture-only chunks are rendered into <path>.chunks/ with a manifest.json and index.json, then joined with ONE continuous audio stream into path. Incremental: a chunk is reused only when its dependency key matches AND the chunk file's sha256 still verifies; otherwise it is rendered, and the result lists reused/rendered chunks with the reason (no_previous, key_changed, file_missing, file_hash_mismatch, forced). force:true re-renders everything. wait:true blocks until the job ends and returns the final status. The job renders a snapshot of the project taken at start. Durability (RV-20): the directory holds an exclusive job.lock (same host only; a second start or resume is refused with a job-locked error, a lock left by a killed process is reclaimed). Every chunk and index write is fsynced; on start/resume an unfinished or unindexed chunk is quarantined (.partial + marker) and every indexed chunk is verified (size, sha256, frame count, key) before reuse. The join is stream-copied into <path>.joining.tmp, verified (decoded frames, durations, monotonic timestamps) and renamed atomically; a failed join leaves nothing at path. Failures are classified as disk-full or encoder-failed errors (stderr tail kept) and keep finished chunks. Limits: crash safety is chunk-granular (at most the in-flight chunk is lost), the lock is same-host only, and encodes are not guaranteed bytewise identical across runs (decoded frames were equal in tests).",
        schema: obj(vec![("path", s("output .mp4 path")), ("chunk_s", n("chunk length in seconds, default 4")), ("crf", n("quality 0..51, default 18")), ("dir", s("chunk directory, default <path>.chunks")), ("force", json!({"type":"boolean"})), ("wait", json!({"type":"boolean","description":"block until done (default false)"}))], &["path"]),
        run: |ss, a| start(ss, a, false) },
    Command { name: "job_status", mutates: false,
        description: "Status of an export job: state (queued|running|cancelling|cancelled|failed|done), frames_done/frames_total, elapsed_s, current_range [first,last) frames, chunks {done,total,reused}, warnings, completed_ranges, and eta {seconds, uncertainty low|medium|high} which stays null until 3 chunks have been measured. The final result (path, reused/rendered chunks with reasons) is under result.",
        schema: obj(vec![("job_id", s("id returned by export_job_start"))], &["job_id"]), run: |_ss, a| { let j = get(a)?; Ok(status_json(&j)) } },
    Command { name: "cancel_job", mutates: false,
        description: "Cancel an export job. Waits up to 20 s for it to stop, then returns the status plus completed_ranges (chunks that finished and stay valid). An unfinished chunk is renamed chunk_NNNNN.mp4.partial with a .partial.marker file so it is never mistaken for a finished one.",
        schema: obj(vec![("job_id", s("job to cancel"))], &["job_id"]),
        run: |_ss, a| { let j = get(a)?; set(&j, |s| if s.state == "running" || s.state == "queued" { s.state = "cancelling"; }); j.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            for _ in 0..200 { { let s = j.st.lock().unwrap_or_else(|e| e.into_inner()); if !matches!(s.state, "cancelling" | "running" | "queued") { break; } } std::thread::sleep(std::time::Duration::from_millis(100)); } Ok(status_json(&j)) } },
    Command { name: "resume_job", mutates: false,
        description: "Continue an interrupted or cancelled export from its chunk directory. Verifies the project still matches the manifest written when the job started and refuses (error resume_mismatch) naming the first differing dependency (use export_job_start to reuse whatever is still valid instead). Finished chunks whose key and file hash verify are kept; the rest are rendered.",
        schema: obj(vec![("path", s("output .mp4 path")), ("dir", s("chunk directory, default <path>.chunks")), ("crf", n("quality 0..51, default 18")), ("chunk_s", n("default: the original job's"))], &["path"]),
        run: |ss, a| start(ss, a, true) },
] }

#[cfg(test)]
mod first_frame_row_tests {
    use super::parse_first_frame;
    #[test]
    fn both_documented_ffprobe_forms_parse_to_the_same_columns() {
        assert_eq!(parse_first_frame("1,I").unwrap(), (true, "I".to_string()));
        assert_eq!(parse_first_frame("1,I,").unwrap(), (true, "I".to_string()));
        assert_eq!(parse_first_frame("0,P").unwrap(), (false, "P".to_string()));
        assert_eq!(parse_first_frame("0,B,").unwrap(), (false, "B".to_string()));
    }
    #[test]
    fn anything_beyond_one_empty_trailing_field_is_refused() {
        for bad in ["", "1", "1,", ",I", "1,I,x", "1,I,,", "1,I,0", "I,1", "2,I", "true,I", " 1,I"] { assert!(parse_first_frame(bad).is_err(), "'{bad}' must be refused"); }
    }
}
