//! add_depth_parallax: the "3D photo" camera move (After Effects / Premiere 2.5D parallax, the Ken Burns move with depth) on a 2D
//! clip from a depth map clip. Near parts (white in the map) move and grow more than far parts (black) as the camera pans
//! (pan_x, pan_y), dollies (dolly) and zooms; focal_depth is the depth that stays still. Near objects hide what is behind them
//! and the background they uncover is filled from the background beside it (layerfx.rs depth_parallax).
//! It is a clip effect of kind depth_parallax, so set_effect_param, set_keyframe (pan_x, pan_y, dolly, zoom, focal_depth),
//! set_effect_mask and remove_effect work on it; a second call updates the same effect. The map clip is hidden.
use super::kit::*;
use crate::keyframe::Prop;
use crate::model::*;

const ARGS: [&str; 10] = ["clip_id", "depth_clip_id", "pan_x", "pan_y", "dolly", "zoom", "focal_depth", "invert", "depth_channel", "remove"];

pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_depth_parallax", description: "3D photo camera move on a 2D clip from a depth map clip: near parts move/grow more than far (pan, dolly, zoom; keyframable).", mutates: true,
        schema: obj(vec![("clip_id", s("clip to move the camera on")),
            ("depth_clip_id", s("depth map: another clip read per pixel, white = near, black = far (the map clip is hidden)")),
            ("pan_x", n("camera pan: a point one unit of depth from focal_depth moves this fraction of the width, -0.5..0.5, default 0")),
            ("pan_y", n("vertical pan, fraction of the height, -0.5..0.5, default 0")),
            ("dolly", n("push in: a point at depth d scales by 1 + dolly * (d - focal_depth), -0.9..0.9, default 0")),
            ("zoom", n("whole-picture scale about the centre (overscan that hides the frame edge on a pan), 1..3, default 1")),
            ("focal_depth", n("depth that stays still, 0..1, default 0.5")),
            ("invert", json!({"type":"boolean","description":"invert the depth map (white = far), default false"})),
            ("depth_channel", json!({"type":"string","enum":crate::layerfx::CHANNELS,"description":"channel of the map read as depth, default luma"})),
            ("remove", json!({"type":"boolean","description":"true removes the clip's depth parallax"}))], &["clip_id"]),
        run: |ss, a| {
            for k in a.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default() { if !ARGS.contains(&k.as_str()) { return Err(format!("add_depth_parallax: unknown argument '{k}' (valid: {})", ARGS.join(", "))); } }
            let cid = a.get("clip_id").and_then(|v| v.as_str()).ok_or("add_depth_parallax needs clip_id")?.to_string();
            if ss.project.find_clip(&cid).is_none() { return Err(format!("no such clip {cid}")); }
            if a.get("remove").and_then(|v| v.as_bool()).unwrap_or(false) {
                let c = ss.project.find_clip_mut(&cid).unwrap(); let n0 = c.effects.len(); c.effects.retain(|e| e.kind != "depth_parallax");
                return Ok(json!({"removed": n0 - c.effects.len()})); }
            let num = |k: &str, d: f64, lo: f64, hi: f64| -> Result<f64, String> { match a.get(k) { None => Ok(d), Some(v) if v.is_null() => Ok(d),
                Some(v) => { let x = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{k} must be a number ({lo}..{hi})"))?; if !(lo..=hi).contains(&x) { Err(format!("{k} must be between {lo} and {hi}, got {x}")) } else { Ok(x) } } } };
            let m = match a.get("depth_clip_id").filter(|v| !v.is_null()) { None => return Err("add_depth_parallax needs depth_clip_id (the clip read as the depth map: white near, black far)".into()), Some(v) => v.as_str().ok_or("depth_clip_id must be a clip id")?.to_string() };
            if m == cid { return Err(format!("depth_clip_id: a clip cannot be its own depth map ({m} is the moved clip itself)")); }
            if ss.project.find_clip(&m).is_none() { return Err(format!("depth_clip_id: no such clip {m}")); }
            let (pan_x, pan_y, dolly, zoom, focal) = (num("pan_x", 0.0, -0.5, 0.5)?, num("pan_y", 0.0, -0.5, 0.5)?, num("dolly", 0.0, -0.9, 0.9)?, num("zoom", 1.0, 1.0, 3.0)?, num("focal_depth", 0.5, 0.0, 1.0)?);
            let inv = match a.get("invert") { None | Some(Value::Null) => false, Some(v) => v.as_bool().ok_or("invert must be true or false")? };
            let ch = match a.get("depth_channel") { None | Some(Value::Null) => "luma".to_string(), Some(v) => { let c = v.as_str().unwrap_or(""); if !crate::layerfx::CHANNELS.contains(&c) { return Err(format!("depth_channel must be one of {}, got {v}", crate::layerfx::CHANNELS.join(", "))); } c.to_string() } };
            let vals = [("pan_x", pan_x), ("pan_y", pan_y), ("dolly", dolly), ("zoom", zoom), ("focal_depth", focal), ("invert", if inv { 1.0 } else { 0.0 })];
            let params: std::collections::BTreeMap<String, Prop> = vals.iter().map(|(k, v)| (k.to_string(), Prop::new(*v))).collect();
            let strings: std::collections::BTreeMap<String, String> = [("depth_clip".to_string(), m.clone()), ("depth_channel".to_string(), ch.clone())].into_iter().collect();
            let eid = { let existing = ss.project.find_clip(&cid).unwrap().effects.iter().find(|e| e.kind == "depth_parallax").map(|e| e.id.clone()); existing.unwrap_or_else(|| ss.project.gen_id("fx")) };
            let c = ss.project.find_clip_mut(&cid).unwrap();
            let replaced = if let Some(e) = c.effects.iter_mut().find(|e| e.id == eid) { e.params = params; e.strings = strings; true } else { c.effects.push(Effect { id: eid.clone(), kind: "depth_parallax".into(), params, strings }); false };
            Ok(json!({"effect_id": eid, "updated": replaced, "depth": {"clip_id": m, "focal_depth": focal, "invert": inv, "channel": ch}, "pan": [pan_x, pan_y], "dolly": dolly, "zoom": zoom})) } },
] }
