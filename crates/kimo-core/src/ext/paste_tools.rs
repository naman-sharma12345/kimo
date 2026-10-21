//! paste_attributes (Premiere Pro Paste Attributes): copy chosen attributes of one clip, with their keyframes and expressions, onto
//! other clips. motion = position, scale, rotation, 3D rotation and z, motion path, corner pin, motion blur; opacity = opacity and
//! blend mode; effects = the effect stack (fresh effect ids; append or replace) plus echo and posterize time; mask; grade = the grade graph; speed = speed,
//! speed ramp, time remap and time mode; audio = volume, pan, mute, fades and audio effects (the target keeps its own ducking and
//! split-edit offsets). scale_times stretches the keyframe times of motion, opacity, effects, speed and volume to each target's
//! duration (Premiere's "Scale Attribute Times"); mask path keys keep their times.
use super::kit::*;
use crate::model::*;

pub const ATTRS: &[&str] = &["motion", "opacity", "effects", "mask", "grade", "speed", "audio"];
const MOTION_PROPS: &[&str] = &["x", "y", "scale", "rotation", "rot_x", "rot_y", "pos_z", "path"];

fn scaled(p: &crate::keyframe::Prop, k: f64) -> crate::keyframe::Prop {
    let mut p = p.clone(); if (k - 1.0).abs() < 1e-12 { return p; }
    for key in p.keys.iter_mut() { key.t *= k; if let Some(h) = key.out.as_mut() { h.dt *= k; } if let Some(h) = key.inn.as_mut() { h.dt *= k; } } p
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "paste_attributes", description: "Paste Attributes: copy a clip's motion, opacity, effects, mask, grade, speed or audio (with keyframes) onto other clips.", mutates: true,
        schema: obj(vec![("from_clip_id", s("the clip to copy from")),
            ("to_clip_ids", json!({"type":"array","items":{"type":"string"},"description":"clips to paste onto"})),
            ("attributes", json!({"type":"array","items":{"type":"string","enum":ATTRS},"description":"which attributes; default all except speed and audio"})),
            ("scale_times", json!({"type":"boolean","description":"stretch keyframe times to each target's duration (default false: same clip-local times)"})),
            ("effects_mode", json!({"type":"string","enum":["replace","append"],"description":"effects: replace the target's stack (default) or append to it"}))], &["from_clip_id","to_clip_ids"]),
        run: |ss, a| {
            for k in a.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default() { if !["from_clip_id", "to_clip_ids", "attributes", "scale_times", "effects_mode"].contains(&k.as_str()) { return Err(format!("paste_attributes: unknown argument '{k}' (valid: from_clip_id, to_clip_ids, attributes, scale_times, effects_mode)")); } }
            let from = str_arg(a, "from_clip_id")?.to_string();
            let to: Vec<String> = a.get("to_clip_ids").and_then(|v| v.as_array()).ok_or("to_clip_ids must be an array of clip ids")?.iter().map(|v| v.as_str().map(String::from).ok_or("to_clip_ids must hold clip id strings")).collect::<Result<_, _>>()?;
            if to.is_empty() { return Err("to_clip_ids is empty: name at least one clip to paste onto".into()); }
            let attrs: Vec<String> = match a.get("attributes") { None => vec!["motion".into(), "opacity".into(), "effects".into(), "mask".into(), "grade".into()], Some(v) if v.is_null() => vec!["motion".into(), "opacity".into(), "effects".into(), "mask".into(), "grade".into()],
                Some(v) => v.as_array().ok_or("attributes must be an array")?.iter().map(|x| x.as_str().map(String::from).ok_or("attributes must be strings")).collect::<Result<_, _>>()? };
            for x in &attrs { if !ATTRS.contains(&x.as_str()) { return Err(format!("unknown attribute '{x}' (valid: {})", ATTRS.join(", "))); } }
            if attrs.is_empty() { return Err("attributes is empty: name what to paste (motion, opacity, effects, mask, grade, speed, audio)".into()); }
            let scale_times = a.get("scale_times").and_then(|v| v.as_bool()).unwrap_or(false);
            let append = match a.get("effects_mode").and_then(|v| v.as_str()) { None | Some("replace") => false, Some("append") => true, Some(o) => return Err(format!("effects_mode must be replace or append, got '{o}'")) };
            let src = ss.project.find_clip(&from).ok_or_else(|| format!("no such clip {from}"))?.clone();
            for id in &to { if id == &from { return Err(format!("{id} is the source clip: paste onto other clips")); } let c = ss.project.find_clip(id).ok_or_else(|| format!("no such clip {id}"))?;
                if attrs.iter().any(|x| x == "speed") && matches!(c.source, Source::Precomp { .. }) != matches!(src.source, Source::Precomp { .. }) { return Err(format!("speed cannot be pasted between a precomp and a non-precomp clip ({id})")); } }
            // an effect limited to a mask layer (set_effect_mask) needs that layer on the target: refuse rather than paste a dangling reference
            if attrs.iter().any(|x| x == "effects") && !attrs.iter().any(|x| x == "mask") { for e in &src.effects { if let Some(ml) = e.strings.get("mask_layer") { for id in &to {
                let has = ss.project.find_clip(id).unwrap().mask.as_ref().is_some_and(|m| m.layers.iter().any(|l| &l.id == ml));
                if !has { return Err(format!("effect {} ({}) is limited to mask layer {ml}, which {id} does not have: add \"mask\" to attributes, or clear it on the source with set_effect_mask", e.id, e.kind)); } } } } }
            let mut pasted = vec![];
            for id in &to {
                let k = if scale_times && src.duration > 1e-9 { ss.project.find_clip(id).unwrap().duration / src.duration } else { 1.0 };
                let mut new_ids: Vec<String> = vec![]; if attrs.iter().any(|x| x == "effects") { for _ in 0..src.effects.len() { new_ids.push(ss.project.gen_id("fx")); } }
                let c = ss.project.find_clip_mut(id).unwrap();
                for x in &attrs { match x.as_str() {
                    "motion" => { for p in MOTION_PROPS { match src.props.get(*p) { Some(v) => { c.props.insert((*p).into(), scaled(v, k)); } None => { c.props.remove(*p); } } }
                        c.motion_path = src.motion_path.clone(); c.corners = src.corners; c.motion_blur = src.motion_blur; }
                    "opacity" => { match src.props.get("opacity") { Some(v) => { c.props.insert("opacity".into(), scaled(v, k)); } None => { c.props.remove("opacity"); } } c.blend = src.blend.clone(); } // xform-lint: ok (copies the keyframed Prop itself for paste_attributes, not a render read)
                    "effects" => { let mut fx: Vec<Effect> = src.effects.iter().zip(&new_ids).map(|(e, nid)| { let mut e = e.clone(); e.id = nid.clone(); for p in e.params.values_mut() { *p = scaled(p, k); } e }).collect();
                        if append { c.effects.append(&mut fx); } else { c.effects = fx; } c.echo = src.echo.clone(); c.posterize_fps = src.posterize_fps; }
                    "mask" => { c.mask = src.mask.clone(); }
                    "grade" => { c.grade_graph = src.grade_graph.clone(); }
                    "speed" => { c.speed = src.speed; c.speed_curve = src.speed_curve.as_ref().map(|p| scaled(p, k)); c.time_remap = src.time_remap.clone(); c.time_mode = src.time_mode.clone(); }
                    "audio" => { let (lead, tail, duck) = (c.audio.lead, c.audio.tail, c.audio.duck_gain.clone()); c.audio = src.audio.clone(); c.audio.duck_gain = duck; c.audio.lead = lead; c.audio.tail = tail; c.audio.volume = scaled(&src.audio.volume, k); }
                    _ => {} } }
                pasted.push(id.clone());
            }
            Ok(json!({"from": from, "pasted_onto": pasted, "attributes": attrs, "scale_times": scale_times})) } },
] }
