//! track_planar_anchored: keyframe-anchored planar tracking. The caller (or an AI that checked frames) supplies verified quads at
//! several frames. Each gap is tracked forward from its left anchor and backward from its right anchor (each with its own template),
//! and the two tracks are blended with a smoothstep weight so the result passes exactly through every anchor. Drift can no longer grow
//! past one gap, and the forward/backward closure error per gap is reported, so a gap that needs another anchor is visible.
//! Limit: anchors are trusted as given. This does not decide which window of a repeated facade is the right one; the anchors do.
use super::kit::*;
use crate::keyframe::{Ease, Prop};
use crate::planar;

type Q = [[f64; 2]; 4];
fn qarg(v: &Value, w: f64, h: f64) -> Result<Q, String> {
    let c: Vec<f64> = v.as_array().ok_or("corners must be an array")?.iter().filter_map(|x| x.as_f64()).collect();
    if c.len() != 8 || c.iter().any(|x| !x.is_finite()) { return Err("corners needs 8 finite numbers".into()); }
    Ok([[c[0] * w, c[1] * h], [c[2] * w, c[3] * h], [c[4] * w, c[5] * h], [c[6] * w, c[7] * h]])
}
fn cdist(a: &Q, b: &Q) -> f64 { (0..4).map(|k| (a[k][0] - b[k][0]).hypot(a[k][1] - b[k][1])).fold(0.0, f64::max) }
fn smooth(x: f64) -> f64 { let x = x.clamp(0.0, 1.0); x * x * (3.0 - 2.0 * x) }

