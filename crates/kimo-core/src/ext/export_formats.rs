//! Delivery and mastering formats beyond the presets: every ProRes flavour (Proxy to 4444 XQ with alpha), DNxHR
//! LB/SQ/HQ/HQX/444, MXF OP1a (DNxHR with 24-bit PCM, the broadcast and Avid handoff), WebM with alpha and Opus sound,
//! 10-bit HEVC, NVIDIA hardware H.264/HEVC, and EXR / DPX / TGA / 16-bit PNG image sequences.
use super::kit::*;
use std::process::Command as Cmd;

const FORMATS: &[(&str, &str, &str)] = &[
    ("prores_proxy", ".mov", "Apple ProRes 422 Proxy (offline editing)"), ("prores_lt", ".mov", "Apple ProRes 422 LT"), ("prores_422", ".mov", "Apple ProRes 422"), ("prores_hq", ".mov", "Apple ProRes 422 HQ"),
    ("prores_4444", ".mov", "Apple ProRes 4444 with alpha"), ("prores_4444xq", ".mov", "Apple ProRes 4444 XQ with alpha (mastering)"),
    ("dnxhr_lb", ".mov", "Avid DNxHR LB (offline)"), ("dnxhr_sq", ".mov", "Avid DNxHR SQ"), ("dnxhr_hq", ".mov", "Avid DNxHR HQ"), ("dnxhr_hqx", ".mov", "Avid DNxHR HQX 10-bit"), ("dnxhr_444", ".mov", "Avid DNxHR 444 10-bit"),
    ("mxf_dnxhr", ".mxf", "MXF OP1a with DNxHR HQ and 24-bit PCM (broadcast / Avid)"), ("webm_alpha", ".webm", "WebM VP9 with alpha and Opus sound (transparent web video)"), ("webm_opus", ".webm", "WebM VP9 with Opus sound"),
    ("hevc_10bit", ".mp4", "HEVC Main 10 (libx265, 10-bit 4:2:0)"), ("h264_nvenc", ".mp4", "H.264 on an NVIDIA GPU (NVENC)"), ("hevc_nvenc", ".mp4", "HEVC on an NVIDIA GPU (NVENC)"),
    ("exr_sequence", ".exr", "OpenEXR image sequence (path with %05d)"), ("dpx_sequence", ".dpx", "10-bit DPX image sequence (path with %05d)"), ("tga_sequence", ".tga", "Targa image sequence with alpha (path with %05d)"), ("png16_sequence", ".png", "16-bit PNG sequence with alpha (path with %05d)"),
];

fn base(p: &crate::model::Project) -> Cmd { let mut c = Cmd::new("ffmpeg"); c.args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{}x{}", p.width, p.height), "-r", &p.fps.to_string(), "-i", "-"]); c }
const TAGS: [&str; 8] = ["-color_range", "tv", "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709"];

/// Mux the timeline's mix into a picture-only file with the given audio codec (MXF wants PCM, WebM Opus).
fn mux_audio(p: &crate::model::Project, video: &str, codec: &[&str], t0: f64, t1: f64) -> Result<(), String> {
    let pcm = crate::audio::mix(p); let sr = crate::audio::SR; let (i0, i1) = (((t0 * sr as f64) as usize * 2).min(pcm.len()), ((t1 * sr as f64) as usize * 2).min(pcm.len()));
    let seg = &pcm[i0..i1.max(i0)]; if seg.iter().all(|v| *v == 0.0) { return Ok(()); }
    let wav = std::env::temp_dir().join(format!("kimo-fmt-mux-{}.wav", std::process::id())); let mut b = Vec::new(); let data = (seg.len() * 4) as u32;
    b.extend_from_slice(b"RIFF"); b.extend_from_slice(&(36 + data).to_le_bytes()); b.extend_from_slice(b"WAVEfmt "); b.extend_from_slice(&16u32.to_le_bytes()); b.extend_from_slice(&3u16.to_le_bytes()); b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&(sr as u32).to_le_bytes()); b.extend_from_slice(&((sr * 8) as u32).to_le_bytes()); b.extend_from_slice(&8u16.to_le_bytes()); b.extend_from_slice(&32u16.to_le_bytes()); b.extend_from_slice(b"data"); b.extend_from_slice(&data.to_le_bytes());
    for v in seg { b.extend_from_slice(&v.to_le_bytes()); } std::fs::write(&wav, b).map_err(|e| e.to_string())?;
    let ext = std::path::Path::new(video).extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default(); let tmp = format!("{video}.mux.{ext}");
    let ok = Cmd::new("ffmpeg").args(["-v", "error", "-y", "-i", video]).arg("-i").arg(&wav).args(["-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy"]).args(codec).args(["-shortest", &tmp]).status().map(|s| s.success()).unwrap_or(false);
    let _ = std::fs::remove_file(&wav); if !ok { let _ = std::fs::remove_file(&tmp); return Err("could not mux the sound".into()); }
    std::fs::rename(&tmp, video).map_err(|e| e.to_string())
}

