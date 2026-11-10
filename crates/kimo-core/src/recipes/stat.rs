//! stat_counter: a big number or percentage with a label and an accent bar. The value is text, so it pops in (scale + opacity); it does not count up.
use super::*;
pub fn stat_counter() -> Recipe { Recipe { name: "stat_counter", version: 1, summary: "Big statistic with a label and an accent bar that grows under it; the value pops in with an eased scale.", keywords: &["stat", "number", "statistic", "counter", "metric", "percentage", "kpi", "figure"], params: params, roles: &["value", "label", "bar"], expand } }
fn params() -> Vec<P> { vec![
    text("value", 24, true, "the statistic as text, e.g. '42%' or '1.2M'"), text("label", 120, false, "caption under the value"),
    num("duration", 0.5, 60.0, 3.0, "seconds on screen"), num("pop", 0.1, 3.0, 0.5, "pop-in length, seconds"), en("easing", EASES, "out_back", "easing of the pop-in"),
    px("value_size", 16.0, 600.0, 160.0, "value font size, px"), px("label_size", 8.0, 200.0, 40.0, "label font size, px"), px("bar_width", 20.0, 2000.0, 360.0, "accent bar width, px"), px("bar_height", 2.0, 60.0, 10.0, "accent bar height, px"),
    px("y", -2000.0, 2000.0, 0.0, "vertical centre of the whole block, px"), color("color", json!([1, 1, 1]), "value colour"), color("accent", json!([1.0, 0.55, 0.1]), "accent bar colour"), color("label_color", json!([0.8, 0.82, 0.88]), "label colour"),
] }
fn expand(c: &Ctx) -> Result<Vec<Op>, String> {
    let (d, pop, y) = (c.num("duration"), c.num("pop").min(c.num("duration")), c.num("y")); let ease = c.text("easing"); let vs = c.num("value_size"); let mut ops = vec![];
    ops.push(op("value", "add_text", json!({"track_id": c.track, "text": c.text("value"), "start": c.start, "duration": d, "size": vs, "color": c.col("color")}))); ops.push(sety("value", y - vs * 0.15));
    let by = y + vs * 0.45; ops.push(op("bar", "add_shape", json!({"track_id": c.track, "shape": "rect", "start": c.start, "duration": d, "width": c.num("bar_width"), "height": c.num("bar_height"), "fill": c.col("accent")}))); ops.push(sety("bar", by));
    let mut ids = vec!["value", "bar"];
    if c.has("label") { ops.push(op("label", "add_text", json!({"track_id": c.track, "text": c.text("label"), "start": c.start, "duration": d, "size": c.num("label_size"), "color": c.col("label_color")}))); ops.push(sety("label", by + c.num("label_size") * 1.2)); ids.push("label"); }
    for id in &ids { ops.push(kf(&format!("{id}_o0"), id, "opacity", 0.0, 0.0, "linear")); ops.push(kf(&format!("{id}_o1"), id, "opacity", pop, 1.0, "out")); }
    ops.push(kf("value_s0", "value", "scale", 0.0, 0.6, "linear")); ops.push(kf("value_s1", "value", "scale", pop, 1.0, &ease));
    ops.push(kf("bar_s0", "bar", "scale", 0.0, 0.0, "linear")); ops.push(kf("bar_s1", "bar", "scale", pop * 1.4, 1.0, "out_expo"));
    Ok(ops)
}
