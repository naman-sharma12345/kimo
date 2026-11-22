//! Same-track transitions. A transition composites the incoming clip over whatever lies under it during its first
//! `duration` seconds, so two clips cut back to back on ONE track had nothing to transition from (the outgoing clip has
//! already ended): every edit needed the incoming clip lifted to a track above by hand and the outgoing clip's tail
//! extended. `prepare` does that inside add_transition:
//! - the incoming clip moves to a track directly above (an existing one free over the span, else a new one);
//! - the overlap comes from the OUTGOING clip's tail handle (it plays `duration` longer, under the incoming one), so the
//!   cut point and the incoming clip's timing stay where the editor put them;
//! - with no tail handle, from the INCOMING clip's head handle (it starts `duration` earlier);
//! - with neither, it says so (freeze-frame the outgoing clip's last frame, or pick a shorter transition).
use super::kit::*;
use crate::model::Source;

/// Seconds of source the clip can still extend by at its tail / head (None = unlimited: not a media clip, or a still).
fn handles(ss: &Session, c: &crate::model::Clip) -> (Option<f64>, Option<f64>) {
    let Source::Media { asset } = &c.source else { return (None, None) };
    let Some(a) = ss.project.assets.get(asset) else { return (Some(0.0), Some(0.0)) };
    if a.duration <= 0.0 || a.fps <= 0.0 && a.width > 0 && a.duration <= 0.0 { return (None, None); }
    if c.retimed() { return (Some(0.0), Some(0.0)); }
    let sp = c.speed.abs().max(1e-6);
    (Some(((a.duration - (c.in_point + c.duration * sp)) / sp).max(0.0)), Some((c.in_point / sp).max(0.0)))
}

/// Makes room for a transition into `clip_id` when the clip it follows is on the same track. Returns what it changed
/// (None when there is nothing to do: no adjacent clip before it on its track, or something already lies under it).
pub fn prepare(ss: &mut Session, clip_id: &str, dur: f64) -> Result<Option<Value>, String> {
    let ti = ss.project.tracks.iter().position(|t| t.clips.iter().any(|c| c.id == clip_id)).ok_or_else(|| format!("no such clip '{clip_id}'"))?;
    let b = ss.project.tracks[ti].clips.iter().find(|c| c.id == clip_id).unwrap().as_ref().clone();
    let fdur = 1.0 / ss.project.fps.max(1.0);
    let Some(a) = ss.project.tracks[ti].clips.iter().filter(|c| c.id != b.id && (c.start + c.duration - b.start).abs() <= fdur * 0.6).max_by(|x, y| x.start.partial_cmp(&y.start).unwrap_or(std::cmp::Ordering::Equal)).map(|c| c.as_ref().clone()) else { return Ok(None) };
    // something already under the incoming clip at its start (a manual A/B layout): leave it alone
    if ss.project.tracks[..ti].iter().any(|t| t.clips.iter().any(|c| c.start < b.start + 1e-6 && c.start + c.duration > b.start + fdur)) { return Ok(None); }
    let (a_tail, _) = handles(ss, &a); let (_, b_head) = handles(ss, &b);
    let (extend_a, mut start_b) = if a_tail.map_or(true, |h| h + 1e-6 >= dur) { (true, b.start) } else if b_head.map_or(true, |h| h + 1e-6 >= dur) { (false, b.start - dur) } else {
        // nothing to overlap with: leave the clips as they are (the transition plays over what is underneath, as before) and say why
        return Ok(Some(json!({"warning": format!("no overlap made for the {dur:.2} s transition between {} and {}: {} has {:.2} s of footage after its end and {} has {:.2} s before its start, so it plays over what is underneath. Freeze the outgoing clip's last frame (set_speed hold) or use a transition of at most {:.2} s", a.id, b.id, a.id, a_tail.unwrap_or(0.0), b.id, b_head.unwrap_or(0.0), a_tail.unwrap_or(0.0).max(b_head.unwrap_or(0.0)))})));
    };
    if start_b < 0.0 { start_b = 0.0; }
    let span = (start_b.min(b.start), b.start + b.duration);
    // the track above: the next one up if it is free over the incoming clip's span, else a new track inserted there
    let free = |t: &crate::model::Track| t.clips.iter().all(|c| c.start + c.duration <= span.0 + 1e-6 || c.start >= span.1 - 1e-6) && !t.locked;
    let up = if ti + 1 < ss.project.tracks.len() && free(&ss.project.tracks[ti + 1]) { ss.project.tracks[ti + 1].id.clone() } else {
        let id = ss.project.gen_id("track"); ss.project.tracks.insert(ti + 1, crate::model::Track { id: id.clone(), clips: vec![], locked: false, muted: false, solo: false, name: String::new() }); id };
    // move B up (keeping the same Arc'd clip, edited), then extend A's tail or B's head
    let ti_b = ss.project.tracks.iter().position(|t| t.clips.iter().any(|c| c.id == clip_id)).unwrap();
    let pos = ss.project.tracks[ti_b].clips.iter().position(|c| c.id == clip_id).unwrap();
    let mut moved = ss.project.tracks[ti_b].clips.remove(pos);
    if !extend_a { let c = std::sync::Arc::make_mut(&mut moved); let sp = c.speed.abs().max(1e-6); c.in_point = (c.in_point - dur * sp).max(0.0); c.start = start_b; c.duration += b.start - start_b; }
    let ui = ss.project.tracks.iter().position(|t| t.id == up).unwrap();
    ss.project.tracks[ui].clips.push(moved); ss.project.tracks[ui].clips.sort_by(|x, y| x.start.partial_cmp(&y.start).unwrap_or(std::cmp::Ordering::Equal));
    if extend_a { if let Some(c) = ss.project.find_clip_mut(&a.id) { c.duration += dur; } }
    Ok(Some(json!({"lifted_to_track": up, "overlap_from": if extend_a { format!("{}'s tail (+{dur:.2} s)", a.id) } else { format!("{}'s head (starts {dur:.2} s earlier)", b.id) }, "outgoing": a.id})))
}

pub fn commands() -> Vec<Command> { vec![] }
