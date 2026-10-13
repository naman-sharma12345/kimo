//! V1c presentation primitives. `add_dim_mask` is the primitive: a full-canvas black layer at `dim_factor` opacity with an inverted, feathered rect
//! mask hole, built from existing tools only (add_track, add_clip, set_property, add_mask, set_keyframe, set_meta) and applied as ONE undo step through
//! `batch`. `spotlight` is a pure wrapper: it expands to one `add_dim_mask` call with every parameter explicit and echoes it; `apply:false` (default) returns
//! the expansion and changes nothing, `apply:true` runs exactly that call.
use super::kit::*;

const MAX_FEATHER: f64 = 256.0;
pub(crate) fn finite(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(d),
        Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("out_of_range: {k} must be a finite number (got {v}); fix_hint: pass a plain number")),
    }
}
pub(crate) fn range(a: &Value, k: &str, d: f64, lo: f64, hi: f64, hint: &str) -> Result<f64, String> {
    let v = finite(a, k, d)?;
    if v < lo || v > hi { return Err(format!("out_of_range: {k} must be between {lo} and {hi} (got {v}); fix_hint: {hint}")); }
    Ok(v)
}
/// Canvas-pixel box [x, y, w, h] from `target_box` or an axis-aligned shape clip.
fn target(ss: &Session, a: &Value) -> Result<[f64; 4], String> {
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64);
    if let Some(id) = a.get("shape_id").and_then(|v| v.as_str()) {
        if a.get("target_box").map_or(false, |v| !v.is_null()) { return Err("invalid_argument_combo: give target_box or shape_id, not both; fix_hint: drop one of them".into()); }
        let id = super::names::resolve_ref(&ss.project, "clip", id)?;
        let c = ss.project.find_clip(&id).ok_or_else(|| format!("no clip '{id}'"))?;
        let crate::model::Source::Shape { width, height, .. } = &c.source else { return Err(format!("invalid_source: '{id}' is not a shape clip; fix_hint: pass target_box [x,y,w,h] in canvas pixels instead")) };
        let q = crate::render::resolve_xform(&ss.project, c, 0.0);
        if q.rotation.abs() > 1e-9 { return Err(format!("invalid_source: shape '{id}' is rotated, its box is not axis aligned; fix_hint: pass target_box [x,y,w,h] in canvas pixels")); }
        let (w, h) = (width * q.scale, height * q.scale);
        return Ok([cw / 2.0 + q.x - w / 2.0, ch / 2.0 + q.y - h / 2.0, w, h]);
    }
    let arr = a.get("target_box").and_then(|v| v.as_array()).ok_or("missing array 'target_box'; fix_hint: pass target_box [x, y, w, h] in canvas pixels or shape_id")?;
    if arr.len() != 4 { return Err(format!("out_of_range: target_box must have 4 numbers [x, y, w, h] (got {}); fix_hint: pass [x, y, w, h]", arr.len())); }
    let mut b = [0.0; 4];
    for (i, v) in arr.iter().enumerate() { b[i] = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("out_of_range: target_box[{i}] must be a finite number (got {v}); fix_hint: pass plain numbers in canvas pixels"))?; }
    if b[2] <= 0.0 || b[3] <= 0.0 { return Err(format!("out_of_range: target_box w and h must be > 0 (got {} x {}); fix_hint: pass a box with positive size, e.g. [{}, {}, 64, 64]", b[2], b[3], b[0], b[1])); }
    Ok(b)
}
/// Every parameter resolved and echoed, plus the batch op list that builds the layer.
fn plan(ss: &Session, a: &Value) -> Result<(Value, Vec<Value>), String> {
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64);
    let tb = target(ss, a)?;
    let (x0, y0, x1, y1) = (tb[0].max(0.0), tb[1].max(0.0), (tb[0] + tb[2]).min(cw), (tb[1] + tb[3]).min(ch));
    if x1 <= x0 || y1 <= y0 { return Err(format!("out_of_range: target_box [{}, {}, {}, {}] lies outside the {cw} x {ch} canvas; fix_hint: move the box onto the canvas", tb[0], tb[1], tb[2], tb[3])); }
    let dim = range(a, "dim_factor", 0.6, 0.0, 1.0, "use 0 (no dim) to 1 (black)")?;
    let feather = range(a, "feather_px", 0.06 * cw.min(ch), 0.0, MAX_FEATHER, "feather_px is the 10-90% ramp width in canvas pixels (default 0.06*min(W,H)), capped at 256; lower it")?;
    let (bw, bh) = (x1 - x0, y1 - y0);
    let radius = range(a, "corner_radius", (0.5 * feather).min(bw.min(bh) / 2.0), 0.0, bw.min(bh) / 2.0, "corner_radius cannot exceed half the shorter side of the clipped box")?;
    let start = range(a, "start", 0.0, 0.0, 1e7, "start is timeline seconds, >= 0")?;
    let dur = range(a, "duration", 5.0, 1e-3, 1e7, "duration is seconds, > 0")?;
    let ein = range(a, "ease_in", 0.0, 0.0, dur, "ease_in is seconds, 0..duration")?;
    let eout = range(a, "ease_out", 0.0, 0.0, dur, "ease_out is seconds, 0..duration")?;
    if ein + eout > dur { return Err(format!("out_of_range: ease_in + ease_out must be <= duration (got {} > {dur}); fix_hint: shorten the eases", ein + eout)); }
    let track = match a.get("track_id") { None | Some(Value::Null) => None, Some(v) => Some(super::names::resolve_ref(&ss.project, "track", v.as_str().ok_or("argument 'track_id' must be a string")?)?) };
    let params = json!({"target_box":[tb[0], tb[1], tb[2], tb[3]], "clipped_box":[x0, y0, bw, bh], "dim_factor":dim, "feather_px":feather, "corner_radius":radius, "start":start, "duration":dur, "ease_in":ein, "ease_out":eout, "track_id":track,
        "layer":"solid dark blue-shifted tint (0.02,0.03,0.09), opacity = dim_factor, rect mask invert=true (hole), ramp = feather_px (10-90% width, smootherstep), mode add", "role":"dim_mask"});
    let mut ops = vec![];
    let tid = match &track { Some(t) => json!(t), None => { ops.push(json!({"id":"track","tool":"add_track","args":{}})); json!("$track.track_id") } };
    ops.push(json!({"id":"dim","tool":"add_clip","args":{"track_id":tid,"source_type":"solid","color":[0.02,0.03,0.09],"start":start,"duration":dur}}));
    ops.push(json!({"id":"role","tool":"set_meta","args":{"id":"$dim.clip_id","role":"dim_mask"}}));
    ops.push(json!({"id":"op","tool":"set_property","args":{"clip_id":"$dim.clip_id","property":"opacity","value":dim}}));
    let round = if bw.min(bh) > 0.0 { radius / (bw.min(bh) / 2.0) } else { 0.0 };
    ops.push(json!({"id":"mask","tool":"add_mask","args":{"clip_id":"$dim.clip_id","kind":"rect","mode":"add","invert":true,"feather":0.0,"params":{"ramp":feather,"cx":(x0 + bw / 2.0) / cw,"cy":(y0 + bh / 2.0) / ch,"w":bw / cw,"h":bh / ch,"round":round}}}));
    if ein > 0.0 || eout > 0.0 {
        let key = |t: f64, v: f64| json!({"tool":"set_keyframe","args":{"clip_id":"$dim.clip_id","property":"opacity","time":t,"value":v,"ease":"linear"}});
        if ein > 0.0 { ops.push(key(0.0, 0.0)); ops.push(key(ein, dim)); } else { ops.push(key(0.0, dim)); }
        if eout > 0.0 { ops.push(key(dur - eout, dim)); ops.push(key(dur, 0.0)); } else { ops.push(key(dur, dim)); }
    }
    Ok((params, ops))
}
fn apply_ops(ss: &mut Session, ops: &[Value]) -> Result<(String, String, Option<String>), String> {
    let r = crate::commands::dispatch_nested(ss, "batch", &json!({"ops":ops,"atomic":true,"return":"full"}))?;
    if r["ok"] != true { return Err(format!("batch_failed: {}", r["failed"])); }
    let res = r["results"].as_array().ok_or("batch returned no results")?;
    let find = |op: &str, k: &str| res.iter().find(|x| x["id"] == op).and_then(|x| x["result"][k].as_str()).map(String::from);
    Ok((find("dim", "clip_id").ok_or("dim layer not created")?, find("mask", "mask_id").ok_or("mask not created")?, find("track", "track_id")))
}
fn add_dim_mask(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let (params, ops) = plan(ss, a)?;
    let (clip, mask, track) = apply_ops(ss, &ops)?;
    Ok(json!({"clip_id":clip,"mask_id":mask,"track_id":track,"params":params,"expansion":ops,"note":"Animate the hole with set_mask_prop_key on mask_id (cx, cy, w, h, round are clip-local, normalized 0..1; ramp is the 10-90% feather width in canvas px)."}))
}
fn spotlight(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let apply = match a.get("apply") { None | Some(Value::Null) => false, Some(v) => v.as_bool().ok_or("argument 'apply' must be a boolean")? };
    let (params, _) = plan(ss, a)?;
    let args = json!({"target_box":params["clipped_box"],"dim_factor":params["dim_factor"],"feather_px":params["feather_px"],"corner_radius":params["corner_radius"],"start":params["start"],"duration":params["duration"],"ease_in":params["ease_in"],"ease_out":params["ease_out"],"track_id":params["track_id"]});
    let expansion = json!([{"tool":"add_dim_mask","args":args}]);
    if !apply { return Ok(json!({"applied":false,"no_history":true,"params":params,"expansion":expansion})); }
    let r = add_dim_mask(ss, &args)?;
    Ok(json!({"applied":true,"clip_id":r["clip_id"],"mask_id":r["mask_id"],"track_id":r["track_id"],"params":params,"expansion":expansion}))
}