pub fn commands() -> Vec<Command> { vec![
    Command { name: "track_planar_anchored", description: "Planar track pinned by verified keyframe quads. anchors = [{frame, corners(8 normalized numbers TL TR BR BL)}] (at least 2, any order). Each gap between neighbouring anchors is tracked forward from the left anchor and backward from the right anchor and blended, so the result passes exactly through every anchor and drift cannot exceed one gap. Reports per-gap closure (how far each one-sided track ends from the next anchor, in px) and flags gaps over threshold_px: put another anchor in the middle of those. Fixes the repeated-window failure only as far as the anchors are right; it does not pick the correct window itself. Optionally pins target_clip_id like track_planar (expand supported). return_quads=true returns every frame's quad.", mutates: true,
        schema: obj(vec![("clip_id", s("media clip")), ("anchors", json!({"type":"array","items":{"type":"object"},"description":"[{frame, corners:[8 normalized]}], >=2"})), ("target_clip_id", s("layer to pin")), ("expand", n("pin margin, as track_planar")), ("levels", n("2-5, default 3")), ("threshold_px", n("closure flag threshold, default 3")), ("return_quads", json!({"type":"boolean"}))], &["clip_id", "anchors"]),
        run: |ss, a| { let cid = str_arg(a, "clip_id")?.to_string(); let (w, h) = (ss.project.width as f64, ss.project.height as f64);
            let mc = ss.project.find_clip(&cid).ok_or("no such clip")?.clone(); let nf = ((mc.duration * ss.project.fps).ceil() as usize).max(1);
            let mut an: Vec<(usize, Q)> = vec![]; for v in a.get("anchors").and_then(|x| x.as_array()).ok_or("anchors needed")? {
                let f = v.get("frame").and_then(|x| x.as_f64()).ok_or("anchor needs frame")?; if f < 0.0 || f as usize >= nf { return Err(format!("anchor frame {f} outside clip (0..{})", nf - 1)); }
                an.push((f as usize, qarg(v.get("corners").ok_or("anchor needs corners")?, w, h)?)); }
            if an.len() < 2 { return Err("need at least 2 anchors".into()); } an.sort_by_key(|x| x.0); if an.windows(2).any(|p| p[0].0 == p[1].0) { return Err("duplicate anchor frames".into()); }
            let levels = opt_num(a, "levels", 3.0) as usize; let thr = opt_num(a, "threshold_px", 3.0); if !thr.is_finite() || thr <= 0.0 { return Err("threshold_px must be > 0".into()); }
            let t0 = std::time::Instant::now(); let f0 = an[0].0; let f1 = an.last().unwrap().0; let mut quads: Vec<Q> = vec![an[0].1; f1 - f0 + 1]; let mut conf = vec![1.0f64; f1 - f0 + 1]; let mut gaps = vec![];
            for p in an.windows(2) { let ((fa, qa), (fb, qb)) = (p[0], p[1]);
                let fw = planar::track_opts(&ss.project, &cid, qa, fa, fb, levels, false)?; let frames: Vec<usize> = (fa..=fb).rev().collect(); let bw = planar::track_frames(&ss.project, &cid, qb, &frames, levels, false)?;
                let n = fb - fa; let (fe, be) = (cdist(&fw.quads[n], &qb), cdist(&bw.quads[n], &qa)); let mut mid = 0f64; let mut minc = 1.0f64;
                for i in 0..=n { let fq = &fw.quads[i]; let bq = &bw.quads[n - i]; let t = smooth(i as f64 / n as f64); let mut o = *fq; for k in 0..4 { for x in 0..2 { o[k][x] = (1.0 - t) * fq[k][x] + t * bq[k][x]; } }
                    mid = mid.max(cdist(fq, bq)); quads[fa - f0 + i] = o; let c = (1.0 - t) * fw.conf[i] + t * bw.conf[n - i]; conf[fa - f0 + i] = if i == 0 || i == n { 1.0 } else { c }; if i > 0 && i < n { minc = minc.min(c); } }
                gaps.push(json!({"from": fa, "to": fb, "forward_closure_px": fe, "backward_closure_px": be, "max_fwd_bwd_disagreement_px": mid, "min_blended_conf": minc, "needs_anchor": fe > thr || be > thr, "suggest_anchor_frame": (fa + fb) / 2})); }
            if let Some(tid) = a.get("target_clip_id").and_then(|v| v.as_str()) { let fps = ss.project.fps; let ts = ss.project.find_clip(tid).ok_or("no target clip")?.start;
                let tc = ss.project.find_clip_mut(tid).ok_or("no target clip")?; tc.props.insert("pin_on".into(), Prop::new(1.0)); let ex = opt_num(a, "expand", 0.0).clamp(0.0, 2.0);
                let pq: Vec<Q> = quads.iter().map(|qd| if ex <= 0.0 { *qd } else { let m = crate::shards::sq2quad([(qd[0][0], qd[0][1]), (qd[1][0], qd[1][1]), (qd[2][0], qd[2][1]), (qd[3][0], qd[3][1])]); let e = |u: f64, v: f64| { let (x, y) = crate::shards::hmap(&m, u, v); [x, y] }; [e(-ex, -ex), e(1.0 + ex, -ex), e(1.0 + ex, 1.0 + ex), e(-ex, 1.0 + ex)] }).collect();
                for k in 0..4 { for (ax, nm) in [(0usize, "x"), (1, "y")] { let mut pr = Prop::new(pq[0][k][ax]); for (i, qd) in pq.iter().enumerate() { pr.set_key(mc.start + (f0 + i) as f64 / fps - ts, qd[k][ax], Ease::Linear); } tc.props.insert(format!("pin{k}{nm}"), pr); } } }
            let flagged = gaps.iter().filter(|g| g["needs_anchor"] == json!(true)).count();
            let mut res = json!({"frames": quads.len(), "first_frame": f0, "last_frame": f1, "seconds": t0.elapsed().as_secs_f64(), "gaps": gaps, "gaps_needing_anchor": flagged, "note": "Passes exactly through the anchors. Closure measures one-sided drift over a gap; it cannot detect a wrong anchor or a shared lock on an identical neighbour."});
            if a.get("return_quads").and_then(|v| v.as_bool()).unwrap_or(false) { res["quads"] = json!(quads.iter().map(|q| q.iter().flat_map(|p| vec![p[0], p[1]]).collect::<Vec<f64>>()).collect::<Vec<_>>()); res["conf"] = json!(conf); }
            Ok(res) } },
] }
