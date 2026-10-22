//! RV-9 `preview_range`: a short, cheap preview of a time range through the SAME composition path as export (render_frame_opts, colour pipeline output,
//! RGBA8), at draft quality: optional scale (box-downscaled), fps_divisor and audio. Read-only: it never changes the project, never writes into an export
//! job's chunk directory, and is bounded (default 30 s). Per-frame hashes can be returned so a preview can be compared with an export bit for bit.
use super::kit::*;
use crate::commands::Session;
use crate::deps::sha256_hex;
use std::io::Write;

pub const MAX_SECONDS: f64 = 30.0;
const MAX_SECONDS_HARD: f64 = 300.0;
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Integer box filter when the factor is exact (sum + n/2) / n, else area-weighted; RGBA8, rounding to nearest.
pub fn box_downscale(src: &[u8], w: usize, h: usize, nw: usize, nh: usize) -> Vec<u8> {
    if nw == w && nh == h { return src.to_vec(); }
    let mut out = vec![0u8; nw * nh * 4];
    if w % nw == 0 && h % nh == 0 {
        let (fx, fy) = (w / nw, h / nh); let n = (fx * fy) as u32;
        for y in 0..nh { for x in 0..nw { for ch in 0..4 { let mut sum = 0u32; for dy in 0..fy { for dx in 0..fx { sum += src[((y * fy + dy) * w + x * fx + dx) * 4 + ch] as u32; } } out[(y * nw + x) * 4 + ch] = ((sum + n / 2) / n) as u8; } } }
        return out;
    }
    let (sx, sy) = (w as f64 / nw as f64, h as f64 / nh as f64);
    for y in 0..nh { let (y0, y1) = (y as f64 * sy, (y + 1) as f64 * sy); for x in 0..nw { let (x0, x1) = (x as f64 * sx, (x + 1) as f64 * sx); let mut acc = [0f64; 4]; let mut wsum = 0f64;
        for iy in (y0.floor() as usize)..((y1.ceil() as usize).min(h)) { let wy = (y1.min(iy as f64 + 1.0) - y0.max(iy as f64)).max(0.0); for ix in (x0.floor() as usize)..((x1.ceil() as usize).min(w)) { let wx = (x1.min(ix as f64 + 1.0) - x0.max(ix as f64)).max(0.0); let wt = wx * wy; for ch in 0..4 { acc[ch] += src[(iy * w + ix) * 4 + ch] as f64 * wt; } wsum += wt; } }
        for ch in 0..4 { out[(y * nw + x) * 4 + ch] = if wsum > 0.0 { (acc[ch] / wsum + 0.5).floor().clamp(0.0, 255.0) as u8 } else { 0 }; } } }
    out
}
fn in_job_dir(p: &std::path::Path) -> bool { let mut d = p.parent(); while let Some(x) = d { if x.file_name().map_or(false, |n| n.to_string_lossy().ends_with(".chunks")) || x.join("job.lock").exists() || x.join("manifest.json").exists() && x.join("index.json").exists() { return true; } d = x.parent(); } false }
fn num(a: &Value, k: &str, d: f64, lo: f64, hi: f64) -> Result<f64, String> { match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => { let x = v.as_f64().filter(|x| x.is_finite()).ok_or(format!("out_of_range: {k} must be a number"))?; if x < lo || x > hi { return Err(format!("out_of_range: {k} must be between {lo} and {hi}")); } Ok(x) } } }