fn export_format(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let p = &ss.project; let path = str_arg(a, "path")?.to_string(); let fmt = str_arg(a, "format")?.to_string();
    let (_, ext, _) = FORMATS.iter().find(|f| f.0 == fmt).ok_or_else(|| format!("unknown format '{fmt}'; formats: {}", FORMATS.iter().map(|f| f.0).collect::<Vec<_>>().join(", ")))?;
    if !path.to_lowercase().ends_with(ext) { return Err(format!("{fmt} needs a {ext} path")); }
    let seq = fmt.ends_with("_sequence"); if seq && !path.contains('%') { return Err("an image sequence path needs a frame number pattern, e.g. shot_%05d.exr".into()); }
    if !seq && std::path::Path::new(&path).exists() && !a.get("overwrite").and_then(|v| v.as_bool()).unwrap_or(false) { return Err(format!("{path} exists; pass overwrite: true to replace it")); }
    if let Some(parent) = std::path::Path::new(&path).parent() { if !parent.as_os_str().is_empty() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; } }
    let (t0, t1) = (opt_num(a, "start", 0.0), opt_num(a, "end", p.duration()).min(p.duration())); if !(t0 >= 0.0 && t1 > t0) { return Err("range must satisfy 0 <= start < end".into()); }
    let (f0, f1) = ((t0 * p.fps).round() as usize, ((t1 * p.fps).round() as usize).max((t0 * p.fps).round() as usize + 1));
    let mut c = base(p); let (mut alpha, mut audio) = (false, true); let mut after: Option<&[&str]> = None;
    let prores = |c: &mut Cmd, prof: &str, a: bool| { c.args(["-vf", &format!("scale=in_range=pc:out_color_matrix=bt709:out_range=tv,format={}", if a { "yuva444p10le" } else { "yuv422p10le" }), "-c:v", "prores_ks", "-profile:v", prof, "-pix_fmt", if a { "yuva444p10le" } else { "yuv422p10le" }, "-vendor", "apl0"]); if a { c.args(["-alpha_bits", "16"]); } c.args(TAGS); };
    let dnx = |c: &mut Cmd, prof: &str, pix: &str| { c.args(["-vf", &format!("scale=in_range=pc:out_color_matrix=bt709:out_range=tv,format={pix}"), "-c:v", "dnxhd", "-profile:v", prof, "-pix_fmt", pix]); c.args(TAGS); };
    if fmt.starts_with("dnxhr") || fmt == "mxf_dnxhr" { if p.width < 256 || p.height < 120 { return Err(format!("DNxHR needs at least 256x120; the project is {}x{}", p.width, p.height)); } }
    match fmt.as_str() {
        "prores_proxy" => prores(&mut c, "0", false), "prores_lt" => prores(&mut c, "1", false), "prores_422" => prores(&mut c, "2", false), "prores_hq" => prores(&mut c, "3", false),
        "prores_4444" => { prores(&mut c, "4", true); alpha = true; } "prores_4444xq" => { prores(&mut c, "5", true); alpha = true; }
        "dnxhr_lb" => dnx(&mut c, "dnxhr_lb", "yuv422p"), "dnxhr_sq" => dnx(&mut c, "dnxhr_sq", "yuv422p"), "dnxhr_hq" => dnx(&mut c, "dnxhr_hq", "yuv422p"), "dnxhr_hqx" => dnx(&mut c, "dnxhr_hqx", "yuv422p10le"), "dnxhr_444" => dnx(&mut c, "dnxhr_444", "yuv444p10le"),
        "mxf_dnxhr" => { dnx(&mut c, "dnxhr_hq", "yuv422p"); c.args(["-f", "mxf"]); audio = false; after = Some(&["-c:a", "pcm_s24le", "-ar", "48000"]); }
        "webm_alpha" => { c.args(["-vf", "format=yuva420p", "-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "30", "-pix_fmt", "yuva420p", "-auto-alt-ref", "0"]); alpha = true; audio = false; after = Some(&["-c:a", "libopus", "-b:a", "160k"]); }
        "webm_opus" => { c.args(["-vf", "scale=in_range=pc:out_color_matrix=bt709:out_range=tv,format=yuv420p", "-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "31", "-pix_fmt", "yuv420p"]); c.args(TAGS); audio = false; after = Some(&["-c:a", "libopus", "-b:a", "160k"]); }
        "hevc_10bit" => { c.args(["-vf", "scale=in_range=pc:out_color_matrix=bt709:out_range=tv,format=yuv420p10le", "-c:v", "libx265", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p10le", "-profile:v", "main10", "-tag:v", "hvc1"]); c.args(TAGS); }
        "h264_nvenc" | "hevc_nvenc" => { c.args(["-vf", "scale=in_range=pc:out_color_matrix=bt709:out_range=tv,format=yuv420p", "-c:v", &fmt, "-preset", "p5", "-rc", "vbr", "-cq", "19", "-b:v", "0", "-pix_fmt", "yuv420p"]); if fmt == "hevc_nvenc" { c.args(["-tag:v", "hvc1"]); } c.args(TAGS); }
        "exr_sequence" => { c.args(["-c:v", "exr", "-pix_fmt", "gbrapf32le", "-compression", "3"]); alpha = true; audio = false; }
        "dpx_sequence" => { c.args(["-c:v", "dpx", "-pix_fmt", "gbrp10le"]); audio = false; }
        "tga_sequence" => { c.args(["-c:v", "targa", "-pix_fmt", "bgra"]); alpha = true; audio = false; }
        "png16_sequence" => { c.args(["-c:v", "png", "-pix_fmt", "rgba64be"]); alpha = true; audio = false; }
        _ => unreachable!(),
    }
    if seq { c.args(["-start_number", "0"]); }
    c.arg(&path);
    let d = crate::render::export_range_cmd(p, &path, f0, f1, c, alpha, audio).map_err(|e| if fmt.ends_with("nvenc") { format!("{e} (NVENC needs an NVIDIA GPU and driver)") } else { e })?;
    if let Some(codec) = after { mux_audio(p, &path, codec, t0, t1)?; }
    let files = if seq { (f1 - f0) as u64 } else { 1 };
    Ok(json!({"path": path, "format": fmt, "duration": d, "files": files, "alpha": alpha}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "export_format", description: "Export in a delivery or mastering format: ProRes Proxy/LT/422/HQ/4444/4444 XQ (alpha), DNxHR LB/SQ/HQ/HQX/444, MXF OP1a (DNxHR + 24-bit PCM), WebM VP9 with alpha or Opus, HEVC 10-bit, NVIDIA NVENC H.264/HEVC, EXR/DPX/TGA/16-bit PNG sequences (path with %05d). Optional start/end range.", mutates: false,
        schema: obj(vec![("path", s("output path (a %05d pattern for sequences)")), ("format", json!({"type":"string","enum":FORMATS.iter().map(|f| f.0).collect::<Vec<_>>()})), ("start", n("s")), ("end", n("s")), ("overwrite", json!({"type":"boolean"}))], &["path","format"]),
        run: export_format },
] }
