//! title_card: full-frame background, title and optional subtitle that fade in and out.
use super::*;
pub fn title_card() -> Recipe { Recipe { name: "title_card", version: 1, summary: "Full-frame title card: solid background, big title, optional subtitle, eased fade in and out.", keywords: &["title", "card", "intro", "heading", "headline", "opening", "text"], params: params, roles: &["bg", "title", "subtitle"], expand } }
fn params() -> Vec<P> { vec![
    text("title", 120, true, "title text"), text("subtitle", 200, false, "optional second line"),
    num("duration", 0.5, 60.0, 3.0, "seconds on screen"), num("fade", 0.0, 5.0, 0.4, "fade in and out length, seconds"),
    en("easing", EASES, "out", "easing of the fade in; the fade out uses 'in'"),
    px("title_size", 8.0, 400.0, 96.0, "title font size, px"), px("subtitle_size", 8.0, 300.0, 40.0, "subtitle font size, px"), px("gap", 0.0, 400.0, 70.0, "distance of the subtitle below the title, px"),
    color("bg_color", json!([0.05, 0.06, 0.09]), "background colour [r,g,b] 0..1"), color("color", json!([1, 1, 1]), "title colour"), color("subtitle_color", json!([0.75, 0.78, 0.85]), "subtitle colour"), ] }
fn expand(c: &Ctx) -> Result<Vec<Op>, String> {
    let (d, f) = (c.num("duration"), c.num("fade").min(c.num("duration") / 2.0)); let ease = c.text("easing"); let mut ops = vec![];
    ops.push(op("bg", "add_shape", json!({"track_id": c.track, "shape": "rect", "start": c.start, "duration": d, "width": c.w, "height": c.h, "fill": c.col("bg_color")})));
    let ty = if c.has("subtitle") { -c.num("gap") / 2.0 } else { 0.0 };
    ops.push(op("title", "add_text", json!({"track_id": c.track, "text": c.text("title"), "start": c.start, "duration": d, "size": c.num("title_size"), "color": c.col("color") })));
    if ty != 0.0 { ops.push(sety("title", ty)); }
    let mut fades = vec!["title"];
    if c.has("subtitle") { ops.push(op("subtitle", "add_text", json!({"track_id": c.track, "text": c.text("subtitle"), "start": c.start, "duration": d, "size": c.num("subtitle_size"), "color": c.col("subtitle_color") }))); ops.push(sety("subtitle", c.num("gap") / 2.0 + c.num("title_size") / 2.0)); fades.push("subtitle"); }
    if f > 0.0 { for id in fades { for (i, (t, v, e)) in [(0.0, 0.0, "linear"), (f, 1.0, ease.as_str()), (d - f, 1.0, "linear"), (d, 0.0, "in")].iter().enumerate() { ops.push(kf(&format!("{id}_o{i}"), id, "opacity", *t, *v, e)); } } }
    Ok(ops)
}
