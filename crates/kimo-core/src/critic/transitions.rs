//! Transitions critic: transitions that dip to black, are too short to read as a transition, or are longer than the
//! clip they belong to.
use super::{editing::is_picture, frame, r3, Finding, Severity};
use crate::model::Project;
use serde_json::json;

pub fn check(p: &Project) -> Vec<Finding> {
    let mut out = vec![];
    let fr = frame(p);
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
}
