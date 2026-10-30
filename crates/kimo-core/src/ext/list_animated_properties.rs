//! v7.2 additions: list_animated_properties. get_curve/move_key/remove_key (now effect-param aware,
//! this same pass) all need the property name up front; nothing lists what IS animated on a clip --
//! an agent otherwise has to guess property names or pull the whole get_project JSON (which the
//! engine's own docs point callers away from for exactly this). Read-only discovery pass.
use super::kit::*;
use crate::commands::Session;
use crate::keyframe::Prop;
use crate::model::Clip;

fn animated(c: &Clip) -> Vec<Value> {
    let mut v: Vec<Value> = c.props.iter().filter(|(_, p): &(&String, &Prop)| !p.keys.is_empty())
        .map(|(name, p)| json!({"scope": "clip", "effect_id": null, "property": name, "base": p.base, "key_count": p.keys.len(), "expressions": p.expr.len()})).collect();
    for e in &c.effects {
        v.extend(e.params.iter().filter(|(_, p)| !p.keys.is_empty())
            .map(|(name, p)| json!({"scope": "effect", "effect_id": e.id, "effect_kind": e.kind, "property": name, "base": p.base, "key_count": p.keys.len(), "expressions": p.expr.len()})));
    }
    v
}

fn list_animated_properties(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let id = str_arg(a, "clip_id")?;
    let c = ss.project.find_clip(id).ok_or(format!("no clip '{id}'"))?;
    let items = animated(c);
    Ok(json!({"clip_id": id, "count": items.len(), "animated": items, "note": "properties/effect params with at least one keyframe; read the curve with get_curve (pass effect_id for an effect param)"}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "list_animated_properties", description: "List every transform property and effect param on a clip that carries at least one keyframe. Discovery pass before get_curve/move_key/remove_key, which all need the property name (and effect_id for an effect param) up front. Each entry: scope (clip or effect), effect_id/effect_kind when scope is effect, property name, base value, key_count, and how many expression ops (wiggle, loopOut, ...) ride it. A clip with nothing keyframed returns an empty list, not an error. Read-only.", mutates: false,
        schema: obj(vec![("clip_id", s("clip"))], &["clip_id"]), run: list_animated_properties },
] }
