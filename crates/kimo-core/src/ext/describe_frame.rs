//! describe_frame (RV-25 s1): read-only, deterministic fact sheet for one timeline time, computed from the rendered display-referred frame
//! in ONE scanline pass over the existing decoded buffer (no copies, no RGB conversion): luma mean/variance, 32-bin histogram (+16 rollup),
//! clipping counts, 3x3-free laplacian focus measure, dominant colours, mean saturation, and a 64-bit DCT phash. `roi` restricts the stats to a normalised box.
use super::kit::*;

const R709: [f32; 3] = [0.2126, 0.7152, 0.0722]; // Rec.709 luma coefficients applied to the rendered (display-referred, 0..1) RGB

/// 64-bit DCT perceptual hash from a 32x32 luma grid (row-major): 8x8 low-frequency DCT-II block, every coefficient compared with the median of the 63 AC terms.
pub fn phash64(grid: &[f32; 1024]) -> u64 {
    let mut cos = [[0f32; 32]; 8]; for u in 0..8 { for x in 0..32 { cos[u][x] = ((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI / 64.0).cos(); } }
    let mut tmp = [[0f32; 8]; 32]; // rows transformed along x for 8 frequencies
    for y in 0..32 { for u in 0..8 { let mut s = 0f32; for x in 0..32 { s += grid[y * 32 + x] * cos[u][x]; } tmp[y][u] = s; } }
    let mut c = [0f32; 64]; for v in 0..8 { for u in 0..8 { let mut s = 0f32; for y in 0..32 { s += tmp[y][u] * cos[v][y]; } c[v * 8 + u] = s; } }
    let mut ac: Vec<f32> = c[1..].to_vec(); ac.sort_by(|a, b| a.partial_cmp(b).unwrap()); let med = ac[31];
    let mut h = 0u64; for (i, v) in c.iter().enumerate() { if *v > med { h |= 1u64 << (63 - i); } } h
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "describe_frame", mutates: false,
        description: "Optional isolate (draw only these clips) / hide (draw all except these) take clip ids or @names; render-time only, the project is not changed, and a child keeps its parent-resolved pose while hiding a parent hides only that clip's own pixels. Deterministic fact sheet for the rendered frame at a timeline time, so you can judge composition without pulling pixels. frame_stats over the whole frame or a roi: luma (Rec.709 coefficients 0.2126/0.7152/0.0722 on the rendered display-referred 0..1 RGB) mean and standard deviation; luma_histogram_32 (32 equal-width bins over luma 0..1, bin 31 includes 1.0; counts as fractions) and luma_histogram_16 (exact pairwise sums of the 32); clipped_black_pct (luma < 16/255) and clipped_white_pct (luma > 240/255) from raw pixel counts, not bin edges; laplacian_variance (4-neighbour laplacian on 0..255 luma: low = flat or unrendered, also flagged low_detail); mean_saturation; dominant_colors (3, 4x4x4 quantised, hex). phash64 is a 64-bit DCT perceptual hash of the whole frame (32x32 luma, 8x8 DCT, median threshold) as a 16-hex string, comparable with hamming distance for dedupe and stability checks; it ignores roi. roi = [ymin, xmin, ymax, xmax] normalised 0..1. visible_clips lists clips active at the time (id, track, source type, blend, opacity at that time; capped at 50 with a total). motion:true adds mean_abs_diff_vs_next_frame (luma, next frame at +1/fps).",
        schema: obj(vec![("isolate", crate::ext::view::list_schema(crate::ext::view::ISOLATE_DOC)), ("hide", crate::ext::view::list_schema(crate::ext::view::HIDE_DOC)), ("time", n("timeline seconds")), ("roi", json!({"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4,"description":"[ymin, xmin, ymax, xmax] normalised 0..1; stats only, phash stays whole-frame"})), ("motion", json!({"type":"boolean","description":"also report mean absolute luma difference to the frame one frame later (renders a second frame)"}))], &["time"]),
        run: |ss, a| crate::perception::tryhint::wrap("describe_frame", crate::ext::view::with_view(ss, a, |ss| {
            let t = num_arg(a, "time")?; if !t.is_finite() { return Err("time must be finite".into()); }
            let roi = match a.get("roi") { None | Some(Value::Null) => [0.0, 0.0, 1.0, 1.0], Some(v) => { let r: Vec<f64> = v.as_array().filter(|x| x.len() == 4).map(|x| x.iter().filter_map(|y| y.as_f64()).collect()).filter(|x: &Vec<f64>| x.len() == 4).ok_or("roi must be [ymin, xmin, ymax, xmax] (4 numbers)")?; [r[0], r[1], r[2], r[3]] } };
            if !roi.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)) || roi[0] >= roi[2] || roi[1] >= roi[3] { return Err("roi must be [ymin, xmin, ymax, xmax] in 0..1 with min < max".into()); }
            let render = |ss: &mut Session, tt: f64| -> Result<crate::effects::Frame, String> { crate::render::prepare_at(&ss.project, &mut ss.cache, Some(tt))?; let mut f = crate::framecache::render_cached(&mut ss.fcache, &ss.project, tt, &ss.cache)?; crate::color::pipeline::output(&ss.project, &mut f)?; Ok(f) };
            let f = render(ss, t)?; let (w, h) = (f.w, f.h); if w < 3 || h < 3 { return Err("frame too small to describe".into()); }
            let (x0, x1) = ((roi[1] * w as f64).floor() as usize, ((roi[3] * w as f64).ceil() as usize).min(w)); let (y0, y1) = ((roi[0] * h as f64).floor() as usize, ((roi[2] * h as f64).ceil() as usize).min(h));
            let (rw, rh) = (x1.saturating_sub(x0), y1.saturating_sub(y0)); if rw == 0 || rh == 0 { return Err("roi covers no pixels".into()); }
            // ---- one pass: row-strided reads of f.d; luma kept in three rolling rows for the laplacian; 32x32 phash grid accumulated on the full frame
            let d = &f.d; let mut hist = [0u64; 32]; let (mut sum, mut sumsq, mut sat) = (0f64, 0f64, 0f64); let (mut nb, mut nw) = (0u64, 0u64); let mut cq = [0u64; 64];
            let (mut lsum, mut lsq, mut ln) = (0f64, 0f64, 0u64); let mut rows = [vec![0f32; rw], vec![0f32; rw], vec![0f32; rw]];
            let mut grid = [0f32; 1024]; let mut gcnt = [0u32; 1024];
            for y in 0..h {
                let inroi = y >= y0 && y < y1; let gy = y * 32 / h;
                let cur = (y.wrapping_sub(y0)) % 3;
                for x in 0..w { let i = (y * w + x) * 4; let (r, g, b) = (d[i].clamp(0.0, 1.0), d[i + 1].clamp(0.0, 1.0), d[i + 2].clamp(0.0, 1.0)); let l = R709[0] * r + R709[1] * g + R709[2] * b;
                    let gi = gy * 32 + x * 32 / w; grid[gi] += l; gcnt[gi] += 1;
                    if inroi && x >= x0 && x < x1 { let ld = l as f64; sum += ld; sumsq += ld * ld; hist[((l * 32.0) as usize).min(31)] += 1;
                        let l8 = l * 255.0; if l8 < 16.0 { nb += 1; } if l8 > 240.0 { nw += 1; }
                        let mx = r.max(g).max(b); let mn = r.min(g).min(b); if mx > 0.0 { sat += ((mx - mn) / mx) as f64; }
                        cq[(((r * 3.999) as usize) << 4) | (((g * 3.999) as usize) << 2) | ((b * 3.999) as usize)] += 1; rows[cur][x - x0] = l * 255.0; } }
                if inroi && y >= y0 + 2 { let (up, mid, dn) = (&rows[(y - 2 - y0) % 3], &rows[(y - 1 - y0) % 3], &rows[(y - y0) % 3]); for x in 1..rw.saturating_sub(1) { let lap = (up[x] + dn[x] + mid[x - 1] + mid[x + 1] - 4.0 * mid[x]) as f64; lsum += lap; lsq += lap * lap; ln += 1; } }
            }
            drop(f);
            let np = (rw * rh) as f64; let mean = sum / np; let var = (sumsq / np - mean * mean).max(0.0);
            let frac32: Vec<f64> = hist.iter().map(|c| *c as f64 / np).collect(); let frac16: Vec<f64> = (0..16).map(|i| (hist[2 * i] + hist[2 * i + 1]) as f64 / np).collect();
            for (g, c) in grid.iter_mut().zip(gcnt.iter()) { if *c > 0 { *g /= *c as f32; } }
            let ph = phash64(&grid);
            let lapvar = if ln > 0 { (lsq / ln as f64 - (lsum / ln as f64).powi(2)).max(0.0) } else { 0.0 };
            let mut order: Vec<usize> = (0..64).collect(); order.sort_by(|a, b| cq[*b].cmp(&cq[*a]).then(a.cmp(b)));
            let dom: Vec<Value> = order.iter().take(3).filter(|i| cq[**i] > 0).map(|i| { let (r, g, b) = ((i >> 4) & 3, (i >> 2) & 3, i & 3); json!({"hex": format!("#{:02x}{:02x}{:02x}", r * 85, g * 85, b * 85), "pct": cq[*i] as f64 / np * 100.0}) }).collect();
            let mut out = json!({"time": t, "canvas": {"width": w, "height": h}, "roi": roi, "roi_px": {"x0": x0, "y0": y0, "x1": x1, "y1": y1},
                "frame_stats": {"luma_coefficients": "rec709 0.2126 0.7152 0.0722", "mean_luma": mean, "stddev_luma": var.sqrt(), "luma_histogram_32": frac32, "luma_histogram_16": frac16,
                    "clipped_black_pct": nb as f64 / np * 100.0, "clipped_white_pct": nw as f64 / np * 100.0, "laplacian_variance": lapvar, "low_detail": lapvar < 1.0, "mean_saturation": sat / np, "dominant_colors": dom},
                "phash64": format!("{:016x}", ph)});
            // clips active at t (ids and numbers only; no project text)
            let mut vis = vec![]; let mut total = 0usize; for (ti, tr) in ss.project.tracks.iter().enumerate() { for c in &tr.clips { if t >= c.start && t < c.start + c.duration {
                total += 1; if vis.len() < 50 { let lt = t - c.start; let op = if c.props.contains_key("opacity") { crate::render::resolve_xform(&ss.project, c, lt).opacity } else { 1.0 }; let dbg = format!("{:?}", c.source); let ty = dbg.split(|ch: char| !ch.is_alphanumeric()).next().unwrap_or("").to_string();
                    vis.push(json!({"clip_id": c.id, "track": ti, "source_type": ty, "blend": c.blend, "opacity": op})); } } } }
            out["visible_clips"] = json!(vis); out["visible_clips_total"] = json!(total);
            let mut warnings = vec![]; if total == 0 { warnings.push("no clip is visible at this time"); } if mean < 0.004 && nb as f64 / np > 0.999 { warnings.push("frame is black"); } if nw as f64 / np > 0.999 { warnings.push("frame is white"); } if lapvar < 1.0 { warnings.push("very low detail: flat, blank or unrendered asset"); }
            if a.get("motion").and_then(|v| v.as_bool()).unwrap_or(false) { let t2 = t + 1.0 / ss.project.fps.max(1.0); let f2 = render(ss, t2)?; let f1 = render(ss, t)?; let (mut s, mut cn) = (0f64, 0u64);
                for y in (0..h).step_by(2) { for x in (0..w).step_by(2) { let i = (y * w + x) * 4; let l1 = R709[0] * f1.d[i].clamp(0.0, 1.0) + R709[1] * f1.d[i + 1].clamp(0.0, 1.0) + R709[2] * f1.d[i + 2].clamp(0.0, 1.0); let l2 = R709[0] * f2.d[i].clamp(0.0, 1.0) + R709[1] * f2.d[i + 1].clamp(0.0, 1.0) + R709[2] * f2.d[i + 2].clamp(0.0, 1.0); s += (l1 - l2).abs() as f64; cn += 1; } }
                out["motion"] = json!({"mean_abs_diff_vs_next_frame": s / cn.max(1) as f64, "sampling": "every 2nd pixel in x and y, luma 0..1"}); }
            out["warnings"] = json!(warnings);
            let clipped = nw as f64 / np * 100.0; let tl = if total == 0 { "no visible clips".to_string() } else { format!("{total} visible clip{}", if total == 1 { "" } else { "s" }) };
            out["summary"] = json!(format!("Frame at {:.2}s is {}x{}: mean luma {:.2}, {}, {:.1}% clipped white, {}.", t, w, h, mean, tl, clipped, if lapvar < 1.0 { "low detail" } else { "detailed" }));
            let mut steps = vec![];
            if total == 0 { steps.push(json!({"tool":"get_project","args":{},"why":"no clip is visible at this time; check where clips start and end"})); } else { steps.push(json!({"tool":"lint_frame","args":{"time":t,"annotate":false},"why":format!("{total} visible clip{}; check bounds, visibility, text size and safe zones at this time", if total == 1 { "" } else { "s" })})); }
            if mean < 0.004 && nb as f64 / np > 0.999 { steps.push(json!({"tool":"get_project","args":{},"why":format!("the frame is black (mean luma {mean:.3}); check opacity and clip timing")})); }
            steps.truncate(3); out["next_steps"] = json!(steps);
            Ok(out) })) },
] }
