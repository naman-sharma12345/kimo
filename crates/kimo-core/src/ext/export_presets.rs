use super::kit::*;
use std::process::Command as Cmd;

const PRESETS: &[(&str, &str, &str)] = &[
    ("web-1080", ".mp4", "H.264 yuva-free yuv420p, CRF 18 medium, +faststart, AAC 192k. Rec709 only. Downscale-to-fit 1920x1080 (or 1080x1920 portrait), aspect kept, even dimensions, never upscaled, scaled in the encoder (lanczos), not by changing the project canvas."),
    ("master", ".mkv", "Lossless 16-bit RGB integer delivery (FFV1 gbrp16le) at the project canvas and fps. Same encoder as export_high_bit ffv1_16. Integer RGB, not float radiance."),
    ("alpha-prores", ".mov", "ProRes 4444 (prores_ks profile 4, yuva444p10le, alpha_bits 16), transparent canvas, straight alpha. Rec709 only. Audio not muxed yet."),
];

/// Output size for web-1080: fit inside 1920x1080 (landscape/square) or 1080x1920 (portrait), no upscale, even dims.
pub fn fit_web(w: usize, h: usize) -> (usize, usize) {
    let (mw, mh) = if h > w { (1080.0, 1920.0) } else { (1920.0, 1080.0) };
    let f = (mw / w as f64).min(mh / h as f64).min(1.0);
    let ev = |v: f64| ((v.round() as usize) & !1usize).max(2);
    if f >= 1.0 { (ev(w as f64), ev(h as f64)) } else { (ev(w as f64 * f), ev(h as f64 * f)) }
}

fn check_ext(path: &str, ext: &str) -> Result<(), String> { if path.to_lowercase().ends_with(ext) { Ok(()) } else { Err(format!("this preset requires a {ext} output path")) } }

