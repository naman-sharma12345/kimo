//! style_captions_batch (v7.5): apply one style to every caption GROUP in the project, or a named list of
//! them, in one call. set_caption_style already applies a style to every CUE inside one caption group (its
//! own description: "Apply caption preset to every cue"); a project can hold more than one caption group --
//! one add_captions/add_caption_cues call per group, so two speakers, two tracks, or an SRT import plus a
//! caption_from_words pass each make their own group -- and restyling all of them today is one
//! set_caption_style call per group. This reuses set_caption_style itself (dispatch_nested, one call per
//! named group) rather than reimplementing the per-cue styling math a second time.
use super::kit::*;
use crate::commands::dispatch_nested as call;

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let style = str_arg(a, "style")?.to_string();
    let explicit = matches!(a.get("caption_ids"), Some(v) if !v.is_null());
    let ids: Vec<String> = if explicit {
        a.get("caption_ids").and_then(|v| v.as_array()).ok_or_else(|| crate::errors::arg_err(a, "caption_ids", "array of caption group id strings"))?
            .iter().map(|v| v.as_str().map(String::from).ok_or_else(|| crate::errors::arg_err(a, "caption_ids", "array of caption group id strings")))
            .collect::<Result<_, _>>()?
    } else {
        ss.project.editorial.captions.keys().cloned().collect()
    };

    let mut args = json!({"style": style});
    if let Some(v) = a.get("size") { args["size"] = v.clone(); }
    if let Some(v) = a.get("y") { args["y"] = v.clone(); }

    let mut styled: Vec<String> = vec![];
    for cid in &ids {
        if !ss.project.editorial.captions.contains_key(cid) { return Err(format!("no caption group '{cid}'")); }
        args["caption_id"] = json!(cid);
        call(ss, "set_caption_style", &args)?;
        styled.push(cid.clone());
    }

    Ok(json!({"styled": styled, "count": styled.len(), "style": style}))
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "style_captions_batch",
        description: "Apply one style to every caption group in the project, or a named list of them, in one call. set_caption_style already restyles every cue inside ONE group; a project can hold several groups (one per add_captions/add_caption_cues call -- different speakers, different tracks, an import plus a caption_from_words pass), and this covers all of them (or exactly the ones named) without one set_caption_style call per group. caption_ids omitted means every caption group in the project; an explicit empty array styles nothing. style/size/y are passed straight through to set_caption_style, so the same subtitle|boxed|yellow presets and overrides apply.",
        mutates: true,
        schema: obj(vec![
            ("caption_ids", json!({"type":"array","items":{"type":"string"},"description":"caption group ids to restyle; omit for every group in the project"})),
            ("style", s("subtitle|boxed|yellow")),
            ("size", n("px, optional override passed through to set_caption_style")),
            ("y", n("px, optional override passed through to set_caption_style")),
        ], &["style"]),
        run,
    }]
}
