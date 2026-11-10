//! add_push_wipe: Premiere's Push and After Effects' Gradient Wipe as transitions INTO a clip (pushwipe.rs renders them).
//! A new tool, so add_transition's schema and kinds stay unchanged.
use super::kit::*;
use crate::model::*;

const ARGS: &[&str] = &["clip_id", "kind", "duration", "ease", "direction", "gradient", "angle", "center", "luma_clip_id", "softness", "invert", "remove"];
const WIPE_ONLY: &[&str] = &["gradient", "angle", "center", "luma_clip_id", "softness", "invert"];

pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_push_wipe", description: "Push (B slides in, A slides out) or Gradient Wipe (reveal by a linear/radial/angular gradient or a clip's luma) into a clip.", mutates: true,
        schema: obj(vec![("clip_id", s("the incoming clip (B). The outgoing picture is the clip ending where B starts on its track, else what is under B")),
            ("kind", json!({"type":"string","enum":crate::pushwipe::KINDS})),
            ("duration", n("seconds from B's start, 0.05..10 and at most B's length, default 0.5")),
            ("ease", json!({"type":"string","enum":crate::transitions::EASES,"description":"progress easing, default in_out_cubic"})),
            ("direction", json!({"type":"string","enum":crate::pushwipe::DIRECTIONS,"description":"push: the way both pictures travel, default left (B enters from the right)"})),
            ("gradient", json!({"type":"string","enum":crate::pushwipe::GRADIENTS,"description":"gradient_wipe ramp, default linear (ignored with luma_clip_id)"})),
            ("angle", n("gradient_wipe: linear direction / angular start, degrees (0 = left to right, 90 = top to bottom)")),
            ("center", json!({"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2,"description":"gradient_wipe radial/angular centre [x, y] in 0..1 of the frame, default [0.5, 0.5]"})),
            ("luma_clip_id", s("gradient_wipe: a clip whose luma drives the reveal (dark first); it is hidden from the composite")),
            ("softness", n("gradient_wipe edge softness 0..1, default 0.1")),
            ("invert", json!({"type":"boolean","description":"gradient_wipe: reveal bright values first"})),
            ("remove", json!({"type":"boolean","description":"true removes clip_id's push or gradient wipe"}))], &["clip_id"]),
        run: |ss, a| {
            let o = a.as_object().ok_or("add_push_wipe: arguments must be an object")?;
            for k in o.keys() { if !ARGS.contains(&k.as_str()) { return Err(format!("add_push_wipe: unknown argument '{k}' (valid: {})", ARGS.join(", "))); } }
            let cid = str_arg(a, "clip_id")?.to_string();
            let c = ss.project.find_clip(&cid).ok_or_else(|| format!("no such clip {cid}"))?;
            if a.get("remove").and_then(|v| v.as_bool()).unwrap_or(false) {
                let had = c.transition_in.as_ref().is_some_and(|t| crate::pushwipe::KINDS.contains(&t.kind.as_str()));
                if had { ss.project.find_clip_mut(&cid).unwrap().transition_in = None; }
                return Ok(json!({"removed": had})); }
            let kind = a.get("kind").and_then(|v| v.as_str()).ok_or("add_push_wipe needs kind: push or gradient_wipe")?.to_string();
            if !crate::pushwipe::KINDS.contains(&kind.as_str()) { return Err(format!("kind must be push or gradient_wipe, got '{kind}'")); }
            let push = kind == "push";
            if push { if let Some(k) = WIPE_ONLY.iter().find(|k| o.contains_key(**k)) { return Err(format!("'{k}' is a gradient_wipe setting; push takes direction and ease")); } }
            else if o.contains_key("direction") { return Err("'direction' is a push setting; gradient_wipe takes gradient, angle, center, luma_clip_id, softness and invert".into()); }
            let d = match a.get("duration") { None => 0.5, Some(v) => v.as_f64().ok_or("duration must be a number of seconds")? };
            if !(0.05..=10.0).contains(&d) { return Err(format!("duration must be between 0.05 and 10 seconds, got {d}")); }
            if d > c.duration + 1e-9 { return Err(format!("duration {d} s is longer than clip {cid} ({:.3} s)", c.duration)); }
            let ease = match a.get("ease") { None => "in_out_cubic".to_string(), Some(v) => v.as_str().ok_or("ease must be a string")?.to_string() };
            if !crate::transitions::EASES.contains(&ease.as_str()) { return Err(format!("unknown ease '{ease}' (valid: {})", crate::transitions::EASES.join(", "))); }
            let direction = match a.get("direction") { None => if push { "left".to_string() } else { String::new() }, Some(v) => v.as_str().ok_or("direction must be a string")?.to_string() };
            if push && !crate::pushwipe::DIRECTIONS.contains(&direction.as_str()) { return Err(format!("direction must be left, right, up or down, got '{direction}'")); }
            let gradient = match a.get("gradient") { None => if push { String::new() } else { "linear".to_string() }, Some(v) => v.as_str().ok_or("gradient must be a string")?.to_string() };
            if !push && !crate::pushwipe::GRADIENTS.contains(&gradient.as_str()) { return Err(format!("gradient must be linear, radial or angular, got '{gradient}'")); }
            let angle = match a.get("angle") { None => 0.0, Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or("angle must be a number of degrees")? };
            let softness = match a.get("softness") { None => 0.1, Some(v) => v.as_f64().ok_or("softness must be a number 0..1")? };
            if !(0.0..=1.0).contains(&softness) { return Err(format!("softness must be between 0 and 1, got {softness}")); }
            let center = match a.get("center") { None => [0.5, 0.5], Some(v) => { let arr = v.as_array().filter(|x| x.len() == 2).ok_or("center must be [x, y] in 0..1")?;
                let (x, y) = (arr[0].as_f64().ok_or("center must be numbers")?, arr[1].as_f64().ok_or("center must be numbers")?); if !(x.is_finite() && y.is_finite()) { return Err("center must be finite".into()); } [x, y] } };
            let invert = match a.get("invert") { None => false, Some(v) => v.as_bool().ok_or("invert must be true or false")? };
            let luma = match a.get("luma_clip_id") { None => None, Some(v) => { let id = v.as_str().ok_or("luma_clip_id must be a clip id")?.to_string();
                if id == cid { return Err("luma_clip_id must be another clip, not the incoming clip itself".into()); }
                if ss.project.find_clip(&id).is_none() { return Err(format!("no such clip {id} (luma_clip_id)")); } Some(id) } };
            let spec = TransitionSpec { direction, gradient, luma_clip: luma.clone(), center, invert };
            let tr = Transition { kind: kind.clone(), duration: d, ease: ease.clone(), softness, angle, color: [0.0; 3], intensity: 1.0, count: 10.0, seed: 0, spec: Some(spec) };
            ss.project.find_clip_mut(&cid).unwrap().transition_in = Some(tr);
            let cc = ss.project.find_clip(&cid).unwrap(); let hs = crate::pushwipe::handle_secs(&ss.project, cc);
            let outgoing = match crate::pushwipe::abut_prev(&ss.project, cc) { Some(p) if hs > 1e-9 => format!("clip {} (ends where {cid} starts; it keeps playing {hs:.2} s past its out point from its media)", p.id),
                Some(p) => format!("clip {} (ends where {cid} starts; its last frame is held: no media past its out point)", p.id), None => "the layers under the incoming clip (no clip ends where it starts on its track)".to_string() };
            let mut r = json!({"clip_id": cid, "kind": kind, "duration": d, "ease": ease, "outgoing": outgoing});
            if let Some(l) = luma { r["luma_clip_id"] = json!(l); r["note"] = json!("the luma clip is hidden from the composite and sampled at the same time (held to its own span)"); }
            Ok(r) } },
] }
