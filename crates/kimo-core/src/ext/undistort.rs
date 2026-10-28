//! undistort_clip: remove radial lens distortion (Brown k1,k2) from a media clip and write a new video, so solved 3D camera / scene3d layers line up with the plate.
use super::kit::*;
use std::io::Write;

/// Output pixel (ideal pinhole) -> source pixel (distorted) under the Brown radial model used by the camera solver.
pub fn distort_px(x: f64, y: f64, f: f64, cx: f64, cy: f64, k1: f64, k2: f64) -> (f64, f64) { let (a, b) = ((x - cx) / f, (y - cy) / f); let r2 = a * a + b * b; let s = 1.0 + k1 * r2 + k2 * r2 * r2; (f * a * s + cx, f * b * s + cy) }
/// Remap an RGBA float frame (w*h*4) to remove distortion; outside the source -> transparent black. Bilinear.
pub fn undistort_frame(src: &[f32], w: usize, h: usize, f: f64, cx: f64, cy: f64, k1: f64, k2: f64) -> Vec<f32> {
    let mut o = vec![0f32; w * h * 4];
    for y in 0..h { for x in 0..w { let (sx, sy) = distort_px(x as f64 + 0.5, y as f64 + 0.5, f, cx, cy, k1, k2); let (fx, fy) = (sx - 0.5, sy - 0.5); if fx < 0.0 || fy < 0.0 || fx > (w - 1) as f64 || fy > (h - 1) as f64 { continue; }
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize); let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1)); let (ax, ay) = ((fx - x0 as f64) as f32, (fy - y0 as f64) as f32);
        for c in 0..4 { let v = |xx: usize, yy: usize| src[(yy * w + xx) * 4 + c]; o[(y * w + x) * 4 + c] = v(x0, y0) * (1.0 - ax) * (1.0 - ay) + v(x1, y0) * ax * (1.0 - ay) + v(x0, y1) * (1.0 - ax) * ay + v(x1, y1) * ax * ay; } } }
    o
}
pub fn commands() -> Vec<Command> { vec![
    Command { name: "undistort_clip", description: "Remove radial lens distortion from a media clip and encode a new corrected file. Remove radial lens distortion (Brown k1,k2, optional principal point) from a media clip: renders the clip, remaps every frame to an ideal pinhole image with the same focal length, and encodes a new H.264 file (crf 14). Parameters come from a camera solve (solve_id, default latest; uses its focal, principal point, k1, k2) or explicit k1/k2/focal_px/cx/cy. Pixels that map outside the source stay black (barrel distortion removal leaves slight edge loss; no cropping or zoom is applied). With import=true (default) the result is added as a project asset (asset_id) ready for add_clip. After undistorting, re-run solve_camera on the new asset with lens_distortion=false, or use the same solve's cam_* keyframes directly: they assume undistorted footage. Limit: radial model only; no tangential distortion, and the clip is rendered as shown (effects and transforms included; keep transforms at identity).", mutates: true,
        schema: obj(vec![("clip_id", s("media clip")), ("solve_id", s("camera solve to take lens parameters from; default latest if no explicit k1/k2")), ("k1", n("radial k1 (explicit)")), ("k2", n("radial k2 (explicit), default 0")), ("focal_px", n("focal length px (explicit)")), ("cx", n("principal point x px, default centre")), ("cy", n("principal point y px, default centre")), ("start", n("first frame index in clip, default 0")), ("end", n("last frame index, default end")), ("out_path", s("output .mp4 path, default /tmp/kimo_undistort_<clip>.mp4")), ("import", json!({"type":"boolean","description":"add result as a project asset (default true)"}))], &["clip_id"]),
        run: |ss, a| {
            let t0 = std::time::Instant::now(); let cid = str_arg(a, "clip_id")?.to_string(); let fps = ss.project.fps; let (w, h) = (ss.project.width as usize, ss.project.height as usize);
            let (cstart, cdur) = { let c = ss.project.find_clip(&cid).ok_or("no such clip")?; (c.start, c.duration) }; let nf = ((cdur * fps).ceil() as usize).max(1);
            let (src_note, f, cx, cy, k1, k2) = if a.get("k1").is_some() { let f = a.get("focal_px").and_then(|v| v.as_f64()).ok_or("explicit k1 needs focal_px")?; ("explicit".to_string(), f, opt_num(a, "cx", w as f64 / 2.0), opt_num(a, "cy", h as f64 / 2.0), opt_num(a, "k1", 0.0), opt_num(a, "k2", 0.0)) } else {
                let (id, f, cx, cy, k1, k2) = super::camera_solve::solve_lens(a.get("solve_id").and_then(|v| v.as_str())).ok_or("no camera solve found; run solve_camera or give k1+focal_px")?; (id, f, cx, cy, k1, k2) };
            if !f.is_finite() || f < 1.0 || !k1.is_finite() || !k2.is_finite() || k1.abs() > 1.0 || k2.abs() > 1.0 { return Err("bad lens parameters".into()); }
            let f0 = (opt_num(a, "start", 0.0) as usize).min(nf - 1); let f1 = (opt_num(a, "end", (nf - 1) as f64) as usize).min(nf - 1).max(f0);
            let out = a.get("out_path").and_then(|v| v.as_str()).map(|x| x.to_string()).unwrap_or_else(|| format!("/tmp/kimo_undistort_{cid}.mp4"));
            let mut enc = std::process::Command::new("ffmpeg").args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{w}x{h}"), "-r", &format!("{fps}"), "-i", "-", "-c:v", "libx264", "-crf", "14", "-pix_fmt", "yuv420p", &out]).stdin(std::process::Stdio::piped()).spawn().map_err(|e| format!("ffmpeg: {e}"))?;
            let mut sin = enc.stdin.take().ok_or("ffmpeg stdin")?;
            for i in f0..=f1 { let t = cstart + i as f64 / fps; crate::render::prepare_at(&ss.project, &mut ss.cache, Some(t + 1e-4))?; let fr = crate::render::render_clip_alone(&ss.project, &cid, t + 1e-4, &ss.cache).ok_or("could not render the clip")?;
                let u = undistort_frame(&fr.d, w, h, f, cx, cy, k1, k2); let mut rgb = vec![0u8; w * h * 3]; for p in 0..w * h { let al = u[p * 4 + 3]; for c in 0..3 { rgb[p * 3 + c] = ((if al > 1e-4 { u[p * 4 + c] / al.max(1e-4) * al.min(1.0) } else { 0.0 }).clamp(0.0, 1.0) * 255.0 + 0.5) as u8; } }
                sin.write_all(&rgb).map_err(|e| format!("ffmpeg pipe: {e}"))?; }
            drop(sin); if !enc.wait().map_err(|e| e.to_string())?.success() { return Err("ffmpeg encode failed".into()); }
            let mut res = json!({"path": out, "frames": f1 + 1 - f0, "lens": {"k1": k1, "k2": k2, "focal_px": f, "cx": cx, "cy": cy, "from": src_note}, "seconds": t0.elapsed().as_secs_f64()});
            if a.get("import").and_then(|v| v.as_bool()).unwrap_or(true) { let (aw, ah, d, afps) = crate::render::probe(&out)?; let id = ss.project.gen_id("asset"); ss.project.assets.insert(id.clone(), crate::model::Asset { origin:None,hash:None,kind:None,id: id.clone(), path: out.clone(), width: aw, height: ah, duration: d, fps: afps, bin: None }); res["asset_id"] = json!(id); }
            Ok(res) } },
] }
