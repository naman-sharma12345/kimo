//! validate_planar_track: independent-anchor consistency check for the planar tracker.
//! A confident fit is not proof of pixel lock (repeated facade windows lock confidently to the wrong window). This tool re-tracks the
//! same plane from several anchor frames, each with its own template built from the quad the base track claims at that frame, and
//! reports where the independent tracks disagree with the base. It does NOT certify a track: agreement can still be a shared wrong lock
//! (an identical window one pitch over is seen by every anchor alike). Disagreement is proof that the base track is not reliable there.
use super::kit::*;
use crate::planar;

fn quantile(v: &mut Vec<f64>, q: f64) -> f64 { if v.is_empty() { return 0.0; } v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[((v.len() - 1) as f64 * q).round() as usize] }
fn quad_arg(a: &Value, k: &str, w: f64, h: f64) -> Result<Option<[[f64; 2]; 4]>, String> {
    let Some(v) = a.get(k) else { return Ok(None) }; let c: Vec<f64> = v.as_array().ok_or(format!("{k} must be an array"))?.iter().filter_map(|x| x.as_f64()).collect();
    if c.len() != 8 || c.iter().any(|x| !x.is_finite()) { return Err(format!("{k} needs 8 finite numbers")); } Ok(Some([[c[0] * w, c[1] * h], [c[2] * w, c[3] * h], [c[4] * w, c[5] * h], [c[6] * w, c[7] * h]]))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "validate_planar_track", description: "Check a planar track without changing the project: tracks the plane (same corners/context_corners/start/end/levels as track_planar), then re-tracks it forward from `anchors` independent frames and backward from the last frame and the same anchors (each builds its own template from the base track's quad at that frame) and reports per-frame max corner disagreement in px against the base track. Returns p50/p95/max, the first frame over `threshold_px` (default 3), the flagged frame list and per-frame per-corner arrays (export these as residual diagnostics). High track_planar confidence with large disagreement means the track is NOT trustworthy there. Limits: agreement does not prove lock (all anchors can share the same wrong-but-identical window), so treat a pass as 'no evidence of drift', not certification.", mutates: false,
        schema: obj(vec![("clip_id", s("media clip")), ("corners", json!({"type":"array","items":{"type":"number"},"description":"8 normalized numbers TL TR BR BL"})), ("context_corners", json!({"type":"array","items":{"type":"number"}})), ("start", n("first frame")), ("end", n("last frame")), ("levels", n("2-5")), ("anchors", n("independent anchor frames, 1-8, default 3, evenly spaced")), ("threshold_px", n("flag threshold, default 3")), ("backward", json!({"type":"boolean","description":"also track backward from the last frame and each anchor and report it separately as backward_max_target_corner_px (about 6x slower; backward tracks start from the base track's own quad so they inherit its error)"}))], &["clip_id", "corners"]),
        run: |ss, a| { let cid = str_arg(a, "clip_id")?.to_string(); let (w, h) = (ss.project.width as f64, ss.project.height as f64);
            let q = quad_arg(a, "corners", w, h)?.ok_or("corners needed")?; let ctx = quad_arg(a, "context_corners", w, h)?;
            let mc = ss.project.find_clip(&cid).ok_or("no such clip")?.clone(); let nf = ((mc.duration * ss.project.fps).ceil() as usize).max(1);
            let f0 = opt_num(a, "start", 0.0).max(0.0) as usize; let f1 = (opt_num(a, "end", (nf - 1) as f64).max(0.0) as usize).min(nf - 1); if f1 <= f0 + 2 { return Err("need at least 3 frames".into()); }
            let levels = opt_num(a, "levels", 3.0) as usize; let na = (opt_num(a, "anchors", 3.0) as usize).clamp(1, 8); let thr = opt_num(a, "threshold_px", 3.0); if !thr.is_finite() || thr <= 0.0 { return Err("threshold_px must be > 0".into()); }
            let t0 = std::time::Instant::now(); let tracked_q = ctx.unwrap_or(q); let base = planar::track_opts(&ss.project, &cid, tracked_q, f0, f1, levels, false)?;
            // distances are measured on the TARGET quad (corners): when a context quad is tracked, the target is propagated through the context homography
            let tgt = |from: &[[f64; 2]; 4], to: &[[f64; 2]; 4], tq: &[[f64; 2]; 4]| -> [[f64; 2]; 4] { if ctx.is_some() { planar::propagate_quad(from, to, tq).unwrap_or(*to) } else { *to } };
            let n = base.quads.len(); let base_t: Vec<[[f64; 2]; 4]> = base.quads.iter().map(|b| tgt(&tracked_q, b, &q)).collect();
            let mut dis = vec![0f64; n]; let mut per_corner = vec![[0f64; 4]; n]; let mut ctx_dis = vec![0f64; n]; let mut used = vec![]; let mut counts = vec![0usize; n];
            // backward tracks: from the last frame and from each anchor, with templates built there, run toward the start
            let mut bwd_used = vec![]; let do_bwd = a.get("backward").and_then(|x| x.as_bool()).unwrap_or(false); let mut bwd_dis = vec![0f64; n];
            for k in 0..=(if do_bwd { na } else { 0 }) { if !do_bwd { break; } let af = if k == 0 { f1 } else { f0 + (f1 - f0) * k / (na + 1) }; if af <= f0 + 1 { continue; } let i0 = af - f0; bwd_used.push(af);
                let frames: Vec<usize> = (f0..=af).rev().collect(); let tr = planar::track_frames(&ss.project, &cid, base.quads[i0], &frames, levels, false)?;
                for (j, qd) in tr.quads.iter().enumerate() { if j == 0 { continue; } let i = i0 - j; let at = tgt(&base.quads[i0], qd, &base_t[i0]); for c in 0..4 { let d = (at[c][0] - base_t[i][c][0]).hypot(at[c][1] - base_t[i][c][1]); if d > bwd_dis[i] { bwd_dis[i] = d; } } } }
            for k in 1..=na { let af = f0 + (f1 - f0) * k / (na + 1); if af <= f0 || af >= f1 { continue; } let i0 = af - f0; used.push(af);
                let tr = planar::track_opts(&ss.project, &cid, base.quads[i0], af, f1, levels, false)?;
                for (j, qd) in tr.quads.iter().enumerate() { let i = i0 + j; if i >= n || j == 0 { continue; } counts[i] += 1;
                    let at = tgt(&base.quads[i0], qd, &base_t[i0]);
                    for c in 0..4 { let d = (at[c][0] - base_t[i][c][0]).hypot(at[c][1] - base_t[i][c][1]); if d > per_corner[i][c] { per_corner[i][c] = d; } let dc = (qd[c][0] - base.quads[i][c][0]).hypot(qd[c][1] - base.quads[i][c][1]); if dc > ctx_dis[i] { ctx_dis[i] = dc; } } } }
            for i in 0..n { dis[i] = per_corner[i].iter().cloned().fold(0.0, f64::max); }
            let checked: Vec<f64> = (0..n).filter(|i| counts[*i] > 0).map(|i| dis[i]).collect(); let mut v = checked.clone();
            let flagged: Vec<usize> = (0..n).filter(|i| counts[*i] > 0 && dis[*i] > thr).map(|i| f0 + i).collect();
            let minc = base.conf.iter().cloned().fold(1.0, f64::min); let meanc = base.conf.iter().sum::<f64>() / n as f64;
            Ok(json!({"frames": n, "anchors_used": used, "backward_anchors_used": bwd_used, "seconds": t0.elapsed().as_secs_f64(), "measured_on": if ctx.is_some() { "target quad (propagated from the tracked context quad by homography)" } else { "tracked quad (= target)" }, "tested_frames": checked.len(), "untested_frames": n - checked.len(), "untested_note": "frames with anchors_compared = 0 (before the first anchor) were NOT tested; their zero distance is not a pass", "base_mean_conf": meanc, "base_min_conf": minc,
                "disagreement_px": {"p50": quantile(&mut v.clone(), 0.5), "p95": quantile(&mut v, 0.95), "max": checked.iter().cloned().fold(0.0, f64::max), "checked_frames": checked.len()},
                "threshold_px": thr, "first_flagged_frame": flagged.first(), "flagged_frames": flagged.len(), "flagged_list": flagged.iter().take(200).collect::<Vec<_>>(),
                "per_frame": (0..n).map(|i| json!({"frame": f0 + i, "conf": base.conf[i], "max_target_corner_px": if counts[i] > 0 { json!(dis[i]) } else { Value::Null }, "tested": counts[i] > 0, "target_corners_px": per_corner[i], "context_max_px": ctx_dis[i], "backward_max_target_corner_px": if do_bwd { json!(bwd_dis[i]) } else { Value::Null }, "anchors_compared": counts[i]})).collect::<Vec<_>>(),
                "note": "Disagreement proves unreliability; agreement does not prove pixel lock (shared wrong-but-identical lock is invisible here)."})) } },
] }