pub fn commands() -> Vec<Command> { vec![
Command { name: "inspect_export_preset", description: "Describe a named export preset (web-1080, master, alpha-prores) or list all: container, codec, color rules and the output size it would produce for the current project.", mutates: false,
    schema: obj(vec![("name", s("preset name, omit to list"))], &[]),
    run: |ss, a| { let one = |n: &str, e: &str, d: &str| { let mut v = json!({"name":n,"extension":e,"description":d}); if n == "web-1080" { let (w, h) = fit_web(ss.project.width as usize, ss.project.height as usize); v["output_size"] = json!([w, h]); } v };
        match a.get("name").and_then(|v| v.as_str()) { Some(n) => { let p = PRESETS.iter().find(|p| p.0 == n).ok_or("unknown preset")?; Ok(one(p.0, p.1, p.2)) }, None => Ok(json!({"presets": PRESETS.iter().map(|p| one(p.0, p.1, p.2)).collect::<Vec<_>>()})) } } },
Command { name: "export_preset", description: "Export with a named preset: web-1080 (.mp4, H.264/AAC, fit 1080p), master (.mkv, FFV1 16-bit RGB lossless) or alpha-prores (.mov, ProRes 4444 with alpha). Container/extension enforced. Optional start/end seconds (web-1080 and alpha-prores). Ordinary export is unchanged.", mutates: false,
    schema: obj(vec![("path", s("output path; extension must match the preset")), ("preset", json!({"type":"string","enum":["web-1080","master","alpha-prores"]})), ("start", n("range start s, default 0")), ("end", n("range end s, default end"))], &["path","preset"]),
    run: |ss, a| { let p = &ss.project; let path = str_arg(a, "path")?; let preset = str_arg(a, "preset")?;
        let (t_in, t_out) = (opt_num(a, "start", 0.0), opt_num(a, "end", p.duration()));
        if !t_in.is_finite() || !t_out.is_finite() || t_in < 0.0 || t_out <= t_in { return Err("range must satisfy 0 <= start < end".into()); }
        let t_out = t_out.min(p.duration()); if t_in >= t_out { return Err("range starts after the end of the timeline".into()); }
        let f0 = (t_in * p.fps).round() as usize; let f1 = ((t_out * p.fps).round() as usize).max(f0 + 1);
        match preset {
            "master" => { check_ext(path, ".mkv")?; if a.get("start").is_some() || a.get("end").is_some() { return Err("master preset exports the whole timeline (no range)".into()); } let d = crate::color::pipeline::export_high(p, path, "ffv1_16", 18)?; Ok(json!({"path":path,"duration":d,"preset":preset})) }
            "web-1080" => { check_ext(path, ".mp4")?;
                // reuse the ordinary encoder's color validation (Rec709 only, no HDR)
                let _ = crate::render::legacy_encoder_command(p, path, 18)?; if p.color_pipeline.enabled && [6, 7].contains(&p.color_pipeline.output_transfer) { return Err("web-1080 is SDR Rec709; HDR output is not converted".into()); }
                let (w, h) = fit_web(p.width as usize, p.height as usize);
                let trc = if p.color_pipeline.enabled { match p.color_pipeline.output_transfer { 0 => "linear", 8 => "bt709", _ => "iec61966-2-1" } } else { "iec61966-2-1" };
                let mut cmd = Cmd::new("ffmpeg"); cmd.args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{}x{}", p.width, p.height), "-r", &p.fps.to_string(), "-i", "-", "-c:v", "libx264", "-preset", "medium", "-crf", "18",
                    "-vf", &format!("scale={w}:{h}:flags=lanczos:in_range=pc:out_color_matrix=bt709:out_range=tv"), "-pix_fmt", "yuv420p", "-color_range", "tv", "-color_primaries", "bt709", "-color_trc", trc, "-colorspace", "bt709", "-movflags", "+faststart", path]);
                let d = crate::render::export_range_cmd(p, path, f0, f1, cmd, false, true)?;
                // audio mux rewrites the file; restore faststart
                let tmp = format!("{path}.fs.tmp.mp4"); let ok = Cmd::new("ffmpeg").args(["-v", "error", "-y", "-i", path, "-c", "copy", "-movflags", "+faststart", &tmp]).status().map(|s| s.success()).unwrap_or(false);
                if ok { std::fs::rename(&tmp, path).map_err(|e| e.to_string())?; } else { let _ = std::fs::remove_file(&tmp); }
                Ok(json!({"path":path,"duration":d,"preset":preset,"output_size":[w,h]})) }
            "alpha-prores" => { check_ext(path, ".mov")?; if p.color_pipeline.enabled { return Err("alpha-prores supports the display-sRGB Rec709 project only for now; disable the managed color pipeline".into()); }
                let mut cmd = Cmd::new("ffmpeg"); cmd.args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{}x{}", p.width, p.height), "-r", &p.fps.to_string(), "-i", "-", "-vf", "scale=in_range=pc:out_color_matrix=bt709:out_range=tv,format=yuva444p10le", "-c:v", "prores_ks", "-profile:v", "4", "-pix_fmt", "yuva444p10le", "-alpha_bits", "16", "-vendor", "apl0", "-color_range", "tv", "-color_primaries", "bt709", "-color_trc", "iec61966-2-1", "-colorspace", "bt709", "-bsf:v", "prores_metadata=color_primaries=bt709:color_trc=13:colorspace=bt709", "-movflags", "+write_colr", path]);
                let d = crate::render::export_range_cmd(p, path, f0, f1, cmd, true, false)?; Ok(json!({"path":path,"duration":d,"preset":preset})) }
            _ => Err("unknown preset".into()),
        } } },
] }
#[cfg(test)] mod tests { use super::fit_web;
    #[test] fn fit_rules() { assert_eq!(fit_web(2560, 1440), (1920, 1080)); assert_eq!(fit_web(1000, 563), (1000, 562)); assert_eq!(fit_web(1080, 1920), (1080, 1920)); assert_eq!(fit_web(1280, 720), (1280, 720)); assert_eq!(fit_web(2160, 3840), (1080, 1920)); assert_eq!(fit_web(641, 361), (640, 360)); } }
