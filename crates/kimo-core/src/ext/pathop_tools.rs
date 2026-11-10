//! add_path_operator (AE shape path operators): Wiggle Paths, Zig Zag, Pucker & Bloat, Twist and Round Corners on a Shape clip.
//! Each is an effect of kind shape_op_<op> (pathops.rs); params are keyframable with set_keyframe on the effect id, and
//! remove_shape_modifier removes one. Operators compose in order with the shape modifiers (trim, repeater, offset, merge).
use super::kit::*;
use crate::keyframe::Prop;
use crate::model::*;

pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_path_operator", description: "AE path operators on a Shape clip: wiggle, zig_zag, pucker_bloat, twist, round_corners. Params keyframable; effect_id updates one.", mutates: true,
        schema: obj(vec![("clip_id", s("Shape clip")),
            ("op", json!({"type":"string","enum":["wiggle","zig_zag","pucker_bloat","twist","round_corners"]})),
            ("params", json!({"type":"object","description":"wiggle {size px, detail, speed /s, seed}; zig_zag {size px, ridges, smooth 0/1}; pucker_bloat {amount -1..1}; twist {angle deg}; round_corners {radius px}"})),
            ("effect_id", s("existing operator id to update"))], &["clip_id","op"]),
        run: |ss, a| {
            for k in a.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default() { if !["clip_id", "op", "params", "effect_id"].contains(&k.as_str()) { return Err(format!("add_path_operator: unknown argument '{k}' (valid: clip_id, op, params, effect_id)")); } }
            let op = str_arg(a, "op")?.to_string(); let cid = str_arg(a, "clip_id")?.to_string();
            let (_, _, defs) = crate::pathops::OPS.iter().find(|o| o.0 == op).ok_or_else(|| format!("unknown op '{op}' (valid: {})", crate::pathops::OPS.iter().map(|o| o.0).collect::<Vec<_>>().join(", ")))?;
            let c = ss.project.find_clip(&cid).ok_or_else(|| format!("no such clip {cid}"))?;
            let Source::Shape { shape, .. } = &c.source else { return Err(format!("add_path_operator needs a Shape clip (see add_shape): '{cid}' is not one")); };
            if op == "round_corners" && !matches!(shape.as_str(), "rect" | "polygon") { return Err(format!("round_corners works on rect and polygon shapes; '{cid}' is a {shape} (an ellipse has no corners; on a path, curve the corner in the path itself)")); }
            let mut params = std::collections::BTreeMap::new();
            if let Some(p) = a.get("params") { let o = p.as_object().ok_or("params must be an object of numbers")?; for k in o.keys() { if !defs.iter().any(|(n, _)| n == k) { return Err(format!("unknown param '{k}' for {op} (valid: {})", defs.iter().map(|d| d.0).collect::<Vec<_>>().join(", "))); } } }
            for (n, d) in defs.iter() { let v = match a.get("params").and_then(|p| p.get(*n)) { None => *d, Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("param '{n}' must be a finite number"))? };
                let ok = match (op.as_str(), *n) { ("pucker_bloat", "amount") => (-1.0..=1.0).contains(&v), ("zig_zag", "ridges") => (1.0..=1000.0).contains(&v), ("wiggle", "detail") => (1.0..=400.0).contains(&v), (_, "radius") | (_, "size") => v >= 0.0, _ => true };
                if !ok { return Err(format!("{op} {n} out of range: {v}")); } params.insert(n.to_string(), Prop::new(v)); }
            let kind = format!("shape_op_{op}");
            let eid = a.get("effect_id").and_then(|v| v.as_str()).map(String::from);
            if let Some(id) = &eid { let e = c.effects.iter().find(|e| &e.id == id).ok_or_else(|| format!("no path operator '{id}' on {cid}"))?; if e.kind != kind { return Err(format!("effect {id} is a {}, not {kind}", e.kind)); } }
            let new_id = if eid.is_none() { Some(ss.project.gen_id("fx")) } else { None };
            let c = ss.project.find_clip_mut(&cid).unwrap();
            let id = match eid { Some(id) => { let e = c.effects.iter_mut().find(|e| e.id == id).unwrap(); e.params = params; id }
                None => { let id = new_id.unwrap(); c.effects.push(Effect { id: id.clone(), kind, params, strings: Default::default() }); id } };
            Ok(json!({"effect_id": id, "op": op})) } },
] }