fn preview(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let t_all = std::time::Instant::now(); let p = &ss.project; let fps = if p.fps > 0.0 { p.fps } else { 30.0 };
    let end_tl = super::export_manifest::project_end(p); if end_tl <= 0.0 { return Err("out_of_range: the timeline is empty".into()); }
    let total = (end_tl * fps).ceil() as usize; let max_s = num(a, "max_seconds", MAX_SECONDS, 0.1, MAX_SECONDS_HARD)?;
    let start = num(a, "start", 0.0, 0.0, f64::MAX)?; let end = num(a, "end", (start + max_s).min(end_tl), 0.0, f64::MAX)?;
    if end <= start { return Err("out_of_range: end must be greater than start".into()); } if end - start > max_s + 1e-9 { return Err(format!("out_of_range: a preview is bounded to {max_s} s (asked for {:.2} s); raise max_seconds (up to {MAX_SECONDS_HARD}) or shorten the range", end - start)); }
    let f0 = (start * fps).round() as usize; if f0 >= total { return Err("out_of_range: start is after the end of the timeline".into()); } let f1 = (((end * fps).round() as usize).max(f0 + 1)).min(total);
    let div = num(a, "fps_divisor", 1.0, 1.0, 30.0)?; if div.fract() != 0.0 { return Err("out_of_range: fps_divisor must be a whole number".into()); } let div = div as usize;
    let scale = num(a, "scale", 0.5, 0.1, 1.0)?; let (w, h) = (p.width as usize, p.height as usize);
    let (nw, nh) = if scale >= 1.0 { (w, h) } else { ((((w as f64 * scale) / 2.0).round() as usize * 2).max(2), (((h as f64 * scale) / 2.0).round() as usize * 2).max(2)) };
    let want_audio = a.get("audio").and_then(|v| v.as_bool()).unwrap_or(false); let want_hashes = a.get("hashes").and_then(|v| v.as_bool()).unwrap_or(false);
    let out = match a.get("path").and_then(|v| v.as_str()) { Some(x) => std::path::PathBuf::from(x), None => std::env::temp_dir().join("kimo-preview").join(format!("preview-{}-{}.mp4", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))) };
    if in_job_dir(&out) { return Err("out_of_range: a preview must not be written into an export job's chunk directory (path is inside one)".into()); }
    if let Some(d) = out.parent().filter(|d| !d.as_os_str().is_empty()) { std::fs::create_dir_all(d).map_err(|e| format!("cannot create {}: {e}", d.display()))?; }
    let idxs: Vec<usize> = (f0..f1).step_by(div).collect(); let n = idxs.len(); let ofps = fps / div as f64;
    let tmp = std::path::PathBuf::from(format!("{}.tmp", out.display()));
    let mut cmd = std::process::Command::new("ffmpeg"); cmd.args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{nw}x{nh}"), "-r", &ofps.to_string(), "-i", "-", "-c:v", "libx264", "-preset", "ultrafast", "-crf", "28", "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2", "-pix_fmt", "yuv420p", "-an", "-f", "mp4"]).arg(&tmp).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("encoder_failed: cannot start ffmpeg: {e}"))?; let mut stdin = child.stdin.take().ok_or("encoder_failed: no encoder stdin")?;
    let nt = crate::guard::export_threads_n(p.width, p.height, std::thread::available_parallelism().map(|x| x.get()).unwrap_or(2).max(1), crate::render::frame_buffers(p)).max(1);
    let mut cache = crate::render::Cache::new_export(); let mut hashes: Vec<String> = vec![]; let mut first_ms = 0.0; let mut i = 0usize; let mut werr: Option<String> = None;
    while i < n && werr.is_none() {
        let end_i = (i + nt * 4).min(n); crate::guard::check();
        if let Err(e) = crate::render::prepare_export_range(p, &mut cache, idxs[i] as f64 / fps, idxs[end_i - 1] as f64 / fps) { drop(stdin); let _ = child.kill(); let _ = child.wait(); let _ = std::fs::remove_file(&tmp); return Err(e); }
        let next = std::sync::atomic::AtomicUsize::new(i); let slots: Vec<std::sync::Mutex<Option<Result<Vec<u8>, String>>>> = (i..end_i).map(|_| std::sync::Mutex::new(None)).collect();
        std::thread::scope(|sc| { for _ in 0..nt.min(end_i - i) { let (cache, next, slots, idxs) = (&cache, &next, &slots, &idxs); crate::guard::scope_spawn(sc, move || loop { let k = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst); if k >= end_i { break; }
            let r = crate::render::export_frame_rgba8(p, idxs[k] as f64 / fps, cache).map(|f| box_downscale(&f, w, h, nw, nh)); *slots[k - i].lock().unwrap() = Some(r); }); } });
        if i == 0 { first_ms = t_all.elapsed().as_secs_f64() * 1000.0; }
        for s in slots { match s.into_inner().unwrap().unwrap() { Ok(f) => { if want_hashes && hashes.len() < 4000 { hashes.push(sha256_hex(&f)); } if werr.is_none() { if let Err(e) = stdin.write_all(&f) { werr = Some(e.to_string()); } } } Err(e) => { werr = Some(e); break; } } }
        i = end_i;
    }
    drop(stdin); let o = child.wait_with_output().map_err(|e| e.to_string())?;
    if let Some(e) = werr { let _ = std::fs::remove_file(&tmp); return Err(if o.status.success() { e } else { format!("encoder_failed: {} {}", e, String::from_utf8_lossy(&o.stderr).trim()) }); }
    if !o.status.success() { let _ = std::fs::remove_file(&tmp); return Err(format!("encoder_failed: {}", String::from_utf8_lossy(&o.stderr).trim())); }
    std::fs::rename(&tmp, &out).map_err(|e| e.to_string())?;
    let mut audio = json!({"included": false});
    if want_audio {
        let sr = crate::audio::SR as f64; let (s0, s1) = ((f0 as f64 / fps * sr) as usize * 2, (f1 as f64 / fps * sr) as usize * 2);
        let full = crate::audio::mix(&ss.project); let (x, y) = (s0.min(full.len()), s1.min(full.len())); let mut pcm = full[x..y].to_vec(); pcm.resize(s1 - s0, 0.0);
        let bytes: Vec<u8> = pcm.iter().flat_map(|v| v.to_le_bytes()).collect(); let audible = pcm.iter().any(|v| v.abs() > 1e-6);
        if audible { crate::color::mux::legacy_audio(out.to_str().ok_or("invalid path")?, &pcm, n)?; }
        audio = json!({"included": audible, "samples": pcm.len() / 2, "pcm_sha256": sha256_hex(&bytes), "audible": audible});
    }
    let mut r = json!({"path": out.to_str(), "frames": n, "width": nw, "height": nh, "fps": ofps, "scale": scale, "fps_divisor": div, "start": f0 as f64 / fps, "end": f1 as f64 / fps, "duration": n as f64 / ofps, "audio": audio,
        "timing_ms": {"first_frame": first_ms, "total": t_all.elapsed().as_secs_f64() * 1000.0}, "bytes": std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0), "note": "draft quality (x264 ultrafast crf 28); not a delivery encode"});
    if want_hashes { r["frame_hashes"] = json!(hashes); }
    Ok(r)
}
pub fn commands() -> Vec<Command> { vec![
    Command { name: "preview_range", mutates: false,
        description: "Render a cheap draft mp4 preview of a time range through the same path as export. Draft preview of a time range as a short mp4 through the same composition path as export (same frame renderer and colour pipeline), cheaply: scale (box-downscaled, default 0.5, 1 = full size), fps_divisor (render every Nth frame, default 1), audio (default false; the mixed PCM for the range muxed as AAC). Bounded to 30 s by default (max_seconds up to 300). Read-only: the project is not changed and the path must not be inside an export job's chunk directory. hashes:true returns the sha256 of every rendered RGBA8 frame (after scaling) so a preview can be compared with export frames. Draft quality, not a delivery encode. Returns path, frames, size, timing_ms {first_frame, total}.",
        schema: obj(vec![("start", n("range start, seconds (default 0)")), ("end", n("range end, seconds (default start + max_seconds, capped at the timeline end)")), ("scale", n("0.1..1, default 0.5")), ("fps_divisor", n("whole number 1..30, default 1")), ("audio", json!({"type":"boolean"})), ("max_seconds", n("bound on the range, default 30, max 300")), ("path", s("output .mp4 (default: a temp file)")), ("hashes", json!({"type":"boolean","description":"return per-frame sha256 of the RGBA8 frames"}))], &[]),
        run: preview },
] }
