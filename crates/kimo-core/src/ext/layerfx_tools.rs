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

const DM: &[&str] = &["clip_id", "map_clip_id", "max_horizontal", "max_vertical", "horizontal_channel", "vertical_channel", "edges", "hide_map", "remove"];
const SM: &[&str] = &["clip_id", "matte_clip_id", "channel", "invert", "hide_matte", "remove"];

pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_displacement_map", description: "Displacement Map: warp a clip by another clip's channels (glass, liquid, heat haze); grey 0.5 = no shift, per-axis max px.", mutates: true,
        schema: obj(vec![("clip_id", s("the clip to warp")), ("map_clip_id", s("the clip whose pixels drive the shift (rendered alone at the same time)")),
            ("max_horizontal", n("px at value 0 / 1 of the horizontal channel, -2000..2000, default 20")), ("max_vertical", n("px for the vertical channel, -2000..2000, default 20")),
            ("horizontal_channel", json!({"type":"string","enum":["red","green","blue","alpha","luma","hue","saturation","lightness","off"],"description":"default red"})),
            ("vertical_channel", json!({"type":"string","enum":["red","green","blue","alpha","luma","hue","saturation","lightness","off"],"description":"default green"})),
            ("edges", json!({"type":"string","enum":crate::layerfx::EDGES,"description":"pixels pulled from outside the frame: clamp (default), wrap or transparent"})),
            ("hide_map", json!({"type":"boolean","description":"hide the map clip from the composite, default true"})),
            ("remove", json!({"type":"boolean","description":"true removes the clip's displacement map"}))], &["clip_id"]),
        run: |ss, a| { check_args(a, "add_displacement_map", DM)?; let cid = str_arg(a, "clip_id")?.to_string(); ss.project.find_clip(&cid).ok_or_else(|| format!("no such clip {cid}"))?;
            if flag(a, "remove", false)? { return remove(ss, &cid, "displacement_map"); }
            let map = source(ss, a, "map_clip_id", &cid)?; let mut chans: Vec<&str> = crate::layerfx::CHANNELS.to_vec(); chans.push("off");
            let (hc, vc, edges) = (word(a, "horizontal_channel", "red", &chans)?, word(a, "vertical_channel", "green", &chans)?, word(a, "edges", "clamp", crate::layerfx::EDGES)?);
            let px = |k: &str| -> Result<f64, String> { match a.get(k) { None => Ok(20.0), Some(v) => { let x = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{k} must be a number of px"))?; if x.abs() > 2000.0 { Err(format!("{k} must be within -2000..2000 px, got {x}")) } else { Ok(x) } } } };
            let (mh, mv) = (px("max_horizontal")?, px("max_vertical")?); let hide = flag(a, "hide_map", true)?;
            let (id, new) = upsert(ss, &cid, "displacement_map", vec![("max_h", mh), ("max_v", mv)], vec![("map_clip", map.clone()), ("h_channel", hc), ("v_channel", vc), ("edges", edges), ("hide", if hide { "1" } else { "0" }.into())])?;
            Ok(json!({"effect_id": id, "created": new, "map_clip_id": map, "hidden": hide})) } },
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
] }
