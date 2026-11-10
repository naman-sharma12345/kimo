//! Inline image blocks for MCP tool results. Core hands over only `Media{path,w,h}`; this is the one place that reads and base64-encodes.
//! Disk files are never touched. Inline copy: scale==1 png -> the file bytes as is; otherwise ffmpeg `scale=W:H:flags=area` (deterministic box filter).
use kimo_core::commands::Media;
use serde_json::{json, Value};
use std::process::{Command, Stdio};

fn max_one() -> usize { std::env::var("KIMO_INLINE_CAP_BYTES").ok().and_then(|v| v.parse().ok()).unwrap_or(1 << 20) } // env: test hook for the 1 MB per-image cap
const MAX_IMGS: usize = 4; const MAX_TOTAL_B64: usize = 2 << 20;

pub fn b64(d: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(d.len().div_ceil(3) * 4);
    for c in d.chunks(3) { let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        o.push(T[(n >> 18) as usize & 63] as char); o.push(T[(n >> 12) as usize & 63] as char);
        o.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' }); o.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' }); }
    o
}
fn sha256_hex(path: &str) -> Option<String> {
    let o = Command::new("sha256sum").arg(path).output().ok()?; if !o.status.success() { return None; }
    String::from_utf8_lossy(&o.stdout).split_whitespace().next().map(String::from)
}
fn ffmpeg_scaled(path: &str, w: u32, h: u32, fmt: &str, q: u32) -> Result<Vec<u8>, String> {
    // deadline + output cap through the engine's bounded child helper (portable; no coreutils `timeout`, no helper thread here)
    let exe = std::env::var("KIMO_INLINE_FFMPEG").unwrap_or_else(|_| "ffmpeg".into()); // env: test hook for the missing-ffmpeg path
    let mut c = Command::new(exe); c.args(["-v", "error", "-i", path, "-vf", &format!("scale={w}:{h}:flags=area")]);
    if fmt == "jpeg" { c.args(["-q:v", &(2 + (100 - q.clamp(1, 100)) * 29 / 100).to_string(), "-f", "mjpeg"]); } else { c.args(["-f", "image2pipe", "-c:v", "png"]); }
    c.args(["-frames:v", "1", "-"]); c.stdin(Stdio::null());
    let out = kimo_core::ext::kit::capture(&mut c, 3 << 20, std::time::Duration::from_secs(20)).map_err(|e| format!("ffmpeg unavailable or failed ({e})"))?;
    if out.is_empty() { Err("empty output".into()) } else { Ok(out) }
}

/// Build (image content blocks, structured media entries) for the media a call produced. Never fails the tool: problems become notes.
pub fn blocks(media: &[Media], args: &Value) -> (Vec<Value>, Vec<Value>, Vec<String>) {
    let (mut blk, mut ent, mut notes) = (vec![], vec![], vec![]);
    let mode = args.get("inline").and_then(|v| v.as_str()).unwrap_or("auto");
    let env_off = std::env::var("KIMO_INLINE_IMAGES").map(|v| v == "0").unwrap_or(false);
    let on = mode == "always" || (mode == "auto" && !env_off);
    let cap = args.get("max_inline_px").and_then(|v| v.as_f64()).map(|v| (v as u32).clamp(128, 2048)).unwrap_or(1024);
    let want_jpeg = args.get("inline_format").and_then(|v| v.as_str()) == Some("jpeg");
    let q = args.get("inline_quality").and_then(|v| v.as_f64()).map(|v| v as u32).unwrap_or(80);
    let mut total = 0usize;
    for (i, m) in media.iter().enumerate() {
        let mut e = json!({"path": m.path, "width": m.w, "height": m.h});
        if let Some(h) = sha256_hex(&m.path) { e["sha256"] = json!(h); }
        if !on { e["inline"] = json!({"omitted": if mode == "never" { "inline:never" } else { "KIMO_INLINE_IMAGES=0" }}); ent.push(e); continue; }
        if i >= MAX_IMGS { notes.push("inline omitted: more than 4 images; use path".into()); ent.push(e); continue; }
        let long = m.w.max(m.h); let scale = if long > cap { cap as f64 / long as f64 } else { 1.0 };
        let (tw, th) = if scale < 1.0 { (((m.w as f64 * scale).round() as u32).max(1), ((m.h as f64 * scale).round() as u32).max(1)) } else { (m.w, m.h) };
        let mut fmt = if want_jpeg { "jpeg" } else { "png" };
        let mut res = if scale == 1.0 && fmt == "png" { std::fs::read(&m.path).map_err(|e| e.to_string()) } else { ffmpeg_scaled(&m.path, tw, th, fmt, q) };
        if fmt == "png" { if let Ok(b) = &res { if b.len() > max_one() { fmt = "jpeg"; res = ffmpeg_scaled(&m.path, tw, th, "jpeg", q); } } }
        let mut bytes = match res { Ok(b) => b, Err(er) => { notes.push(format!("inline omitted: {er}; use path")); ent.push(e); continue; } };
        // reduce quality / size to a floor if still over the per-image cap
        let mut qq = q; while bytes.len() > max_one() && fmt == "jpeg" && qq > 30 { qq -= 20; match ffmpeg_scaled(&m.path, tw, th, "jpeg", qq) { Ok(b) => bytes = b, Err(_) => break } }
        let enc_len = bytes.len().div_ceil(3) * 4;
        if bytes.len() > max_one() || total + enc_len > MAX_TOTAL_B64 { notes.push(format!("inline omitted: {} bytes over the cap; use path", bytes.len())); ent.push(e); continue; }
        total += enc_len;
        let mime = if fmt == "jpeg" { "image/jpeg" } else { "image/png" };
        blk.push(json!({"type":"image","data": b64(&bytes),"mimeType": mime}));
        e["inline"] = json!({"scale": scale, "bytes": bytes.len(), "format": fmt, "width": tw, "height": th}); ent.push(e);
    }
    (blk, ent, notes)
}
