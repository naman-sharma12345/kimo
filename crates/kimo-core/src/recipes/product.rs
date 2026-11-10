//! product_reveal: background, a product image that scales in, a title and a tagline.
use super::*;
pub fn product_reveal() -> Recipe { Recipe { name: "product_reveal", version: 1, summary: "Product image revealed over a background with a title and tagline; eased scale and fade.", keywords: &["product", "reveal", "showcase", "hero", "launch", "image"], params: params, roles: &["bg", "product", "title", "tagline"], expand } }
fn params() -> Vec<P> { vec![
    P { name: "asset_id", kind: Kind::Asset, required: true, default: None, desc: "imported image asset id (import_media)", space: None },
    text("title", 80, false, "product name"), text("tagline", 160, false, "one line under the title"),
    num("duration", 0.5, 60.0, 4.0, "seconds on screen"), num("reveal", 0.1, 5.0, 0.8, "reveal length, seconds"), en("easing", EASES, "out_back", "easing of the product scale"),
    num("product_scale", 0.05, 4.0, 1.0, "final product scale"), px("product_y", -2000.0, 2000.0, -40.0, "product vertical offset, px"), px("title_size", 8.0, 300.0, 72.0, "title size, px"), px("tagline_size", 8.0, 200.0, 34.0, "tagline size, px"),
    color("bg_color", json!([0.08, 0.08, 0.1]), "background colour"), color("color", json!([1, 1, 1]), "title colour"),
] }
fn expand(c: &Ctx) -> Result<Vec<Op>, String> {
    let (d, r, ease) = (c.num("duration"), c.num("reveal").min(c.num("duration")), c.text("easing")); let mut ops = vec![];
    ops.push(op("bg", "add_shape", json!({"track_id": c.track, "shape": "rect", "start": c.start, "duration": d, "width": c.w, "height": c.h, "fill": c.col("bg_color")})));
    ops.push(op("product", "add_clip", json!({"track_id": c.track, "source_type": "media", "asset_id": c.text("asset_id"), "start": c.start, "duration": d}))); ops.push(sety("product", c.num("product_y")));
    ops.push(kf("product_s0", "product", "scale", 0.0, 0.5 * c.num("product_scale"), "linear")); ops.push(kf("product_s1", "product", "scale", r, c.num("product_scale"), &ease)); ops.push(kf("product_o0", "product", "opacity", 0.0, 0.0, "linear")); ops.push(kf("product_o1", "product", "opacity", r * 0.6, 1.0, "out"));
    let (ty, mut ids) = (c.h * 0.36, vec![]);
    if c.has("title") { ops.push(op("title", "add_text", json!({"track_id": c.track, "text": c.text("title"), "start": c.start, "duration": d, "size": c.num("title_size"), "color": c.col("color")}))); ops.push(sety("title", ty)); ids.push("title"); }
    if c.has("tagline") { ops.push(op("tagline", "add_text", json!({"track_id": c.track, "text": c.text("tagline"), "start": c.start, "duration": d, "size": c.num("tagline_size"), "color": c.col("color")}))); ops.push(sety("tagline", ty + c.num("title_size") * 0.9)); ids.push("tagline"); }
    for id in ids { ops.push(kf(&format!("{id}_o0"), id, "opacity", r * 0.5, 0.0, "linear")); ops.push(kf(&format!("{id}_o1"), id, "opacity", r * 1.2, 1.0, "out")); }
    Ok(ops)
}
