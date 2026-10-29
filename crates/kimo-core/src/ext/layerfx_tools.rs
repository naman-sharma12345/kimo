//! add_displacement_map and set_matte (After Effects' Displacement Map and Set Matte): effects on a clip driven by another
//! clip of the project (layerfx.rs). Each is one effect in the clip's stack (kind displacement_map / set_matte), so
//! set_effect_param, set_keyframe (max_h, max_v), set_effect_mask and remove_effect work on it. A second call updates it.
use super::kit::*;
use crate::keyframe::Prop;
use crate::model::*;

fn check_args(a: &Value, tool: &str, ok: &[&str]) -> Result<(), String> { for k in a.as_object().ok_or_else(|| format!("{tool}: arguments must be an object"))?.keys() { if !ok.contains(&k.as_str()) { return Err(format!("{tool}: unknown argument '{k}' (valid: {})", ok.join(", "))); } } Ok(()) }
fn word(a: &Value, k: &str, d: &str, ok: &[&str]) -> Result<String, String> { let v = match a.get(k) { None => d.to_string(), Some(v) => v.as_str().ok_or_else(|| format!("{k} must be a string"))?.to_string() };
    if ok.contains(&v.as_str()) { Ok(v) } else { Err(format!("{k} must be one of {}, got '{v}'", ok.join(", "))) } }
fn flag(a: &Value, k: &str, d: bool) -> Result<bool, String> { match a.get(k) { None => Ok(d), Some(v) => v.as_bool().ok_or_else(|| format!("{k} must be true or false")) } }
/// The source clip id, checked: it exists, is not the clip itself.
fn source(ss: &Session, a: &Value, k: &str, cid: &str) -> Result<String, String> { let id = str_arg(a, k)?.to_string(); if id == cid { return Err(format!("{k} must be another clip, not {cid} itself")); }
    if ss.project.find_clip(&id).is_none() { return Err(format!("no such clip {id} ({k})")); } Ok(id) }
/// Insert or update the clip's one effect of `kind`; returns its id and whether it was new.
fn upsert(ss: &mut Session, cid: &str, kind: &str, params: Vec<(&str, f64)>, strings: Vec<(&str, String)>) -> Result<(String, bool), String> {
    let c = ss.project.find_clip(cid).ok_or_else(|| format!("no such clip {cid}"))?; let existing = c.effects.iter().find(|e| e.kind == kind).map(|e| e.id.clone());
    let id = match &existing { Some(id) => id.clone(), None => ss.project.gen_id("fx") }; let c = ss.project.find_clip_mut(cid).unwrap();
    if existing.is_none() { c.effects.push(Effect { id: id.clone(), kind: kind.into(), params: Default::default(), strings: Default::default() }); }
    let e = c.effects.iter_mut().find(|e| e.id == id).unwrap();
    for (k, v) in params { e.params.insert(k.into(), Prop::new(v)); } for (k, v) in strings { e.strings.insert(k.into(), v); }
    Ok((id, existing.is_none()))
}
fn remove(ss: &mut Session, cid: &str, kind: &str) -> Result<Value, String> { let c = ss.project.find_clip_mut(cid).ok_or_else(|| format!("no such clip {cid}"))?; let n = c.effects.len(); c.effects.retain(|e| e.kind != kind); Ok(json!({"removed": n - c.effects.len()})) }

const DM: &[&str] = &["clip_id", "map_clip_id", "max_horizontal", "max_vertical", "horizontal_channel", "vertical_channel", "edges", "map_space", "hide_map", "remove"];
const SM: &[&str] = &["clip_id", "matte_clip_id", "channel", "invert", "hide_matte", "remove"];
const CC: &[&str] = &["clip_id", "ref_clip_id", "operation", "opacity", "ref_channel", "hide_ref", "remove"];
const SC: &[&str] = &["clip_id", "ref_clip_id", "r_source", "r_channel", "g_source", "g_channel", "b_source", "b_channel", "a_source", "a_channel", "hide_ref", "remove"];
const MG: &[&str] = &["clip_id", "ref_clip_id", "strength", "size", "hide_ref", "remove"];

pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_displacement_map", description: "Displacement Map: warp a clip by another clip's channels (glass, liquid, heat haze); grey 0.5 = no shift, per-axis max px.", mutates: true,
        schema: obj(vec![("clip_id", s("the clip to warp")), ("map_clip_id", s("the clip whose pixels drive the shift (rendered alone at the same time)")),
            ("max_horizontal", n("px at value 0 / 1 of the horizontal channel, -2000..2000, default 20")), ("max_vertical", n("px for the vertical channel, -2000..2000, default 20")),
            ("horizontal_channel", json!({"type":"string","enum":["red","green","blue","alpha","luma","hue","saturation","lightness","off"],"description":"default red"})),
            ("vertical_channel", json!({"type":"string","enum":["red","green","blue","alpha","luma","hue","saturation","lightness","off"],"description":"default green"})),
            ("edges", json!({"type":"string","enum":crate::layerfx::EDGES,"description":"pixels pulled from outside the frame: clamp (default), wrap or transparent"})),
            ("map_space", json!({"type":"string","enum":crate::layerfx::MAP_SPACES,"description":"screen (default): the map is read where the clip lands, after its scale/move/rotation; clip: in the clip's own frame"})),
            ("hide_map", json!({"type":"boolean","description":"hide the map clip from the composite, default true"})),
            ("remove", json!({"type":"boolean","description":"true removes the clip's displacement map"}))], &["clip_id"]),
        run: |ss, a| { check_args(a, "add_displacement_map", DM)?; let cid = str_arg(a, "clip_id")?.to_string(); ss.project.find_clip(&cid).ok_or_else(|| format!("no such clip {cid}"))?;
            if flag(a, "remove", false)? { return remove(ss, &cid, "displacement_map"); }
            let map = source(ss, a, "map_clip_id", &cid)?; let mut chans: Vec<&str> = crate::layerfx::CHANNELS.to_vec(); chans.push("off");
            let (hc, vc, edges) = (word(a, "horizontal_channel", "red", &chans)?, word(a, "vertical_channel", "green", &chans)?, word(a, "edges", "clamp", crate::layerfx::EDGES)?); let space = word(a, "map_space", "screen", crate::layerfx::MAP_SPACES)?;
            let px = |k: &str| -> Result<f64, String> { match a.get(k) { None => Ok(20.0), Some(v) => { let x = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{k} must be a number of px"))?; if x.abs() > 2000.0 { Err(format!("{k} must be within -2000..2000 px, got {x}")) } else { Ok(x) } } } };
            let (mh, mv) = (px("max_horizontal")?, px("max_vertical")?); let hide = flag(a, "hide_map", true)?;
            let (id, new) = upsert(ss, &cid, "displacement_map", vec![("max_h", mh), ("max_v", mv)], vec![("map_clip", map.clone()), ("h_channel", hc), ("v_channel", vc), ("edges", edges), ("map_space", space.clone()), ("hide", if hide { "1" } else { "0" }.into())])?;
            Ok(json!({"effect_id": id, "created": new, "map_clip_id": map, "map_space": space, "hidden": hide})) } },
    Command { name: "set_matte", description: "Set Matte: a clip's alpha from another clip's channel (alpha, luma, red, green, blue, hue, saturation, lightness), optional invert.", mutates: true,
        schema: obj(vec![("clip_id", s("the clip to matte")), ("matte_clip_id", s("the clip whose channel becomes the matte (rendered alone at the same time)")),
            ("channel", json!({"type":"string","enum":crate::layerfx::CHANNELS,"description":"default alpha"})),
            ("invert", json!({"type":"boolean","description":"use 1 - channel"})),
            ("hide_matte", json!({"type":"boolean","description":"hide the matte clip from the composite, default true"})),
            ("remove", json!({"type":"boolean","description":"true removes the clip's set matte"}))], &["clip_id"]),
        run: |ss, a| { check_args(a, "set_matte", SM)?; let cid = str_arg(a, "clip_id")?.to_string(); ss.project.find_clip(&cid).ok_or_else(|| format!("no such clip {cid}"))?;
            if flag(a, "remove", false)? { return remove(ss, &cid, "set_matte"); }
            let m = source(ss, a, "matte_clip_id", &cid)?; let ch = word(a, "channel", "alpha", crate::layerfx::CHANNELS)?; let (inv, hide) = (flag(a, "invert", false)?, flag(a, "hide_matte", true)?);
            let (id, new) = upsert(ss, &cid, "set_matte", vec![], vec![("matte_clip", m.clone()), ("channel", ch), ("invert", if inv { "1" } else { "0" }.into()), ("hide", if hide { "1" } else { "0" }.into())])?;
            Ok(json!({"effect_id": id, "created": new, "matte_clip_id": m, "hidden": hide})) } },
    Command { name: "add_channel_combiner", description: "Channel Combiner (AE's Calculations): combine a clip with ANOTHER clip using add/subtract/multiply/screen/difference/darken/lighten. The true two-clip arithmetic the clip's own `blend` field (against whatever sits below it on the timeline) can't express, since the second input here is any clip you name, not just what's underneath.", mutates: true,
        schema: obj(vec![("clip_id", s("the clip to combine onto")), ("ref_clip_id", s("the other clip supplying the second input (rendered alone at the same time)")),
            ("operation", json!({"type":"string","enum":["add","subtract","multiply","screen","difference","darken","lighten"],"description":"default add"})),
            ("opacity", n("0..1, how much of the combined result replaces the clip's own pixel, default 1")),
            ("ref_channel", json!({"type":"string","enum":["rgb","red","green","blue","alpha","luma","hue","saturation","lightness"],"description":"default rgb (full colour); any single channel is broadcast to all three output channels"})),
            ("hide_ref", json!({"type":"boolean","description":"hide the reference clip from the composite, default true"})),
            ("remove", json!({"type":"boolean","description":"true removes the clip's channel combiner"}))], &["clip_id"]),
        run: |ss, a| { check_args(a, "add_channel_combiner", CC)?; let cid = str_arg(a, "clip_id")?.to_string(); ss.project.find_clip(&cid).ok_or_else(|| format!("no such clip {cid}"))?;
            if flag(a, "remove", false)? { return remove(ss, &cid, "channel_combiner"); }
            let r = source(ss, a, "ref_clip_id", &cid)?;
            let op = word(a, "operation", "add", &["add", "subtract", "multiply", "screen", "difference", "darken", "lighten"])?;
            let op_f = match a.get("opacity") { None => Ok(1.0), Some(v) => v.as_f64().filter(|x| x.is_finite() && (0.0..=1.0).contains(x)).ok_or_else(|| "opacity must be 0..1".to_string()) }?;
            let mut rch_ok: Vec<&str> = vec!["rgb"]; rch_ok.extend(crate::layerfx::CHANNELS); let rch = word(a, "ref_channel", "rgb", &rch_ok)?;
            let hide = flag(a, "hide_ref", true)?;
            let (id, new) = upsert(ss, &cid, "channel_combiner", vec![("opacity", op_f)], vec![("ref_clip", r.clone()), ("operation", op), ("ref_channel", rch), ("hide", if hide { "1" } else { "0" }.into())])?;
            Ok(json!({"effect_id": id, "created": new, "ref_clip_id": r, "hidden": hide})) } },
    Command { name: "add_set_channels", description: "Set Channels (AE's Set Channels): build each output channel (red, green, blue, alpha) from an arbitrary source. The source can be this clip's own data, a named channel of another referenced clip, or a flat on/off value -- the real gap `shift_channels` (this-clip-only) leaves open: a second clip as a channel source.", mutates: true,
        schema: obj(vec![("clip_id", s("the clip to rebuild channels on")), ("ref_clip_id", s("optional: the other clip any channel's source can read from (rendered alone at the same time); required only if some channel's source is \"ref\"")),
            ("r_source", json!({"type":"string","enum":["this","ref","on","off"],"description":"default this (this clip's own red channel)"})),
            ("r_channel", json!({"type":"string","enum":crate::layerfx::CHANNELS,"description":"which channel to read when r_source is this or ref; default red"})),
            ("g_source", json!({"type":"string","enum":["this","ref","on","off"],"description":"default this (this clip's own green channel)"})),
            ("g_channel", json!({"type":"string","enum":crate::layerfx::CHANNELS,"description":"which channel to read when g_source is this or ref; default green"})),
            ("b_source", json!({"type":"string","enum":["this","ref","on","off"],"description":"default this (this clip's own blue channel)"})),
            ("b_channel", json!({"type":"string","enum":crate::layerfx::CHANNELS,"description":"which channel to read when b_source is this or ref; default blue"})),
            ("a_source", json!({"type":"string","enum":["this","ref","on","off"],"description":"default this (this clip's own alpha channel)"})),
            ("a_channel", json!({"type":"string","enum":crate::layerfx::CHANNELS,"description":"which channel to read when a_source is this or ref; default alpha"})),
            ("hide_ref", json!({"type":"boolean","description":"hide the reference clip from the composite, default true"})),
            ("remove", json!({"type":"boolean","description":"true removes the clip's set channels"}))], &["clip_id"]),
        run: |ss, a| { check_args(a, "add_set_channels", SC)?; let cid = str_arg(a, "clip_id")?.to_string(); ss.project.find_clip(&cid).ok_or_else(|| format!("no such clip {cid}"))?;
            if flag(a, "remove", false)? { return remove(ss, &cid, "set_channels"); }
            const SRC_OK: &[&str] = &["this", "ref", "on", "off"];
            let (rs, rc) = (word(a, "r_source", "this", SRC_OK)?, word(a, "r_channel", "red", crate::layerfx::CHANNELS)?);
            let (gs, gc) = (word(a, "g_source", "this", SRC_OK)?, word(a, "g_channel", "green", crate::layerfx::CHANNELS)?);
            let (bs, bc) = (word(a, "b_source", "this", SRC_OK)?, word(a, "b_channel", "blue", crate::layerfx::CHANNELS)?);
            let (as_, ac) = (word(a, "a_source", "this", SRC_OK)?, word(a, "a_channel", "alpha", crate::layerfx::CHANNELS)?);
            let any_ref = [&rs, &gs, &bs, &as_].iter().any(|s| s.as_str() == "ref");
            let r = if any_ref || a.get("ref_clip_id").is_some() { Some(source(ss, a, "ref_clip_id", &cid)?) } else { None };
            if any_ref && r.is_none() { return Err("add_set_channels: at least one channel's source is \"ref\" but ref_clip_id was not given".into()); }
            let hide = flag(a, "hide_ref", true)?;
            let mut strings: Vec<(&str, String)> = vec![("r_source", rs), ("r_channel", rc), ("g_source", gs), ("g_channel", gc), ("b_source", bs), ("b_channel", bc), ("a_source", as_), ("a_channel", ac), ("hide", if hide { "1" } else { "0" }.into())];
            if let Some(rc2) = &r { strings.push(("ref_clip", rc2.clone())); }
            let (id, new) = upsert(ss, &cid, "set_channels", vec![], strings)?;
            Ok(json!({"effect_id": id, "created": new, "ref_clip_id": r, "hidden": hide})) } },
    Command { name: "add_match_grain", description: "Match Grain: estimate a reference clip's grain amplitude and reproduce equivalent synthetic grain on this clip, via the existing grain engine. The estimate is a high-pass residual of the reference's pixels; grain SIZE is not estimated, only amplitude -- an honest simplification.", mutates: true,
        schema: obj(vec![("clip_id", s("the clip to add matching grain to")), ("ref_clip_id", s("the clip whose grain is measured (rendered alone at the same time)")),
            ("strength", n("multiplies the estimated grain amount, default 1")), ("size", n("the synthetic grain's own size in px, 1..32, default 1")),
            ("hide_ref", json!({"type":"boolean","description":"hide the reference clip from the composite, default true"})),
            ("remove", json!({"type":"boolean","description":"true removes the clip's match grain"}))], &["clip_id"]),
        run: |ss, a| { check_args(a, "add_match_grain", MG)?; let cid = str_arg(a, "clip_id")?.to_string(); ss.project.find_clip(&cid).ok_or_else(|| format!("no such clip {cid}"))?;
            if flag(a, "remove", false)? { return remove(ss, &cid, "match_grain"); }
            let r = source(ss, a, "ref_clip_id", &cid)?;
            let strength = match a.get("strength") { None => Ok(1.0), Some(v) => v.as_f64().filter(|x| x.is_finite() && *x >= 0.0).ok_or_else(|| "strength must be >= 0".to_string()) }?;
            let size = match a.get("size") { None => Ok(1.0), Some(v) => v.as_f64().filter(|x| x.is_finite() && (1.0..=32.0).contains(x)).ok_or_else(|| "size must be 1..32".to_string()) }?;
            let hide = flag(a, "hide_ref", true)?;
            let (id, new) = upsert(ss, &cid, "match_grain", vec![("strength", strength), ("size", size)], vec![("ref_clip", r.clone()), ("hide", if hide { "1" } else { "0" }.into())])?;
            Ok(json!({"effect_id": id, "created": new, "ref_clip_id": r, "hidden": hide})) } },
] }
