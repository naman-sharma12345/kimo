//! batch_set_meta (v7.6): apply a role, colour label and/or tags to many clips at once, in one call --
//! by explicit clip_ids, or by the same find-style filter `find` already takes (name_glob, role, color,
//! tag, track, source_kind, has_effect, time_range, text_contains). set_meta already does one clip per
//! call; tagging "every clip with the glow effect" or "everything on the b-roll track" today is one
//! set_meta call per match, found by hand. The filter lives under its own `filter` object (not spread at
//! the top level) so filtering by role/color is never confused with the role/color being SET -- the same
//! field name would otherwise mean two different things depending on how a clip was selected. Literal
//! `name` is deliberately NOT exposed as a value to set: set_meta requires names to be unique among
//! clips, so a single batch value can never apply to more than one clip -- naming stays a per-clip
//! set_meta call. Reuses `find` and `set_meta` themselves (dispatch_nested) rather than reimplementing
//! either clip matching or the metadata write.
use super::kit::*;
use crate::commands::dispatch_nested as call;

const FILTER_KEYS: &[&str] = &["name", "name_glob", "role", "color", "tag", "track", "source_kind", "has_effect", "time_range", "text_contains"];

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let has_meta_change = ["role", "color", "tags", "add_tags", "remove_tags"].iter().any(|k| a.get(*k).is_some());
    if !has_meta_change { return Err("nothing to apply: give at least one of role, color, tags, add_tags, remove_tags".into()); }

    let explicit_ids = matches!(a.get("clip_ids"), Some(v) if !v.is_null());
    let has_filter = matches!(a.get("filter"), Some(Value::Object(m)) if !m.is_empty());
    if explicit_ids && has_filter { return Err("give clip_ids or filter, not both".into()); }
    if !explicit_ids && !has_filter { return Err("nothing to select: give clip_ids or a non-empty filter".into()); }

    let ids: Vec<String> = if explicit_ids {
        a.get("clip_ids").and_then(|v| v.as_array()).ok_or_else(|| crate::errors::arg_err(a, "clip_ids", "array of clip id strings"))?
            .iter().map(|v| v.as_str().map(String::from).ok_or_else(|| crate::errors::arg_err(a, "clip_ids", "array of clip id strings")))
            .collect::<Result<_, _>>()?
    } else {
        let given = a.get("filter").unwrap();
        for k in given.as_object().unwrap().keys() {
            if !FILTER_KEYS.contains(&k.as_str()) { return Err(format!("filter.{k} is not a recognised find filter field (name, name_glob, role, color, tag, track, source_kind, has_effect, time_range, text_contains)")); }
        }
        let mut filter = json!({"kind": "clip", "limit": 500});
        for k in FILTER_KEYS { if let Some(v) = given.get(*k) { filter[*k] = v.clone(); } }
        let found = call(ss, "find", &filter)?;
        if found["truncated"].as_bool().unwrap_or(false) { return Err(format!("the filter matched {} clips, more than the 500 batch_set_meta can see in one call -- narrow it (e.g. add track) and call again", found["total"])); }
        found["matches"].as_array().cloned().unwrap_or_default().iter().filter_map(|m| m["id"].as_str().map(String::from)).collect()
    };

    let mut args = json!({"target": "clip"});
    for k in ["role", "color", "tags", "add_tags", "remove_tags"] { if let Some(v) = a.get(k) { args[k] = v.clone(); } }

    let mut updated: Vec<String> = vec![];
    for cid in &ids {
        args["id"] = json!(cid);
        call(ss, "set_meta", &args)?;
        updated.push(cid.clone());
    }

    Ok(json!({"updated": updated, "count": updated.len()}))
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "batch_set_meta",
        description: "Apply a role, colour label and/or tags to many clips at once, in one call. Select the clips either by clip_ids (an explicit list; an empty array matches nothing) or by filter, an object with the same fields find already takes (name, name_glob, role, color, tag, track, source_kind, has_effect, time_range, text_contains) -- give one of the two, not both. At least one of role/color/tags/add_tags/remove_tags (top-level, not inside filter) is required and is passed straight through to set_meta for every selected clip -- e.g. filter: {has_effect: \"glow\"}, color: \"red\" colours every glow clip red, independent of what filter.color (if any) was used to find them. Literal name is not supported as a value to set: set_meta requires a name to be unique among clips, so one batch value could never legally apply to more than one clip -- naming stays its own per-clip set_meta call. A filter matching more than 500 clips (find's own cap) errors asking you to narrow it, rather than silently only touching the first 500. Nothing is changed if nothing matches.",
        mutates: true,
        schema: obj(vec![
            ("clip_ids", json!({"type":"array","items":{"type":"string"},"description":"explicit clip ids to update; alternative to filter"})),
            ("filter", json!({"type":"object","description":"find-style selector, alternative to clip_ids: name, name_glob, role, color, tag, track, source_kind, has_effect, time_range, text_contains","properties":{
                "name": {"type":"string"}, "name_glob": {"type":"string"}, "role": {"type":"string"}, "color": {"type":"string"}, "tag": {"type":"string"}, "track": {"type":"string"}, "source_kind": {"type":"string"}, "has_effect": {"type":"string"}, "time_range": {"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2}, "text_contains": {"type":"string"}
            }})),
            ("role", json!({"type":["string","null"],"description":"role to set on every selected clip (set_meta semantics, null clears)"})),
            ("color", json!({"type":["string","null"],"enum":["red","orange","yellow","green","cyan","blue","purple","pink","gray",null],"description":"colour label to set on every selected clip (set_meta semantics, null clears)"})),
            ("tags", json!({"type":"array","items":{"type":"string"},"description":"replaces each selected clip's whole tag list (set_meta semantics)"})),
            ("add_tags", json!({"type":"array","items":{"type":"string"}})),
            ("remove_tags", json!({"type":"array","items":{"type":"string"}})),
        ], &[]),
        run,
    }]
}
