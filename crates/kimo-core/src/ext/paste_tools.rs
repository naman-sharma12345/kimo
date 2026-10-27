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
    Command { name: "copy_keyframes", description: "Copy one property's keyframes (base value, keys, eases/bezier handles, expression ops) from one clip onto one or more other clips: graph-editor copy/paste keyframes. Source is a clip transform property, or an effect param when from_effect_id is given. Destination property/effect default to the source's; to_effect_id applies the same effect id on every to_clip_id (pass one target clip for a per-effect paste onto a different clip's effect). time_offset shifts every copied key's time (clip-local seconds). replace (default true) clears the destination's existing keys first; false merges, overwriting only keys at matching times and keeping the rest.", mutates: true,
        schema: obj(vec![
            ("from_clip_id", s("clip to copy from")),
            ("from_property", s("transform property or effect-param name to copy")),
            ("from_effect_id", s("optional: read from this effect's params instead of a clip transform prop")),
            ("to_clip_ids", json!({"type":"array","items":{"type":"string"},"description":"clips to paste onto"})),
            ("to_property", s("optional: defaults to from_property")),
            ("to_effect_id", s("optional: paste onto this effect's params on every to_clip_id instead of a clip transform prop; defaults to from_effect_id")),
            ("time_offset", n("optional seconds added to every copied key's time")),
            ("replace", json!({"type":"boolean","description":"default true: clear the destination property's keys first. false: merge, overwriting only keys at matching times"}))],
            &["from_clip_id","from_property","to_clip_ids"]),
        run: |ss, a| {
            let from_clip = str_arg(a, "from_clip_id")?.to_string();
            let from_prop_name = str_arg(a, "from_property")?.to_string();
            let from_eff = a.get("from_effect_id").and_then(|v| v.as_str()).map(String::from);
            let to_clips: Vec<String> = a.get("to_clip_ids").and_then(|v| v.as_array()).ok_or("to_clip_ids must be an array of clip ids")?
                .iter().map(|v| v.as_str().map(String::from).ok_or("to_clip_ids must hold clip id strings")).collect::<Result<_, _>>()?;
            if to_clips.is_empty() { return Err("to_clip_ids is empty: name at least one clip to paste onto".into()); }
            let to_prop_name = a.get("to_property").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| from_prop_name.clone());
            let to_eff = a.get("to_effect_id").and_then(|v| v.as_str()).map(String::from).or_else(|| from_eff.clone());
            let offset = a.get("time_offset").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if !offset.is_finite() { return Err("time_offset must be a finite number".into()); }
            let replace = a.get("replace").and_then(|v| v.as_bool()).unwrap_or(true);

            let src_clip = ss.project.find_clip(&from_clip).ok_or_else(|| format!("no such clip {from_clip}"))?;
            let src_prop: crate::keyframe::Prop = match &from_eff {
                Some(eid) => src_clip.effects.iter().find(|e| e.id == *eid).ok_or_else(|| format!("no such effect {eid} on clip {from_clip}"))?
                    .params.get(&from_prop_name).cloned().ok_or_else(|| format!("no such param '{from_prop_name}' on effect {eid}"))?,
                None => src_clip.props.get(&from_prop_name).cloned().ok_or_else(|| format!("no such property '{from_prop_name}' on clip {from_clip}"))?,
            };
            if src_prop.keys.is_empty() { return Err(format!("'{from_prop_name}' on clip {from_clip} has no keyframes to copy")); }
            let mut copied = src_prop.clone();
            if offset != 0.0 { for k in copied.keys.iter_mut() { k.t += offset; } }

            let mut pasted = vec![];
            for tc in &to_clips {
                if tc == &from_clip && to_eff == from_eff && to_prop_name == from_prop_name { return Err(format!("{tc} is the source (same clip, effect and property): nothing to paste onto")); }
                let dst = ss.project.find_clip_mut(tc).ok_or_else(|| format!("no such clip {tc}"))?;
                let slot = match &to_eff {
                    Some(eid) => dst.effects.iter_mut().find(|e| e.id == *eid).ok_or_else(|| format!("no such effect {eid} on clip {tc}"))?.params.entry(to_prop_name.clone()).or_insert_with(|| crate::keyframe::Prop::new(copied.base)),
                    None => dst.props.entry(to_prop_name.clone()).or_insert_with(|| crate::keyframe::Prop::new(copied.base)),
                };
                if replace { *slot = copied.clone(); } else {
                    slot.base = copied.base; slot.expr = copied.expr.clone();
                    for k in &copied.keys { slot.keys.retain(|x| (x.t - k.t).abs() > 1e-9); slot.keys.push(k.clone()); }
                    slot.keys.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
                }
                pasted.push(tc.clone());
            }
            Ok(json!({"from": from_clip, "property": from_prop_name, "pasted_onto": pasted, "keys": copied.keys.len(), "time_offset": offset, "replace": replace})) } },
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
                    "speed" => { c.speed = src.speed; c.speed_curve = src.speed_curve.as_ref().map(|p| scaled(p, k)); c.speed_blend = src.speed_blend; c.time_remap = src.time_remap.clone(); c.time_mode = src.time_mode.clone(); }
                    "audio" => { let (lead, tail, duck) = (c.audio.lead, c.audio.tail, c.audio.duck_gain.clone()); c.audio = src.audio.clone(); c.audio.duck_gain = duck; c.audio.lead = lead; c.audio.tail = tail; c.audio.volume = scaled(&src.audio.volume, k); }
                    _ => {} } }
                pasted.push(id.clone());
            }
            Ok(json!({"from": from, "pasted_onto": pasted, "attributes": attrs, "scale_times": scale_times})) } },
] }
