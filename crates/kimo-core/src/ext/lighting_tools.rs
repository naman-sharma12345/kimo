//! Shared light/material model MCP surface: project lights, named PBR materials, the relight effect, and a lit preview.
use super::kit::*;
use crate::lighting::{self, LightDef, Material};
use crate::model::Effect;
use std::collections::BTreeMap;

const RELIGHT: &[(&str, f64)] = &[("amount", 1.0), ("bump", 1.0), ("blur", 6.0), ("ambient", 0.15), ("shadow", 0.6), ("spec_gain", 1.0), ("height", 1.0)];
fn find_clip<'a>(ss: &'a mut Session, a: &Value) -> Result<&'a mut crate::model::Clip, String> { let cid = str_arg(a, "clip_id")?.to_string(); ss.project.tracks.iter_mut().flat_map(|t| t.clips.iter_mut()).find(|c| c.id == cid).ok_or(format!("no such clip '{cid}'")) }
fn merge<T: serde::Serialize + serde::de::DeserializeOwned>(base: &T, a: &Value, skip: &[&str]) -> Result<T, String> {
    let mut v = serde_json::to_value(base).map_err(|e| e.to_string())?; let o = v.as_object_mut().ok_or("bad base")?;
    if let Some(m) = a.as_object() { for (k, x) in m { if skip.contains(&k.as_str()) { continue; } o.insert(k.clone(), x.clone()); } }
    serde_json::from_value(v).map_err(|e| format!("invalid value: {e}"))
}
fn light_json(id: &str, l: &LightDef) -> Value { let mut v = serde_json::to_value(l).unwrap(); v["id"] = json!(id); v }