// ---------------------------------------------------------------- V1c-C: fit_box_to_text + callout_card

fn text_rect(ss: &mut Session, a: Value) -> Result<[f64; 4], String> {
    let r = crate::commands::dispatch(ss, "compute_text_bounds", &a)?;
    let b = r["canvas_rect"].as_array().ok_or_else(|| format!("empty_text: the text has no ink to measure; fix_hint: pass text with at least one visible character"))?;
    let mut o = [0.0; 4];
    for i in 0..4 { o[i] = b[i].as_f64().ok_or("canvas_rect is not numeric")?; }
    Ok(o)
}
fn fit_box_to_text(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let id = a.get("clip_id").and_then(|v| v.as_str()).ok_or("missing string 'clip_id'; fix_hint: pass a text clip id or @name")?;
    let pad = range(a, "padding", 0.0, 0.0, 10000.0, "padding is canvas pixels, >= 0")?;
    let mut args = json!({"clip_id": id}); if let Some(t) = a.get("t") { args["t"] = t.clone(); }
    let r = text_rect(ss, args)?;
    let rect = [r[0] - pad, r[1] - pad, r[2] + 2.0 * pad, r[3] + 2.0 * pad];
    Ok(json!({"rect": rect, "ink_rect": r, "padding": pad, "note": "read-only, nothing is moved. ink_rect is compute_text_bounds canvas_rect (ink incl. stroke, with the clip transform); rect = ink_rect grown by padding on every side"}))
}
fn rgba(a: &Value, k: &str, d: Option<[f64; 4]>) -> Result<Option<[f64; 4]>, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(d),
        Some(v) => {
            let arr = v.as_array().filter(|x| x.len() == 3 || x.len() == 4).ok_or_else(|| format!("out_of_range: {k} must be [r,g,b] or [r,g,b,a] (got {v}); fix_hint: pass 3 or 4 numbers in 0..1"))?;
            let mut o = [0.0, 0.0, 0.0, 1.0];
            for (i, x) in arr.iter().enumerate() { o[i] = x.as_f64().filter(|f| f.is_finite() && *f >= 0.0 && *f <= 1.0).ok_or_else(|| format!("out_of_range: {k}[{i}] must be a number in 0..1 (got {x}); fix_hint: colour channels are 0..1"))?; }
            Ok(Some(o))
        }
    }
}
const ANCHORS: &[&str] = &["top_left", "top_center", "top_right", "center", "bottom_left", "bottom_center", "bottom_right"];
fn callout_card(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64);
    let apply = match a.get("apply") { None | Some(Value::Null) => false, Some(v) => v.as_bool().ok_or("argument 'apply' must be a boolean")? };
    let text = a.get("text").and_then(|v| v.as_str()).filter(|t| !t.trim().is_empty()).ok_or("missing string 'text'; fix_hint: pass the callout text, \\n for a new line")?.replace("\\n", "\n");
    let badge = match a.get("badge") { None | Some(Value::Null) => None, Some(v) => Some(v.as_str().filter(|t| !t.trim().is_empty()).ok_or("out_of_range: badge must be a non-empty string; fix_hint: omit badge or pass text")?.to_string()) };
    // style: explicit object, optionally seeded from an existing clip (style_from), then neutral documented defaults. Nothing else.
    let mut st = a.get("style").cloned().unwrap_or(json!({}));
    if !st.is_object() { return Err("out_of_range: style must be an object; fix_hint: pass {\"fill\":[r,g,b,a],\"padding\":16,...} or omit it".into()); }
    let mut from = json!({});
    if let Some(sf) = a.get("style_from").and_then(|v| v.as_str()) {
        let id = super::names::resolve_ref(&ss.project, "clip", sf)?;
        let c = ss.project.find_clip(&id).ok_or_else(|| format!("no clip '{id}'"))?;
        match &c.source {
            crate::model::Source::Shape { fill, corner_radius, .. } => { from["fill"] = json!(fill); from["corner_radius"] = json!(corner_radius); }
            crate::model::Source::Text { font, size, color, letter_spacing, .. } => { from["font"] = json!(font); from["size"] = json!(size); from["text_color"] = json!(color); from["letter_spacing"] = json!(letter_spacing); }
            _ => return Err(format!("invalid_source: style_from '{id}' is neither a shape nor a text clip; fix_hint: pass a shape or text clip, or give style explicitly")),
        }
    }
    let get = |k: &str| -> Value { st.get(k).filter(|v| !v.is_null()).cloned().or_else(|| from.get(k).cloned()).unwrap_or(Value::Null) };
    let g = |k: &str| { let mut o = json!({}); o[k] = get(k); o };
    let fill = rgba(&g("fill"), "fill", Some([0.08, 0.08, 0.10, 0.92]))?.unwrap();
    let text_color = rgba(&g("text_color"), "text_color", Some([1.0, 1.0, 1.0, 1.0]))?.unwrap();
    let badge_fill = rgba(&g("badge_fill"), "badge_fill", Some([0.78, 0.17, 0.20, 1.0]))?.unwrap();
    let badge_text_color = rgba(&g("badge_text_color"), "badge_text_color", Some([1.0, 1.0, 1.0, 1.0]))?.unwrap();
    let font = get("font").as_str().unwrap_or("Montserrat").to_string();
    let align = get("align").as_str().unwrap_or("left").to_string();
    if !["left", "center", "right"].contains(&align.as_str()) { return Err("out_of_range: style.align is not allowed. Valid: left, center, right; fix_hint: use one of those".into()); }
    let u1 = ch / 135.0; // one design unit; every default below is in u so it scales with the frame (8 px at 1080p)
    let size = range(&g("size"), "size", 4.5 * u1, 1.0, 2000.0, "size is pixels, 1..2000")?;
    let weight = range(&g("weight"), "weight", 500.0, 100.0, 900.0, "weight is 100..900 (variable fonts only)")?;
    let bweight = range(&g("badge_weight"), "badge_weight", 600.0, 100.0, 900.0, "badge_weight is 100..900 (variable fonts only)")?;
    let letter = range(&g("letter_spacing"), "letter_spacing", 0.0, -100.0, 1000.0, "letter_spacing is pixels")?;
    let padding = range(&g("padding"), "padding", 2.0 * u1, 0.0, 1000.0, "padding is pixels, >= 0 (sets both axes unless padding_x / padding_y is given)")?;
    let padding_y = range(&g("padding_y"), "padding_y", padding, 0.0, 1000.0, "padding_y is pixels from the ink to the card top and bottom, >= 0")?;
    let padding_x = range(&g("padding_x"), "padding_x", if get("padding").is_null() { 3.0 * u1 } else { padding }, 0.0, 1000.0, "padding_x is pixels from the ink to the card left and right, >= 0")?;
    let radius = range(&g("corner_radius"), "corner_radius", 1.5 * u1, 0.0, 10000.0, "corner_radius is pixels, >= 0")?;
    let margin = range(&g("margin"), "margin", 12.0 * u1, 0.0, 10000.0, "margin is pixels from the canvas edge for anchors")?;
    let max_card_w = range(&g("max_width"), "max_width", (cw - 2.0 * margin).max(1.0), 1.0, 100000.0, "max_width is the widest allowed card in pixels")?;
    let bsize = range(&g("badge_size"), "badge_size", 3.0 * u1, 1.0, 2000.0, "badge_size is pixels")?;
    let bpad = range(&g("badge_padding"), "badge_padding", 1.5 * u1, 0.0, 1000.0, "badge_padding is the horizontal pixels between the badge text and its pill edge")?;
    let bheight = range(&g("badge_height"), "badge_height", 3.5 * u1, 1.0, 2000.0, "badge_height is the pill height in pixels")?;
    let bgap = range(&g("badge_gap"), "badge_gap", 1.0 * u1, 0.0, 1000.0, "badge_gap is the pixel gap between badge and card")?;
    let sh_one = |v: &Value| -> Result<Value, String> { let o = v.as_object().ok_or("out_of_range: style.shadow must be an object {dx,dy,blur,opacity}, an array of them, false for none, or omitted for the two-layer default; fix_hint: pass one of those")?; let o = Value::Object(o.clone());
        Ok(json!({"dx": range(&o, "dx", 0.0, -1000.0, 1000.0, "dx is pixels")?, "dy": range(&o, "dy", 2.0 * u1, -1000.0, 1000.0, "dy is pixels")?, "blur": range(&o, "blur", 6.0 * u1, 0.0, 500.0, "blur is pixels")?, "opacity": range(&o, "opacity", 0.18, 0.0, 1.0, "opacity is 0..1")?})) };
    // default = two layers: ambient (0, 2u, blur 6u, a .18) and contact (0, .5u, blur 1u, a .25)
    let shadows: Vec<Value> = match get("shadow") { Value::Null => vec![json!({"dx":0.0,"dy":2.5*u1,"blur":9.0*u1,"opacity":0.19}), json!({"dx":0.0,"dy":0.5*u1,"blur":1.0*u1,"opacity":0.32})], Value::Bool(false) => vec![], Value::Array(v) => v.iter().map(|x| sh_one(x)).collect::<Result<Vec<_>, _>>()?, v => vec![sh_one(&v)?] };
    let start = range(a, "start", 0.0, 0.0, 1e7, "start is timeline seconds, >= 0")?;
    let dur = range(a, "duration", 5.0, 1e-3, 1e7, "duration is seconds, > 0")?;
    // measure the text exactly as the renderer lays it out (read-only), centred on the canvas at x=y=0
    let (font_path, font_subst) = crate::text::resolve_font_info(&font); let weight_unavailable = font_subst || !font_path.to_lowercase().contains("var"); // weights 500/600 need a variable font; otherwise report it, never silently regular
    let measure = |ss: &mut Session, t: &str, sz: f64, ls: f64, al: &str, wt: f64| text_rect(ss, json!({"text": t, "size": sz, "font": font, "weight": wt, "letter_spacing": ls, "align": al, "x": 0, "y": 0}));
    let ink = measure(ss, &text, size, letter, &align, weight)?;
    let (card_w, card_h) = (ink[2] + 2.0 * padding_x, ink[3] + 2.0 * padding_y);
    if card_w > max_card_w + 1e-9 { return Err(format!("text_too_wide: the card would be {card_w:.1}px wide but style.max_width is {max_card_w:.1}px; the text is never clipped or auto-wrapped; fix_hint: add \\n line breaks, lower style.size, or raise style.max_width")); }
    // The card, its badge and its shadow extent are ONE unit: position, near:, anchors, the box fit and the safe-zone check all use the unit bounds.
    let badge_pos = match a.get("badge_pos") { None | Some(Value::Null) => "above_left".to_string(), Some(v) => { let p = v.as_str().unwrap_or(""); if !["above_left", "above_right", "inside_top_left", "none"].contains(&p) { return Err("out_of_range: badge_pos is not allowed. Valid: above_left, above_right, inside_top_left, none; fix_hint: use one of those".into()); } p.to_string() } };
    let badge = if badge_pos == "none" { None } else { badge };
    let safe_profile = match a.get("safe_profile") { None | Some(Value::Null) => "title_safe".to_string(), Some(v) => v.as_str().filter(|p| !p.is_empty()).ok_or("out_of_range: safe_profile must be a profile name from query_safe_margins or \"none\"; fix_hint: use title_safe, action_safe or none")?.to_string() };
    let mut binfo = None; let (mut bw, mut bh) = (0.0, 0.0);
    if let Some(bt) = &badge { let bi = measure(ss, bt, bsize, 0.0, "center", bweight)?; bw = bi[2] + 2.0 * bpad; bh = bheight; binfo = Some(bi); }
    let inside = badge.is_some() && badge_pos == "inside_top_left";
    let text_dy = if inside { bh + bgap } else { 0.0 };
    let card_w = if inside { card_w.max(bw + 2.0 * padding_x) } else { card_w };
    let (card_w, card_h) = (card_w, card_h + text_dy);
    // everything below is relative to the card top-left (0,0)
    let badge_rel: Option<[f64; 4]> = badge.as_ref().map(|_| match badge_pos.as_str() { "above_right" => [card_w - bw, -bgap - bh, bw, bh], "inside_top_left" => [padding_x, padding_y, bw, bh], _ => [0.0, -bgap - bh, bw, bh] });
    let shadow_rel: Option<[f64; 4]> = shadows.iter().map(|sh| { let (dx, dy, bl) = (sh["dx"].as_f64().unwrap_or(0.0), sh["dy"].as_f64().unwrap_or(0.0), sh["blur"].as_f64().unwrap_or(0.0)); [dx - bl, dy - bl, card_w + 2.0 * bl, card_h + 2.0 * bl] }).reduce(|a, b| { let (x0, y0) = (a[0].min(b[0]), a[1].min(b[1])); [x0, y0, (a[0] + a[2]).max(b[0] + b[2]) - x0, (a[1] + a[3]).max(b[1] + b[3]) - y0] });
    let mut u = [0.0, 0.0, card_w, card_h];
    for r in [badge_rel, shadow_rel].iter().flatten() { let (x1, y1) = ((u[0] + u[2]).max(r[0] + r[2]), (u[1] + u[3]).max(r[1] + r[3])); u[0] = u[0].min(r[0]); u[1] = u[1].min(r[1]); u[2] = x1 - u[0]; u[3] = y1 - u[1]; }
    let (uw, uh) = (u[2], u[3]);
    // safe zones: the unit must sit inside every safe zone of the chosen profile and out of every avoid zone; "none" checks the canvas only
    let zones: Vec<(String, String, [f64; 4])> = if safe_profile == "none" { vec![("canvas".into(), "safe".into(), [0.0, 0.0, cw, ch])] } else {
        let m = crate::commands::dispatch_nested(ss, "query_safe_margins", &json!({"profile": safe_profile}))?;
        m["zones"].as_array().ok_or("query_safe_margins returned no zones")?.iter().map(|z| { let v = z["rect"].as_array().map(|v| v.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect::<Vec<_>>()).unwrap_or_default(); (z["name"].as_str().unwrap_or("zone").to_string(), z["kind"].as_str().unwrap_or("safe").to_string(), [v[0], v[1], v[2], v[3]]) }).collect() };
    // overflow per edge [left, top, right, bottom] in px (>0 = outside) against the canvas and each safe zone, plus avoid-zone hits
    let overflow = |b: [f64; 4]| -> ([f64; 4], Option<String>) {
        let mut o = [(0.0 - b[0]).max(0.0), (0.0 - b[1]).max(0.0), (b[0] + b[2] - cw).max(0.0), (b[1] + b[3] - ch).max(0.0)]; let mut hit = None;
        for (name, kind, z) in &zones { if kind == "safe" { o[0] = o[0].max(z[0] - b[0]); o[1] = o[1].max(z[1] - b[1]); o[2] = o[2].max(b[0] + b[2] - (z[0] + z[2])); o[3] = o[3].max(b[1] + b[3] - (z[1] + z[3])); } else if b[0] < z[0] + z[2] && z[0] < b[0] + b[2] && b[1] < z[1] + z[3] && z[1] < b[1] + b[3] { hit = Some(name.clone()); } }
        (o, hit) };
    let unfit = |b: [f64; 4], why: &str| -> String { let (o, hit) = overflow(b); format!("callout_unfit: the card + badge + shadow unit [{:.1}, {:.1}, {:.1}, {:.1}] does not fit ({why}) inside profile '{safe_profile}' on the {cw} x {ch} canvas; overflow_px {{left: {:.1}, top: {:.1}, right: {:.1}, bottom: {:.1}}}{}; fix_hint: move it with position or offset, lower style.size or style.margin, shorten the text, or drop the badge (badge_pos none)", b[0], b[1], b[2], b[3], o[0], o[1], o[2], o[3], hit.map(|h| format!(", overlaps avoid zone '{h}'")).unwrap_or_default()) };
    let fits = |b: [f64; 4]| { let (o, hit) = overflow(b); hit.is_none() && o.iter().all(|x| *x <= 1e-9) };
    let hit_rect = |p: [f64; 4], q: [f64; 4]| p[0] < q[0] + q[2] - 1e-9 && q[0] < p[0] + p[2] - 1e-9 && p[1] < q[1] + q[3] - 1e-9 && q[1] < p[1] + p[3] - 1e-9;
    // position: a box [x,y,w,h] (unit is centred in it and must fit), an anchor name (margin from the edge), or near:<clip_id>
    let pos = a.get("position").cloned().unwrap_or(json!("bottom_center"));
    let (px0, py0, pos_note, mut placement, mut tried);
    placement = "anchor".to_string(); tried = vec![];
    if let Some(arr) = pos.as_array() {
        if arr.len() != 4 { return Err(format!("out_of_range: position box must be [x,y,w,h] (got {} numbers); fix_hint: pass 4 numbers or an anchor name", arr.len())); }
        let mut b = [0.0; 4]; for i in 0..4 { b[i] = arr[i].as_f64().filter(|f| f.is_finite()).ok_or_else(|| format!("out_of_range: position[{i}] must be a finite number; fix_hint: pass plain pixel numbers"))?; }
        if b[2] <= 0.0 || b[3] <= 0.0 { return Err("out_of_range: position box w and h must be > 0; fix_hint: pass a box with positive size".into()); }
        if uw > b[2] + 1e-9 || uh > b[3] + 1e-9 { return Err(format!("text_too_wide: the card + badge + shadow unit is {uw:.1} x {uh:.1}px but the position box is {} x {}; fix_hint: enlarge the box, lower style.size, add \\n, or drop the badge", b[2], b[3])); }
        px0 = b[0] + (b[2] - uw) / 2.0 - u[0]; py0 = b[1] + (b[3] - uh) / 2.0 - u[1]; pos_note = "box: the card + badge + shadow unit is centred in the box"; placement = "box".into();
    } else if let Some(name) = pos.as_str() {
        if let Some(near) = name.strip_prefix("near:") {
            let id = super::names::resolve_ref(&ss.project, "clip", near)?;
            let is_text = matches!(ss.project.find_clip(&id).map(|c| &c.source), Some(crate::model::Source::Text { .. }));
            let r = if is_text { text_rect(ss, json!({"clip_id": id}))? } else { target(ss, &json!({"shape_id": id}))? };
            let pair = |k: &str| -> Result<Option<[f64; 2]>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => { let o = v.as_array().filter(|x| x.len() == 2).ok_or_else(|| format!("out_of_range: {k} must be [dx,dy]; fix_hint: pass two pixel numbers"))?; Ok(Some([o[0].as_f64().filter(|f| f.is_finite()).ok_or_else(|| format!("out_of_range: {k} must be finite; fix_hint: pass plain pixel numbers"))?, o[1].as_f64().filter(|f| f.is_finite()).ok_or_else(|| format!("out_of_range: {k} must be finite; fix_hint: pass plain pixel numbers"))?])) } } };
            let (off_u, off_c) = (pair("offset")?, pair("card_offset")?);
            if off_u.is_some() && off_c.is_some() { return Err("out_of_range: offset and card_offset are mutually exclusive; fix_hint: pass offset (moves the card+badge+shadow unit) or card_offset (moves the card only, the unit is still validated)".into()); }
            let explicit = off_u.or(off_c); let by_card = off_c.is_some();
            let ub = |x0: f64, y0: f64| [x0, y0, uw, uh];
            if let Some(off) = explicit {
                // explicit offset: the UNIT top-left = target left + dx, target bottom + dy; validated, never retried elsewhere
                let b = if by_card { ub(r[0] + off[0] + u[0], r[1] + r[3] + off[1] + u[1]) } else { ub(r[0] + off[0], r[1] + r[3] + off[1]) };
                if hit_rect(b, r) {
                    let cand = [[0.0, r[1] + r[3] - b[1]], [0.0, r[1] - uh - b[1]], [r[0] + r[2] - b[0], 0.0], [r[0] - uw - b[0], 0.0]];
                    let m = cand.iter().min_by(|p, q| (p[0].abs() + p[1].abs()).partial_cmp(&(q[0].abs() + q[1].abs())).unwrap()).unwrap();
                    return Err(format!("callout_overlap: the unit [{:.1}, {:.1}, {:.1}, {:.1}] at offset [{}, {}] intersects the target ink rect [{:.1}, {:.1}, {:.1}, {:.1}]; fix_hint: use the min_fixing_offset [{:.1}, {:.1}]; min_fixing_offset: [{:.1}, {:.1}]", b[0], b[1], b[2], b[3], off[0], off[1], r[0], r[1], r[2], r[3], off[0] + m[0], off[1] + m[1], off[0] + m[0], off[1] + m[1]));
                }
                if !fits(b) { return Err(unfit(b, "explicit offset")); }
                px0 = b[0] - u[0]; py0 = b[1] - u[1]; pos_note = "near: explicit offset moves the unit (unit top-left = target left + dx, target bottom + dy; card_offset moves the card instead), then it is re-validated"; placement = "offset".into();
            } else {
                let gap = padding; // default gap between the target and the unit's nearest ink = the card padding, so a badge never crowds the target
                let c = [("below", ub(r[0], r[1] + r[3] + gap)), ("above", ub(r[0], r[1] - gap - uh)), ("right", ub(r[0] + r[2] + gap, r[1])), ("left", ub(r[0] - gap - uw, r[1]))];
                let mut win = None;
                for (nm, b) in c.iter() { let ok = fits(*b) && !hit_rect(*b, r); tried.push(json!({"side": nm, "unit": b, "ok": ok})); if ok && win.is_none() { win = Some((*nm, *b)); } }
                let (nm, b) = win.ok_or_else(|| unfit(c[0].1, "none of below, above, right, left is inside the safe area and clear of the target"))?;
                px0 = b[0] - u[0]; py0 = b[1] - u[1]; pos_note = "near: the unit tries below, above, right, left of the target ink rect (gap = style.padding) and takes the first one that is inside the safe area and does not touch the target"; placement = nm.into();
            }
        } else if ANCHORS.contains(&name) {
            let ins = match a.get("offset") { None | Some(Value::Null) => None, Some(v) => { let o = v.as_array().filter(|x| x.len() == 2).ok_or("out_of_range: offset must be [dx,dy]; fix_hint: pass two pixel numbers")?; Some([o[0].as_f64().filter(|f| f.is_finite()).ok_or("out_of_range: offset must be finite")?, o[1].as_f64().filter(|f| f.is_finite()).ok_or("out_of_range: offset must be finite")?]) } };
            if ins.is_some() && a.get("card_offset").map(|v| !v.is_null()).unwrap_or(false) { return Err("out_of_range: offset and card_offset are mutually exclusive; fix_hint: pass one of them".into()); }
            let (ix, iy) = match ins { Some(o) => (Some(o[0]), Some(o[1])), None => (None, None) };
            let ux = if name.ends_with("left") { ix.unwrap_or(margin) } else if name.ends_with("right") { cw - ix.unwrap_or(margin) - uw } else { (cw - uw) / 2.0 + ix.unwrap_or(0.0) };
            let uy = if name.starts_with("top") { iy.unwrap_or(margin) } else if name.starts_with("bottom") { ch - iy.unwrap_or(margin) - uh } else { (ch - uh) / 2.0 + iy.unwrap_or(0.0) };
            px0 = ux - u[0]; py0 = uy - u[1];
            pos_note = "anchor: the card + badge + shadow unit edge sits `margin` px from the canvas edge, centred on the other axis";
        } else { return Err(format!("out_of_range: position '{name}' is not allowed. Valid: {}, near:<clip_id>, or a box [x,y,w,h]; fix_hint: use one of those", ANCHORS.join(", "))); }
    } else { return Err("out_of_range: position must be an anchor name, near:<clip_id> or a box [x,y,w,h]; fix_hint: pass one of those".into()); }
    let card = [px0, py0, card_w, card_h];
    let unit_bounds = [px0 + u[0], py0 + u[1], uw, uh];
    if !fits(unit_bounds) { return Err(unfit(unit_bounds, "placement")); }
    let badge_rect = badge_rel.map(|r| [px0 + r[0], py0 + r[1], r[2], r[3]]);
    let shadow_rect = shadow_rel.map(|r| [px0 + r[0], py0 + r[1], r[2], r[3]]);
    let text_top = py0 + padding_y + text_dy;
    let n_existing = ss.project.tracks.iter().flat_map(|t| t.clips.iter()).filter(|c| c.tags.iter().any(|t| t.starts_with("callout:"))).count();
    let cid = match a.get("callout_id") { None | Some(Value::Null) => format!("c{}", n_existing + 1), Some(v) => { let s = v.as_str().filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')).ok_or("out_of_range: callout_id must be 1+ characters of A-Z a-z 0-9 _ -; fix_hint: pass a short id like intro")?; s.to_string() } };
    let tag = format!("callout:{cid}");
    // expansion
    let mut ops: Vec<Value> = vec![];
    let tracks = if badge.is_some() { 4 } else { 2 };
    for i in 0..tracks { ops.push(json!({"id": format!("t{i}"), "tool": "add_track", "args": {}})); }
    let shape = |id: &str, track: &str, r: [f64; 4], f: [f64; 4], rad: f64, ops: &mut Vec<Value>| {
        ops.push(json!({"id": id, "tool": "add_shape", "args": {"track_id": format!("$t{track}.track_id"), "shape": "rect", "start": start, "duration": dur, "width": r[2], "height": r[3], "fill": [f[0], f[1], f[2]], "corner_radius": rad}}));
        ops.push(json!({"tool": "set_property", "args": {"clip_id": format!("${id}.clip_id"), "property": "x", "value": r[0] + r[2] / 2.0 - cw / 2.0}}));
        ops.push(json!({"tool": "set_property", "args": {"clip_id": format!("${id}.clip_id"), "property": "y", "value": r[1] + r[3] / 2.0 - ch / 2.0}}));
        if f[3] < 1.0 { ops.push(json!({"tool": "set_property", "args": {"clip_id": format!("${id}.clip_id"), "property": "fill_alpha", "value": f[3]}})); }
    };
    let txt = |id: &str, track: &str, t: &str, wt: f64, sz: f64, ls: f64, al: &str, col: [f64; 4], ink_now: [f64; 4], want_left_top: [f64; 2], ops: &mut Vec<Value>, font: &str| {
        // measured at x=y=0 the ink top-left is ink_now; shift so it lands on want_left_top
        ops.push(json!({"id": id, "tool": "add_text", "args": {"track_id": format!("$t{track}.track_id"), "text": t, "start": start, "duration": dur, "font": font, "size": sz, "color": [col[0], col[1], col[2]], "letter_spacing": ls, "align": al}}));
        ops.push(json!({"tool": "set_text_weight", "args": {"clip_id": format!("${id}.clip_id"), "weight": wt}}));
        ops.push(json!({"tool": "set_property", "args": {"clip_id": format!("${id}.clip_id"), "property": "x", "value": want_left_top[0] - ink_now[0]}}));
        ops.push(json!({"tool": "set_property", "args": {"clip_id": format!("${id}.clip_id"), "property": "y", "value": want_left_top[1] - ink_now[1]}}));
    };
    shape("card", "0", card, fill, radius, &mut ops);
    for sh in &shadows { ops.push(json!({"tool": "add_effect", "args": {"clip_id": "$card.clip_id", "kind": "drop_shadow", "params": sh}})); }
    txt("text", "1", &text, weight, size, letter, &align, text_color, ink, [px0 + padding_x, text_top], &mut ops, &font);
    if let (Some(bt), Some(br), Some(bi)) = (&badge, badge_rect, binfo) {
        shape("badge", "2", br, badge_fill, br[3] / 2.0, &mut ops);
        ops.push(json!({"tool": "add_effect", "args": {"clip_id": "$badge.clip_id", "kind": "drop_shadow", "params": {"dx": 0.0, "dy": u1, "blur": 2.0 * u1, "opacity": 0.35}}}));
        txt("badge_text", "3", bt, bweight, bsize, 0.0, "center", badge_text_color, bi, [br[0] + bpad, br[1] + (br[3] - bi[3]) / 2.0], &mut ops, &font);
    }
    // inner top highlight: a thin 10% white line on the card's top edge, above everything
    let ht = tracks; ops.push(json!({"id": format!("t{ht}"), "tool": "add_track", "args": {}}));
    // highlights fade out at both ends through a rect mask whose smootherstep ramp grows outward from the inner span (no hard cut)
    let fade = |ops: &mut Vec<Value>, id: &str, wf: f64, f: f64| { ops.push(json!({"tool": "add_mask", "args": {"clip_id": format!("${id}.clip_id"), "kind": "rect", "mode": "add", "invert": false, "feather": 0.0, "params": {"cx": 0.5, "cy": 0.5, "w": ((wf - 2.0 * f) / wf).clamp(0.05, 1.0), "h": 8.0, "round": 0.0, "ramp": (0.5071 * f).max(0.5)}}})); };
    let (hw, hf) = ((card_w - 0.8 * radius).max(2.0), 0.5 * radius);
    shape("highlight", &ht.to_string(), [px0 + 0.4 * radius, py0, hw, (u1 / 6.0).max(1.0)], [1.0, 1.0, 1.0, 0.16], 0.0, &mut ops); fade(&mut ops, "highlight", hw, hf);
    if let Some(br) = badge_rect { let bw2 = (br[2] - 1.4 * br[3]).max(2.0); shape("badge_hl", &ht.to_string(), [br[0] + 0.7 * br[3], br[1], bw2, (u1 / 6.0).max(1.0)], [1.0, 1.0, 1.0, 0.20], 0.0, &mut ops); fade(&mut ops, "badge_hl", bw2, 0.35 * br[3]); }
    let ids: Vec<&str> = if badge.is_some() { vec!["card", "text", "badge", "badge_text", "highlight", "badge_hl"] } else { vec!["card", "text", "highlight"] };
    for id in &ids { ops.push(json!({"tool": "set_meta", "args": {"id": format!("${id}.clip_id"), "role": "callout", "tags": [tag.clone()]}})); }
    let text_ink = [px0 + padding_x, text_top, ink[2], ink[3]];
    let style = json!({"fill": fill, "text_color": text_color, "font": font, "size": size, "letter_spacing": letter, "align": align, "padding": padding, "padding_x": padding_x, "padding_y": padding_y, "corner_radius": radius, "margin": margin, "max_width": max_card_w, "shadow": shadows,
        "badge_fill": badge_fill, "badge_text_color": badge_text_color, "badge_size": bsize, "badge_padding": bpad, "badge_height": bheight, "badge_gap": bgap, "unit_px": u1, "weight": weight, "badge_weight": bweight});
    let params = json!({"text": text, "badge": badge, "position": pos, "position_rule": pos_note, "style": style, "style_from": a.get("style_from"), "callout_id": cid, "tag": tag, "font_resolved": font_path, "font_weight_unavailable": weight_unavailable, "role": "callout", "start": start, "duration": dur,
        "card_rect": card, "text_rect": text_ink, "badge_rect": badge_rect, "shadow_rect": shadow_rect, "unit_bounds": unit_bounds, "applied_offset": a.get("offset").or(a.get("card_offset")).cloned().unwrap_or(Value::Null), "offset_kind": if a.get("card_offset").map(|v| !v.is_null()).unwrap_or(false) { "card_offset" } else { "offset" }, "badge_pos": badge_pos, "safe_profile": safe_profile, "placement": placement, "tried": tried, "default_gap": padding,
        "layout": "tracks bottom to top: card, text, badge, badge text (later tracks draw on top); card_rect = measured text ink rect grown by style.padding; the card, badge and shadow extent are one unit (unit_bounds) that must sit inside the safe profile (callout_unfit otherwise); the badge sits per badge_pos (default above the card, left aligned), never over the text; text is never clipped or auto-wrapped (text_too_wide error)"});
    if !apply { return Ok(json!({"applied": false, "no_history": true, "params": params, "expansion": ops})); }
    let r = crate::commands::dispatch_nested(ss, "batch", &json!({"ops": ops, "atomic": true, "return": "full"}))?;
    if r["ok"] != true { return Err(format!("batch_failed: {}", r["failed"])); }
    let res = r["results"].as_array().ok_or("batch returned no results")?;
    let clip = |op: &str| res.iter().find(|x| x["id"] == op).and_then(|x| x["result"]["clip_id"].as_str()).map(String::from);
    Ok(json!({"applied": true, "card_clip_id": clip("card"), "text_clip_id": clip("text"), "badge_clip_id": clip("badge"), "badge_text_clip_id": clip("badge_text"), "params": params, "expansion": ops}))
}
pub fn commands() -> Vec<Command> {
    let ease = |d: &str| n(d);
    vec![
        Command{name:"add_dim_mask",mutates:true,description:"Dim everything except a box: adds a full-canvas dark blue-shifted tint layer (0.02,0.03,0.09) at dim_factor opacity with an inverted rect hole over target_box [x,y,w,h] canvas px or an axis-aligned shape_id (box clipped to the canvas). Built from add_track/add_clip/set_property/add_mask/set_keyframe/set_meta and applied as one undo step; returns clip_id, mask_id and the exact op list. feather_px is the 10-90% ramp width in canvas px (smootherstep, grows outward from the hole so the hole stays bit-equal), default 0.06*min(W,H), max 256; corner_radius defaults to half the feather. Animate the hole with set_mask_prop_key.",
            schema:obj(vec![("target_box",json!({"type":"array","items":{"type":"number"},"description":"[x,y,w,h] canvas pixels"})),("shape_id",s("axis-aligned shape clip to spotlight")),("dim_factor",n("0..1, default 0.6")),("feather_px",n("10-90% ramp width in px, 0..256, default 0.06*min(W,H)")),("corner_radius",n("px, default 0.5*feather_px")),("start",n("seconds, default 0")),("duration",n("seconds, default 5")),("ease_in",ease("fade-in seconds, default 0")),("ease_out",ease("fade-out seconds, default 0")),("track_id",s("existing track; default a new top track"))],&[]),run:add_dim_mask},
        Command{name:"spotlight",mutates:true,description:"Wrapper over add_dim_mask: same parameters, returns the expansion (one add_dim_mask call with every parameter explicit) and params. apply:false (default) changes nothing; apply:true runs that call as one undo step.",
            schema:obj(vec![("target_box",json!({"type":"array","items":{"type":"number"},"description":"[x,y,w,h] canvas pixels"})),("shape_id",s("axis-aligned shape clip")),("dim_factor",n("0..1, default 0.6")),("feather_px",n("10-90% ramp width in px, 0..256, default 0.06*min(W,H)")),("corner_radius",n("px, default 0.5*feather_px")),("start",n("seconds, default 0")),("duration",n("seconds, default 5")),("ease_in",ease("seconds, default 0")),("ease_out",ease("seconds, default 0")),("track_id",s("existing track")),("apply",json!({"type":"boolean","description":"default false: return the expansion only"}))],&[]),run:spotlight},
        Command{name:"fit_box_to_text",mutates:false,description:"Read-only, nothing is moved: the canvas-pixel rect [x,y,w,h] of a text clip's measured ink (compute_text_bounds canvas_rect, at optional t) grown by padding px on every side. Use it to size a card or underline around text.",
            schema:obj(vec![("clip_id",s("text clip id or @name")),("padding",n("px, default 0")),("t",n("seconds, default 0"))],&["clip_id"]),run:fit_box_to_text},
        Command{name:"callout_card",mutates:true,description:"Wrapper: a rounded card around text with an optional badge, as a pure expansion into add_track/add_shape/add_text/set_property/add_effect/set_meta (card rect = measured text ink + padding_x/padding_y; every default is in u = canvas height/135, 8 px at 1080p; two-layer shadow, top highlight and a badge pill are part of the expansion). apply:false (default) returns the expansion and changes nothing; apply:true runs the same ops as one undo step. position is an anchor (top_left, top_center, top_right, center, bottom_left, bottom_center, bottom_right), near:<clip_id> plus offset [dx,dy], or a box [x,y,w,h]. style is an explicit object with documented neutral defaults echoed in params (no theme); style_from copies from a shape or text clip. Text is never clipped or wrapped: too wide gives text_too_wide with a fix_hint. Clips get role callout and tag callout:<callout_id>.",
            schema:obj(vec![("text",s("callout text, \\n for new lines")),("badge",s("optional short badge text, shown above the card")),("position",json!({"description":"anchor name, near:<clip_id>, or box [x,y,w,h] in canvas px; default bottom_center"})),("offset",json!({"type":"array","items":{"type":"number"},"description":"[dx,dy]: near: = gap between target rect and the card+badge+shadow unit (unit top-left = target left+dx, target bottom+dy); anchors = inset of the unit from the anchor edge; without it near: tries below, above, right, left (gap = style.padding_y)"})),("card_offset",json!({"type":"array","items":{"type":"number"},"description":"opt-in, near: only: [dx,dy] moves the CARD (card top-left = target left+dx, target bottom+dy); exclusive with offset; the unit is still validated"})),("badge_pos",json!({"type":"string","enum":["above_left","above_right","inside_top_left","none"],"description":"default above_left; none drops the badge"})),("safe_profile",json!({"type":"string","description":"query_safe_margins profile the unit must sit inside, default title_safe; none = canvas only"})),("style",json!({"type":"object","description":"fill [r,g,b,a], text_color, font, size, letter_spacing, align, padding, padding_x, padding_y, corner_radius, margin, max_width, weight, badge_weight, shadow {dx,dy,blur,opacity} | [layers] | false (default two layers), badge_fill, badge_text_color, badge_size, badge_height, badge_padding, badge_gap"})),("style_from",s("shape or text clip to copy fill/corner_radius or font/size/text_color from")),("callout_id",s("tag suffix, default c<N>")),("start",n("seconds, default 0")),("duration",n("seconds, default 5")),("apply",json!({"type":"boolean","description":"default false: return the expansion only"}))],&["text"]),run:callout_card},
    ]
}
