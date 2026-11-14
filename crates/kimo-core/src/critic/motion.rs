//! Animation critic: keyframed motion that looks wrong. Jitter (the move keeps reversing), a one-frame pop (a jump
//! far larger than the frames around it), long linear moves (mechanical, no ease), and an element that leaves the frame
//! in the middle of its move.
use super::{frame, r3, Finding, Severity};
use crate::model::Project;
use serde_json::json;

const MOVING: [&str; 4] = ["x", "y", "scale", "rotation"];

pub fn check(p: &Project) -> Vec<Finding> {
    let mut out = vec![];
    let fr = frame(p);
    let (hw, hh) = (p.width as f64 / 2.0, p.height as f64 / 2.0);
    for tr in &p.tracks { for c in &tr.clips {
        for name in MOVING {
            let Some(prop) = c.props.get(name) else { continue };
            if prop.keys.len() < 2 { continue; }
            let (k0, k1) = (prop.keys.first().unwrap().t.max(0.0), prop.keys.last().unwrap().t.min(c.duration));
            if k1 - k0 < 2.0 * fr { continue; }
            // sample the curve at the frame rate across its keyed span
            let n = (((k1 - k0) / fr).ceil() as usize).min(3000);
            let vals: Vec<f64> = (0..=n).map(|i| prop.at(k0 + (k1 - k0) * i as f64 / n as f64)).collect();
            let d: Vec<f64> = vals.windows(2).map(|w| w[1] - w[0]).collect();
            let span = vals.iter().cloned().fold(f64::MIN, f64::max) - vals.iter().cloned().fold(f64::MAX, f64::min);
            let unit = match name { "scale" => 0.01, "rotation" => 0.5, _ => 1.0 };
            if span < unit * 2.0 { continue; }
            // jitter: direction reversals with real amplitude, more than ~2 per second
            let rev = d.windows(2).filter(|w| w[0].abs() > unit * 0.3 && w[1].abs() > unit * 0.3 && w[0].signum() != w[1].signum()).count();
            let per_s = rev as f64 / (k1 - k0);
            if rev >= 3 && per_s > 2.0 {
                out.push(Finding::new("jittery_motion", Severity::Warning, "motion", format!("{} {} reverses direction {} times in {:.1} s: it shakes instead of moving", c.id, name, rev, k1 - k0))
                    .clip(&c.id).range(c.start + k0, c.start + k1).measured(json!({"reversals_per_s": r3(per_s)})).threshold(json!({"max_reversals_per_s": 2.0})));
            }
            // pop: one frame's change much larger than its neighbours (a key landing on a wrong value, a missing ease)
            let typical = { let mut a: Vec<f64> = d.iter().map(|x| x.abs()).collect(); a.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)); a[a.len() / 2] };
            if let Some((i, big)) = d.iter().map(|x| x.abs()).enumerate().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)) {
                if big > unit * 4.0 && big > typical.max(unit * 0.25) * 8.0 && big > span * 0.35 {
                    let t = c.start + k0 + (k1 - k0) * i as f64 / n as f64;
                    out.push(Finding::new("motion_pop", Severity::Warning, "motion", format!("{} {} jumps by {:.2} in one frame at {:.2} s (the frames around it move {:.2}): a visible pop", c.id, name, big, t, typical))
                        .clip(&c.id).at(t).measured(json!({"jump": r3(big), "typical": r3(typical)})));
                }
            }
            // long linear segments on position/scale: mechanical motion
            for w in prop.keys.windows(2) {
                let lin = serde_json::to_value(&w[0].ease).ok().and_then(|v| v.as_str().map(|s| s == "linear")).unwrap_or(false);
                if lin && w[0].bez.is_none() && (w[1].t - w[0].t) > 0.3 && (w[1].v - w[0].v).abs() > unit * 10.0 && name != "rotation" {
                    out.push(Finding::new("linear_motion", Severity::Info, "motion", format!("{} {} moves linearly for {:.2} s: it starts and stops abruptly. An ease (in_out) looks natural", c.id, name, w[1].t - w[0].t))
                        .clip(&c.id).range(c.start + w[0].t, c.start + w[1].t)
                        .fix("set_keyframe", json!({"clip_id": c.id, "property": name, "time": r3(w[0].t), "value": w[0].v, "ease": "in_out"})));
                }
            }
            // leaves the frame mid-move while both ends are inside
            if name == "x" || name == "y" {
                let half = if name == "x" { hw } else { hh };
                let (a, b) = (vals[0].abs() < half, vals[vals.len() - 1].abs() < half);
                if a && b { if let Some(i) = vals.iter().position(|v| v.abs() > half) {
                    let t = c.start + k0 + (k1 - k0) * i as f64 / n as f64;
                    out.push(Finding::new("leaves_frame_mid_move", Severity::Info, "motion", format!("{} goes off-screen at {:.2} s in the middle of its {} move and comes back", c.id, t, name))
                        .clip(&c.id).at(t));
                } }
            }
        }
    } }
    out
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;
    fn key(ss: &mut crate::commands::Session, id: &str, prop: &str, t: f64, v: f64, ease: &str) {
        call(ss, "set_keyframe", json!({"clip_id": id, "property": prop, "time": t, "value": v, "ease": ease}));
    }
    #[test]
    fn jitter_and_linear_moves_are_found() {
        let mut ss = session();
        let t = track(&mut ss);
        let a = solid(&mut ss, &t, 0.0, 4.0);
        for i in 0..12 { key(&mut ss, &a, "x", i as f64 * 0.1, if i % 2 == 0 { 0.0 } else { 40.0 }, "linear"); }
        let b = solid(&mut ss, &t, 4.0, 3.0);
        key(&mut ss, &b, "x", 0.0, -600.0, "linear"); key(&mut ss, &b, "x", 1.5, 600.0, "linear");
        let f = check(&ss.project);
        assert!(f.iter().any(|x| x.code == "jittery_motion" && x.clip_id.as_deref() == Some(a.as_str())), "{:?}", codes(&f));
        assert!(f.iter().any(|x| x.code == "linear_motion" && x.clip_id.as_deref() == Some(b.as_str())), "{:?}", codes(&f));
    }
    #[test]
    fn an_eased_move_is_clean() {
        let mut ss = session();
        let t = track(&mut ss);
        let a = solid(&mut ss, &t, 0.0, 3.0);
        key(&mut ss, &a, "x", 0.0, -400.0, "in_out"); key(&mut ss, &a, "x", 1.0, 0.0, "in_out");
        assert!(check(&ss.project).is_empty(), "{:?}", codes(&check(&ss.project)));
    }
}
