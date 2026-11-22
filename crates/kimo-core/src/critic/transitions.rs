//! Transitions critic: transitions that dip to black, are too short to read as a transition, are longer than the
//! clip they belong to, are out of step with the edit's rhythm, or move the picture without motion blur.
use super::{editing::is_picture, frame, r3, Finding, Severity};
use crate::model::Project;
use serde_json::json;

/// Transitions that throw the picture across the frame: without motion blur they strobe.
pub fn is_moving(kind: &str) -> bool { ["slide", "push", "zoom", "spin", "whip"].iter().any(|k| kind.starts_with(k)) }

pub fn check(p: &Project) -> Vec<Finding> {
    let mut out = vec![];
    let fr = frame(p);
    // the edit's rhythm: median length of the picture shots
    let mut lens: Vec<f64> = p.tracks.iter().flat_map(|t| t.clips.iter()).filter(|c| is_picture(p, c) && c.duration > 0.0).map(|c| c.duration).collect();
    lens.sort_by(f64::total_cmp);
    let median = if lens.is_empty() { 0.0 } else { lens[lens.len() / 2] };
    for tr in &p.tracks { for c in &tr.clips {
        let Some(tx) = &c.transition_in else { continue };
        if tx.duration <= 0.0 || !is_picture(p, c) { continue; }
        if tx.duration < 4.0 * fr - 1e-9 {
            out.push(Finding::new("transition_too_short", Severity::Warning, "transitions", format!("the {} into {} lasts {:.0} frame(s): it reads as a glitch, not a transition", tx.kind, c.id, (tx.duration / fr).round()))
                .clip(&c.id).at(c.start).measured(json!({"frames": r3(tx.duration / fr)})).threshold(json!({"min_frames": 4}))
                .fix("add_transition", json!({"clip_id": c.id, "kind": tx.kind, "duration": r3((8.0 * fr).max(0.25))})));
        }
        if tx.duration > c.duration + 1e-9 {
            out.push(Finding::new("transition_longer_than_clip", Severity::Warning, "transitions", format!("the {} into {} ({:.2} s) is longer than the clip itself ({:.2} s)", tx.kind, c.id, tx.duration, c.duration))
                .clip(&c.id).at(c.start).fix("add_transition", json!({"clip_id": c.id, "kind": tx.kind, "duration": r3(c.duration * 0.5)})));
        }
        // a transition needs a picture under it to transition FROM: otherwise the export dips to black
        let mid = c.start + tx.duration.min(c.duration) * 0.5;
        let under = p.tracks.iter().flat_map(|t| t.clips.iter()).any(|o| o.id != c.id && is_picture(p, o) && o.start <= mid && mid < o.start + o.duration);
        // rhythm: a transition that eats most of a shot, or drags against a fast-cut edit
        let from = p.tracks.iter().flat_map(|t| t.clips.iter()).filter(|o| o.id != c.id && is_picture(p, o) && o.start <= mid && mid < o.start + o.duration).map(|o| o.duration).fold(f64::MAX, f64::min);
        let shorter = c.duration.min(if from == f64::MAX { c.duration } else { from });
        if tx.duration <= c.duration + 1e-9 && tx.duration >= 4.0 * fr - 1e-9 {
            let ideal = r3((0.4 * shorter).min(if median > 0.0 { 0.25 * median } else { f64::MAX }).max(8.0 * fr).min(0.7_f64.max(8.0 * fr)));
            if tx.duration > 0.4 * shorter + 1e-9 {
                out.push(Finding::new("transition_rhythm", Severity::Warning, "transitions", format!("the {} into {} lasts {:.2} s, {:.0}% of the shorter shot it joins ({:.2} s): the shots never get to play clean; keep a transition under 40% of its shorter shot", tx.kind, c.id, tx.duration, tx.duration / shorter * 100.0, shorter))
                    .clip(&c.id).range(c.start, c.start + tx.duration).measured(json!({"duration": r3(tx.duration), "shorter_shot": r3(shorter), "share": r3(tx.duration / shorter)})).threshold(json!({"max_share": 0.4}))
                    .fix("add_transition", json!({"clip_id": c.id, "kind": tx.kind, "duration": ideal})));
            } else if median > 0.0 && tx.duration > 0.5 && tx.duration > 0.35 * median {
                out.push(Finding::new("transition_rhythm", Severity::Info, "transitions", format!("the {} into {} lasts {:.2} s while the edit cuts every {:.2} s (median shot): it drags against the pace; most cuts should be straight cuts, dissolves 0.3-0.7 s", tx.kind, c.id, tx.duration, median))
                    .clip(&c.id).range(c.start, c.start + tx.duration).measured(json!({"duration": r3(tx.duration), "median_shot": r3(median)})).threshold(json!({"max_share_of_median": 0.35}))
                    .fix("add_transition", json!({"clip_id": c.id, "kind": tx.kind, "duration": ideal})));
            }
        }
        if is_moving(&tx.kind) && c.motion_blur <= 0.0 {
            out.push(Finding::new("moving_transition_without_blur", Severity::Info, "transitions", format!("the {} into {} moves the picture fast with motion blur off: it strobes; a 180 degree shutter makes it read as motion", tx.kind, c.id))
                .clip(&c.id).range(c.start, c.start + tx.duration).measured(json!({"motion_blur": c.motion_blur}))
                .fix("set_motion_blur", json!({"clip_id": c.id, "shutter_angle": 180})));
        }
        if !under && !tx.kind.starts_with("dip_") && tx.kind != "flash" {
            out.push(Finding::new("transition_from_black", Severity::Warning, "transitions", format!("the {} into {} at {:.2} s has nothing under it, so the picture dips to black", tx.kind, c.id, c.start))
                .clip(&c.id).range(c.start, c.start + tx.duration)
                .fix("remove_transition", json!({"clip_id": c.id})));
        }
    } }
    out
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;
    #[test]
    fn short_and_from_black_transitions() {
        let mut ss = session();
        let t = track(&mut ss);
        solid(&mut ss, &t, 0.0, 2.0);
        let b = solid(&mut ss, &t, 2.0, 2.0);
        call(&mut ss, "add_transition", json!({"clip_id": b, "kind": "crossfade", "duration": 2.0 / 30.0}));
        let f = check(&ss.project);
        assert!(codes(&f).contains(&"transition_too_short"), "{:?}", codes(&f));
        assert!(codes(&f).contains(&"transition_from_black"), "touching clips: nothing under the dissolve: {:?}", codes(&f));
    }
    #[test]
    fn overlapped_dissolve_is_clean() {
        let mut ss = session();
        let t1 = track(&mut ss); let t2 = track(&mut ss);
        solid(&mut ss, &t1, 0.0, 3.0);
        let b = solid(&mut ss, &t2, 2.5, 2.0);
        call(&mut ss, "add_transition", json!({"clip_id": b, "kind": "crossfade", "duration": 0.5}));
        assert!(check(&ss.project).is_empty(), "{:?}", codes(&check(&ss.project)));
    }
    #[test]
    fn long_and_unblurred_moving_transitions() {
        let mut ss = session();
        let t1 = track(&mut ss); let t2 = track(&mut ss);
        solid(&mut ss, &t1, 0.0, 3.0);
        let b = solid(&mut ss, &t2, 1.5, 2.0);
        call(&mut ss, "add_transition", json!({"clip_id": b, "kind": "slide_left", "duration": 1.4}));
        let f = check(&ss.project);
        assert!(codes(&f).contains(&"transition_rhythm"), "1.4 s of a 2 s shot: {:?}", codes(&f));
        assert!(codes(&f).contains(&"moving_transition_without_blur"), "{:?}", codes(&f));
        let r = f.iter().find(|x| x.code == "transition_rhythm").unwrap();
        assert!(r.fix["args"]["duration"].as_f64().unwrap() <= 0.8, "{}", r.fix);
        call(&mut ss, "set_motion_blur", json!({"clip_id": b, "shutter_angle": 180}));
        call(&mut ss, "add_transition", json!({"clip_id": b, "kind": "slide_left", "duration": 0.5}));
        assert!(check(&ss.project).is_empty(), "{:?}", codes(&check(&ss.project)));
    }
}
