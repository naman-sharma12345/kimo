//! text_to_shapes: turns a text clip into vector path shapes, one per glyph (or one for the whole text), placed exactly
//! where the text renders. Each glyph is then an ordinary path shape: every shape modifier works on it (trim for
//! draw-on, offset, repeater, round corners, wiggle via the path operators), and each letter can be animated on its own.
//! `draw_on` builds the handwriting / stroke-reveal look in the same call: each glyph's outline is traced in turn (trim
//! end 0 -> 1), then its fill fades in.
use super::kit::*;
use crate::keyframe::{Ease, Prop};
use crate::model::{Effect, Source};

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let cid = str_arg(a, "clip_id")?.to_string();
    let c = ss.project.find_clip(&cid).ok_or_else(|| format!("no such clip '{cid}'"))?.clone();
    let Source::Text { text, font, size, color, letter_spacing, stroke_width, stroke_color, align, .. } = &c.source else { return Err(format!("{cid} is not a text clip")) };
    let (w, h) = (ss.project.width as f64, ss.project.height as f64);
    let fx = c.text_fx.clone();
    let weight = fx.as_ref().map_or(0.0, |f| f.weight);
    let text = crate::render::dyn_text(text, c.start, ss.project.fps);
    let glyphs = crate::text::glyph_paths(&text, font, *size, *letter_spacing, weight, align, w, h, fx.as_ref())?;
    if glyphs.is_empty() { return Err("the text has no visible glyphs to convert".into()); }
    let per = a.get("per").and_then(|v| v.as_str()).unwrap_or("glyph");
    if !matches!(per, "glyph" | "whole") { return Err("per must be glyph or whole".into()); }
    let draw_on = opt_num(a, "draw_on", 0.0).max(0.0);
    let stagger = opt_num(a, "stagger", if draw_on > 0.0 { (draw_on * 0.35).min(0.12) } else { 0.0 }).max(0.0);
    let keep_text = a.get("keep_text").and_then(|v| v.as_bool()).unwrap_or(false);
    // the outline drawn while tracing: the text's own stroke if it has one, else a thin line in the text colour
    let (sw, sc) = if *stroke_width > 0.0 { (*stroke_width, *stroke_color) } else if draw_on > 0.0 { ((size * 0.035).max(1.5), *color) } else { (0.0, *stroke_color) };
    let units: Vec<(String, String)> = if per == "whole" { vec![(text.clone(), glyphs.iter().map(|g| g.d.as_str()).collect::<Vec<_>>().join(""))] } else { glyphs.iter().map(|g| (g.ch.to_string(), g.d.clone())).collect() };
    // a track of its own, directly above the text's track, so the glyphs sit where the text was in the stack
    let ti = ss.project.tracks.iter().position(|t| t.clips.iter().any(|x| x.id == cid)).ok_or("clip has no track")?;
    let tid = ss.project.gen_id("track");
    ss.project.tracks.insert(ti + 1, crate::model::Track { id: tid.clone(), clips: vec![], locked: false, muted: false, solo: false, name: "Text shapes".into() });
    let mut ids = vec![];
    for (k, (label, d)) in units.iter().enumerate() {
        let r = crate::commands::dispatch_nested(ss, "add_path_shape", &json!({"track_id": tid, "start": c.start, "duration": c.duration, "d": d, "fill": color, "stroke_width": sw, "stroke_color": sc}))?;
        let id = r["clip_id"].as_str().ok_or("add_path_shape returned no clip id")?.to_string();
        let shape = ss.project.find_clip_mut(&id).ok_or("shape vanished")?;
        // the text's own transform and timing: same position, scale, rotation, opacity (keyframes included) and blend
        for key in ["x", "y", "scale", "rotation", "opacity", "anchor_x", "anchor_y"] { if let Some(p) = c.props.get(key) { shape.props.insert(key.into(), p.clone()); } }
        shape.blend = c.blend.clone(); shape.transition_in = c.transition_in.clone(); shape.parent = c.parent.clone();
        let _ = label;
        if draw_on > 0.0 {
            let t0 = k as f64 * stagger; let t1 = t0 + draw_on;
            let mut end = Prop::new(1.0); end.set_key(0.0, 0.0, Ease::Hold); if t0 > 0.0 { end.set_key(t0, 0.0, Ease::InOut); } else { end.keys[0].ease = Ease::InOut; } end.set_key(t1, 1.0, Ease::Hold);
            let mut params = std::collections::BTreeMap::new(); params.insert("start".into(), Prop::new(0.0)); params.insert("end".into(), end); params.insert("offset".into(), Prop::new(0.0));
            shape.effects.push(Effect { id: format!("trim_{k}"), kind: "shape_trim".into(), params, strings: Default::default() });
            // the fill arrives once the letter is traced
            let mut fa = Prop::new(1.0); fa.set_key(0.0, 0.0, Ease::Hold); fa.set_key(t1 - 0.05f64.min(draw_on * 0.2), 0.0, Ease::Out); fa.set_key(t1 + 0.3, 1.0, Ease::Hold);
            shape.props.insert("fill_alpha".into(), fa);
            if *stroke_width <= 0.0 { let mut sa = Prop::new(0.0); sa.set_key(0.0, 1.0, Ease::Hold); sa.set_key(t1, 1.0, Ease::Out); sa.set_key(t1 + 0.3, 0.0, Ease::Hold); shape.props.insert("stroke_alpha".into(), sa); }
        }
        ids.push(id);
    }
    if !keep_text { for t in ss.project.tracks.iter_mut() { t.clips.retain(|x| x.id != cid); } }
    Ok(json!({"track_id": tid, "clip_ids": ids, "glyphs": units.len(), "text_removed": !keep_text,
        "note": if draw_on > 0.0 { "each glyph traces its outline (shape_trim end 0->1), then fills in" } else { "each glyph is a path shape: add_shape_modifier / add_path_operator work on them one by one" }}))
}

pub fn commands() -> Vec<Command> {
    vec![Command { name: "text_to_shapes", description: "Turn a text clip into vector path shapes, one per letter (or one for the whole text), exactly where the text renders, keeping its colour, outline, transform and keyframes. Each letter can then be animated on its own and every shape modifier works on it (trim paths, offset, repeater, round corners). draw_on: seconds to trace each letter's outline before it fills (a handwriting / stroke reveal), letters staggered by `stagger`. The text clip is replaced unless keep_text.", mutates: true,
        schema: obj(vec![("clip_id", s("text clip")), ("per", json!({"type":"string","enum":["glyph","whole"],"description":"one shape per letter (default) or one for the whole text"})),
            ("draw_on", n("seconds to trace each letter's outline, then fill it (0 = no draw-on, default)")), ("stagger", n("seconds between letters starting their draw-on (default 35% of draw_on, max 0.12)")),
            ("keep_text", json!({"type":"boolean","description":"keep the original text clip (default false: it is replaced by the shapes)"}))], &["clip_id"]),
        run }]
}