pub fn commands() -> Vec<Command> { vec![
    Command { name: "set_light", description: "Create or update a project light (shared by every effect that reads lights, e.g. relight). kind directional|point|spot; pos x,y in 0..1 of the frame (y down), z in frame-heights above the layer; dir = direction the light travels (directional/spot); color r,g,b; intensity 0..1000; falloff 0..100 (0 = none, 1 = inverse square from one frame-height); cone 1..179 deg and softness 0..1 for spots; enabled; keys = [{t, pos?, intensity?}] smooth-interpolated over project time. Only the fields you pass change on update.", mutates: true,
        schema: obj(vec![("id", s("light id")), ("kind", s("directional|point|spot")), ("pos", json!({"type":"array","items":{"type":"number"},"description":"[x,y,z]"})), ("dir", json!({"type":"array","items":{"type":"number"},"description":"[dx,dy,dz]"})), ("color", json!({"type":"array","items":{"type":"number"},"description":"[r,g,b]"})), ("intensity", n("0..1000")), ("falloff", n("0..100")), ("cone", n("deg")), ("softness", n("0..1")), ("enabled", json!({"type":"boolean"})), ("keys", json!({"type":"array","items":{"type":"object"}}))], &["id"]),
        run: |ss, a| { let id = str_arg(a, "id")?.to_string(); if id.is_empty() || id.len() > 64 { return Err("id must be 1..64 chars".into()); }
            let base = ss.project.lights.get(&id).cloned().unwrap_or_else(|| serde_json::from_value(json!({})).unwrap());
            let l: LightDef = merge(&base, a, &["id"])?; lighting::validate_light(&l)?; if l.keys.len() > 256 { return Err("too many keys (max 256)".into()); }
            for k in &l.keys { if !k.t.is_finite() { return Err("key t must be finite".into()); } if let Some(p) = k.pos { if p.iter().any(|x| !x.is_finite()) { return Err("key pos must be finite".into()); } } if k.intensity.map_or(false, |i| !i.is_finite() || i < 0.0 || i > 1000.0) { return Err("key intensity must be in 0..1000".into()); } }
            if !ss.project.lights.contains_key(&id) && ss.project.lights.len() >= 16 { return Err("max 16 lights".into()); }
            ss.project.lights.insert(id.clone(), l.clone()); Ok(light_json(&id, &l)) } },
    Command { name: "remove_light", description: "Remove a project light.", mutates: true, schema: obj(vec![("id", s("light id"))], &["id"]),
        run: |ss, a| { let id = str_arg(a, "id")?; ss.project.lights.remove(id).ok_or(format!("no such light '{id}'"))?; Ok(json!({"removed": id})) } },
    Command { name: "list_lights", description: "List project lights with all parameters.", mutates: false, schema: obj(vec![], &[]),
        run: |ss, _| Ok(json!({"lights": ss.project.lights.iter().map(|(k, l)| light_json(k, l)).collect::<Vec<_>>()})) },
    Command { name: "set_material", description: "Define or update a named PBR material for relight/preview: albedo [r,g,b] (omit = use the layer colour), roughness 0..1, metal 0..1, ior 1..4, clearcoat 0..1, emissive [r,g,b]. Presets exist without defining: plastic, rubber, brushed_metal, chrome, gold, glass, concrete, wet_stone, car_paint, skin.", mutates: true,
        schema: obj(vec![("name", s("material name")), ("albedo", json!({"type":"array","items":{"type":"number"}})), ("roughness", n("0..1")), ("metal", n("0..1")), ("ior", n("1..4")), ("clearcoat", n("0..1")), ("emissive", json!({"type":"array","items":{"type":"number"}}))], &["name"]),
        run: |ss, a| { let name = str_arg(a, "name")?.to_string(); if name.is_empty() || name.len() > 64 { return Err("name must be 1..64 chars".into()); }
            let base = ss.project.materials.get(&name).cloned().or_else(|| lighting::lookup(&BTreeMap::new(), &name)).unwrap_or_default();
            let m: Material = merge(&base, a, &["name"])?; lighting::validate_material(&m)?; if !ss.project.materials.contains_key(&name) && ss.project.materials.len() >= 64 { return Err("max 64 materials".into()); }
            ss.project.materials.insert(name.clone(), m.clone()); let mut v = serde_json::to_value(&m).unwrap(); v["name"] = json!(name); Ok(v) } },
    Command { name: "list_materials", description: "List defined materials and the built-in presets.", mutates: false, schema: obj(vec![], &[]),
        run: |ss, _| Ok(json!({"defined": ss.project.materials, "presets": lighting::presets().into_iter().map(|(n, m)| json!({"name": n, "material": m})).collect::<Vec<_>>()})) },
    Command { name: "remove_material", description: "Remove a defined material.", mutates: true, schema: obj(vec![("name", s("material"))], &["name"]),
        run: |ss, a| { let n = str_arg(a, "name")?; ss.project.materials.remove(n).ok_or(format!("no such defined material '{n}'"))?; Ok(json!({"removed": n})) } },
    Command { name: "set_relight", description: "Light any layer (video, text, shape, sticker, precomp) with the project lights and a material. Builds a normal field from a height map (strings.height_source alpha|luma|both) and shades it with GGX specular, Smith geometry, Schlick fresnel, optional clearcoat and soft height-field self shadows. params: amount 0..1 mix with the original, bump 0..8 (edge slope), blur px 0..64 (height smoothing, sets feature scale), ambient 0..4, shadow 0..1, spec_gain 0..16, height 0..16 (relief used for light vectors and shadows). strings: material (defined name or preset, default plastic), lights (comma list, default all enabled). Updates the existing relight by effect_id or the clip's single relight.", mutates: true,
        schema: obj(vec![("clip_id", s("clip")), ("effect_id", s("existing relight id")), ("params", json!({"type":"object","description":"param name -> number"})), ("strings", json!({"type":"object","description":"material, lights, height_source"}))], &["clip_id"]),
        run: |ss, a| { if let Some(m) = a.get("strings").and_then(|v| v.as_object()) { for (k, v) in m { if !["material", "lights", "height_source"].contains(&k.as_str()) { return Err(format!("unknown string '{k}'")); } if !v.is_string() { return Err("strings values must be strings".into()); } } }
            if let Some(m) = a.get("params").and_then(|v| v.as_object()) { for (k, v) in m { if !RELIGHT.iter().any(|(n, _)| n == k) { return Err(format!("unknown param '{k}'")); } if !v.as_f64().map_or(false, |x| x.is_finite()) { return Err(format!("param '{k}' must be a finite number")); } } }
            if let Some(m) = a.get("strings").and_then(|v| v.as_object()) { if let Some(h) = m.get("height_source").and_then(|v| v.as_str()) { if !["alpha", "luma", "both"].contains(&h) { return Err(format!("height_source must be alpha|luma|both, got '{h}'")); } }
                if let Some(mn) = m.get("material").and_then(|v| v.as_str()) { lighting::lookup(&ss.project.materials, mn).ok_or(format!("unknown material '{mn}'"))?; }
                if let Some(ls) = m.get("lights").and_then(|v| v.as_str()) { for n in ls.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()) { if !ss.project.lights.contains_key(n) { return Err(format!("unknown light '{n}'")); } } } }
            let eid = a.get("effect_id").and_then(|v| v.as_str()).map(String::from); let c = find_clip(ss, a)?;
            let mut params = BTreeMap::new(); for (n, d) in RELIGHT { let v = a.get("params").and_then(|p| p.get(*n)).and_then(|v| v.as_f64()).unwrap_or(*d); params.insert(n.to_string(), crate::keyframe::Prop::new(v)); }
            let mut strings = BTreeMap::new(); if let Some(o) = a.get("strings").and_then(|v| v.as_object()) { for (k, v) in o { strings.insert(k.clone(), v.as_str().unwrap().to_string()); } }
            let idx = match &eid { Some(id) => Some(c.effects.iter().position(|e| &e.id == id && e.kind == "relight").ok_or(format!("no relight '{id}'"))?), None => c.effects.iter().position(|e| e.kind == "relight") };
            let id = match idx { Some(i) => { c.effects[i].params = params; c.effects[i].strings = strings; c.effects[i].id.clone() } None => { let id = "relight".to_string(); c.effects.push(Effect { id: id.clone(), kind: "relight".into(), params, strings }); id } };
            Ok(json!({"effect_id": id, "kind": "relight"})) } },
    Command { name: "preview_material", description: "Render a lit sphere with the project lights and a material to a PNG, to judge lights and materials without a layer. Returns the path and mean/max luminance.", mutates: false,
        schema: obj(vec![("path", s("output .png")), ("material", s("material name or preset")), ("size", n("px 64..1024, default 384")), ("color", json!({"type":"array","items":{"type":"number"},"description":"base colour when the material has no albedo"}))], &["path", "material"]),
        run: |ss, a| { let path = str_arg(a, "path")?; let size = opt_num(a, "size", 384.0); if !size.is_finite() || !(64.0..=1024.0).contains(&size) { return Err("size must be 64..1024".into()); }
            let col = a.get("color").and_then(|v| v.as_array()).map(|v| v.iter().map(|x| x.as_f64().unwrap_or(0.8)).collect::<Vec<_>>()).filter(|v| v.len() == 3).map(|v| [v[0], v[1], v[2]]).unwrap_or([0.8; 3]);
            let scene = lighting::build_scene(&ss.project.lights, &ss.project.materials, 0.0); let f = lighting::preview(&scene, str_arg(a, "material")?, size as usize, col)?;
            let (mut sum, mut mx, mut cnt) = (0f64, 0f64, 0f64); for i in 0..f.w * f.h { if f.d[i * 4 + 3] > 0.0 { let l = (0.2126 * f.d[i * 4] + 0.7152 * f.d[i * 4 + 1] + 0.0722 * f.d[i * 4 + 2]) as f64; sum += l; mx = mx.max(l); cnt += 1.0; } }
            crate::render::write_image(&f, path)?; Ok(json!({"path": path, "mean_luma": sum / cnt.max(1.0), "max_luma": mx, "lights": scene.lights.len()})) } },
] }
