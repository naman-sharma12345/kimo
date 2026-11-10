//! explainer_diagram: a row of labelled boxes joined by connectors, revealed one after another.
use super::*;
pub fn explainer_diagram() -> Recipe { Recipe { name: "explainer_diagram", version: 1, summary: "Left-to-right diagram: 2 to 6 labelled boxes with connector bars, staggered eased reveal.", keywords: &["diagram", "flow", "steps", "process", "explainer", "boxes", "pipeline"], params: params, roles: &["box1..boxN", "label1..labelN", "link1..linkN-1"], expand } }
fn params() -> Vec<P> { vec![
    P { name: "labels", kind: Kind::TextList { max_items: 6, max: 40 }, required: true, default: None, desc: "one label per box, 1 to 6", space: None },
    num("duration", 1.0, 120.0, 6.0, "seconds on screen"), num("stagger", 0.0, 3.0, 0.5, "delay between boxes, seconds"), en("easing", EASES, "out", "easing of each reveal"),
    auto("box_width", 40.0, 1200.0, "box width, px; default fits the canvas (max 260)"), auto("box_height", 20.0, 800.0, "box height, px; default 20% of canvas height"), auto("spacing", 0.0, 800.0, "gap between boxes, px; default 6% of canvas width"), px("y", -2000.0, 2000.0, 0.0, "vertical centre, px"),
    auto("text_size", 8.0, 200.0, "label size, px; default 40% of box height"), px("corner_radius", 0.0, 200.0, 8.0, "box corner radius, px"), color("box_color", json!([0.16, 0.2, 0.32]), "box fill"), color("link_color", json!([1.0, 0.55, 0.1]), "connector colour"), color("color", json!([1, 1, 1]), "label colour"),
] }
fn expand(c: &Ctx) -> Result<Vec<Op>, String> {
    let labels: Vec<String> = c.p.get("labels").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(); let n = labels.len();
    let opt = |k: &str, d: f64| c.p.get(k).and_then(|v| v.as_f64()).unwrap_or(d); let sp = opt("spacing", c.w * 0.06); let bw = opt("box_width", ((c.w * 0.86 - (n as f64 - 1.0) * sp) / n as f64).min(260.0).max(40.0)); let bh = opt("box_height", c.h * 0.2); let (y, d, ease) = (c.num("y"), c.num("duration"), c.text("easing")); let ts = opt("text_size", (bh * 0.4).max(8.0));
    let total = n as f64 * bw + (n as f64 - 1.0) * sp; if total > c.w * 1.5 { return Err(format!("invalid_value: param 'box_width' too wide: {n} boxes need {total:.0} px but the canvas is {:.0} px; lower box_width or spacing", c.w)); }
    let x0 = -total / 2.0 + bw / 2.0; let mut ops = vec![]; let st = c.num("stagger"); let rev = 0.35;
    for (i, l) in labels.iter().enumerate() {
        let (b, t) = (format!("box{}", i + 1), format!("label{}", i + 1)); let x = x0 + i as f64 * (bw + sp); let t0 = i as f64 * st; let life = (d - t0).max(0.2);
        ops.push(op(&b, "add_shape", json!({"track_id": c.track, "shape": "rect", "start": c.start + t0, "duration": life, "width": bw, "height": bh, "fill": c.col("box_color"), "corner_radius": c.num("corner_radius")}))); ops.push(setx(&b, x)); ops.push(sety(&b, y));
        ops.push(op(&t, "add_text", json!({"track_id": c.track, "text": esc(l), "start": c.start + t0, "duration": life, "size": ts, "color": c.col("color")}))); ops.push(setx(&t, x)); ops.push(sety(&t, y));
        for id in [b.as_str(), t.as_str()] { ops.push(kf(&format!("{id}_o0"), id, "opacity", 0.0, 0.0, "linear")); ops.push(kf(&format!("{id}_o1"), id, "opacity", rev, 1.0, &ease)); }
        if i > 0 { let l = format!("link{i}"); let lw = sp.max(2.0); ops.push(op(&l, "add_shape", json!({"track_id": c.track, "shape": "rect", "start": c.start + t0, "duration": life, "width": lw, "height": 6, "fill": c.col("link_color")}))); ops.push(setx(&l, x - bw / 2.0 - sp / 2.0)); ops.push(sety(&l, y)); ops.push(kf(&format!("{l}_o0"), &l, "opacity", 0.0, 0.0, "linear")); ops.push(kf(&format!("{l}_o1"), &l, "opacity", rev, 1.0, &ease)); }
    }
    Ok(ops)
}
