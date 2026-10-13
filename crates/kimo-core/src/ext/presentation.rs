//! V1c presentation primitives. `add_dim_mask` is the primitive: a full-canvas black layer at `dim_factor` opacity with an inverted, feathered rect
//! mask hole, built from existing tools only (add_track, add_clip, set_property, add_mask, set_keyframe, set_meta) and applied as ONE undo step through
//! `batch`. `spotlight` is a pure wrapper: it expands to one `add_dim_mask` call with every parameter explicit and echoes it; `apply:false` (default) returns
//! the expansion and changes nothing, `apply:true` runs exactly that call.
use super::kit::*;

const MAX_FEATHER: f64 = 256.0;
fn finite(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(d),
        Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("out_of_range: {k} must be a finite number (got {v}); fix_hint: pass a plain number")),
    }
}
fn range(a: &Value, k: &str, d: f64, lo: f64, hi: f64, hint: &str) -> Result<f64, String> {
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
    let feather = range(a, "feather_px", 0.0, 0.0, MAX_FEATHER, "feather_px is capped at 256; lower it")?;
    let (bw, bh) = (x1 - x0, y1 - y0);
    let radius = range(a, "corner_radius", 0.0, 0.0, bw.min(bh) / 2.0, "corner_radius cannot exceed half the shorter side of the clipped box")?;
    let start = range(a, "start", 0.0, 0.0, 1e7, "start is timeline seconds, >= 0")?;
    let dur = range(a, "duration", 5.0, 1e-3, 1e7, "duration is seconds, > 0")?;
    let ein = range(a, "ease_in", 0.0, 0.0, dur, "ease_in is seconds, 0..duration")?;
    let eout = range(a, "ease_out", 0.0, 0.0, dur, "ease_out is seconds, 0..duration")?;
    if ein + eout > dur { return Err(format!("out_of_range: ease_in + ease_out must be <= duration (got {} > {dur}); fix_hint: shorten the eases", ein + eout)); }
    let track = match a.get("track_id") { None | Some(Value::Null) => None, Some(v) => Some(super::names::resolve_ref(&ss.project, "track", v.as_str().ok_or("argument 'track_id' must be a string")?)?) };
    let params = json!({"target_box":[tb[0], tb[1], tb[2], tb[3]], "clipped_box":[x0, y0, bw, bh], "dim_factor":dim, "feather_px":feather, "corner_radius":radius, "start":start, "duration":dur, "ease_in":ein, "ease_out":eout, "track_id":track,
        "layer":"solid black, opacity = dim_factor, rect mask invert=true (hole), feather in px, mode add", "role":"dim_mask"});
    let mut ops = vec![];
    let tid = match &track { Some(t) => json!(t), None => { ops.push(json!({"id":"track","tool":"add_track","args":{}})); json!("$track.track_id") } };
    ops.push(json!({"id":"dim","tool":"add_clip","args":{"track_id":tid,"source_type":"solid","color":[0,0,0],"start":start,"duration":dur}}));
    ops.push(json!({"id":"role","tool":"set_meta","args":{"id":"$dim.clip_id","role":"dim_mask"}}));
    ops.push(json!({"id":"op","tool":"set_property","args":{"clip_id":"$dim.clip_id","property":"opacity","value":dim}}));
    let round = if bw.min(bh) > 0.0 { radius / (bw.min(bh) / 2.0) } else { 0.0 };
    ops.push(json!({"id":"mask","tool":"add_mask","args":{"clip_id":"$dim.clip_id","kind":"rect","mode":"add","invert":true,"feather":feather,"params":{"cx":(x0 + bw / 2.0) / cw,"cy":(y0 + bh / 2.0) / ch,"w":bw / cw,"h":bh / ch,"round":round}}}));
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
    Ok(json!({"clip_id":clip,"mask_id":mask,"track_id":track,"params":params,"expansion":ops,"note":"Animate the hole with set_mask_prop_key on mask_id (cx, cy, w, h, feather, round are clip-local, normalized 0..1 except feather px)."}))
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
    let badge_fill = rgba(&g("badge_fill"), "badge_fill", Some([0.90, 0.25, 0.20, 1.0]))?.unwrap();
    let badge_text_color = rgba(&g("badge_text_color"), "badge_text_color", Some([1.0, 1.0, 1.0, 1.0]))?.unwrap();
    let font = get("font").as_str().unwrap_or("Montserrat").to_string();
    let align = get("align").as_str().unwrap_or("left").to_string();
    if !["left", "center", "right"].contains(&align.as_str()) { return Err("out_of_range: style.align is not allowed. Valid: left, center, right; fix_hint: use one of those".into()); }
    let size = range(&g("size"), "size", 36.0, 1.0, 2000.0, "size is pixels, 1..2000")?;
    let letter = range(&g("letter_spacing"), "letter_spacing", 0.0, -100.0, 1000.0, "letter_spacing is pixels")?;
    let padding = range(&g("padding"), "padding", 16.0, 0.0, 1000.0, "padding is pixels, >= 0")?;
    let radius = range(&g("corner_radius"), "corner_radius", 12.0, 0.0, 10000.0, "corner_radius is pixels, >= 0")?;
    let margin = range(&g("margin"), "margin", 48.0, 0.0, 10000.0, "margin is pixels from the canvas edge for anchors")?;
    let max_card_w = range(&g("max_width"), "max_width", (cw - 2.0 * margin).max(1.0), 1.0, 100000.0, "max_width is the widest allowed card in pixels")?;
    let bsize = range(&g("badge_size"), "badge_size", 20.0, 1.0, 2000.0, "badge_size is pixels")?;
    let bpad = range(&g("badge_padding"), "badge_padding", 8.0, 0.0, 1000.0, "badge_padding is pixels")?;
    let bgap = range(&g("badge_gap"), "badge_gap", 6.0, 0.0, 1000.0, "badge_gap is the pixel gap between badge and card")?;
    let shadow = match get("shadow") { Value::Null => None, v => {
        let o = v.as_object().ok_or("out_of_range: style.shadow must be an object {dx,dy,blur,opacity} or null; fix_hint: omit it for no shadow")?; let o = Value::Object(o.clone());
        Some(json!({"dx": range(&o, "dx", 0.0, -1000.0, 1000.0, "dx is pixels")?, "dy": range(&o, "dy", 8.0, -1000.0, 1000.0, "dy is pixels")?, "blur": range(&o, "blur", 12.0, 0.0, 500.0, "blur is pixels")?, "opacity": range(&o, "opacity", 0.5, 0.0, 1.0, "opacity is 0..1")?})) } };
    let start = range(a, "start", 0.0, 0.0, 1e7, "start is timeline seconds, >= 0")?;
    let dur = range(a, "duration", 5.0, 1e-3, 1e7, "duration is seconds, > 0")?;
    // measure the text exactly as the renderer lays it out (read-only), centred on the canvas at x=y=0
    let measure = |ss: &mut Session, t: &str, sz: f64, ls: f64, al: &str| text_rect(ss, json!({"text": t, "size": sz, "font": font, "letter_spacing": ls, "align": al, "x": 0, "y": 0}));
    let ink = measure(ss, &text, size, letter, &align)?;
    let (card_w, card_h) = (ink[2] + 2.0 * padding, ink[3] + 2.0 * padding);
    if card_w > max_card_w + 1e-9 { return Err(format!("text_too_wide: the card would be {card_w:.1}px wide but style.max_width is {max_card_w:.1}px; the text is never clipped or auto-wrapped; fix_hint: add \\n line breaks, lower style.size, or raise style.max_width")); }
    // position: a box [x,y,w,h] (card is centred in it and must fit), an anchor name (margin from the edge), or near:<clip_id> (below that clip, plus offset)
    let pos = a.get("position").cloned().unwrap_or(json!("bottom_center"));
    let (px0, py0, pos_note);
    if let Some(arr) = pos.as_array() {
        if arr.len() != 4 { return Err(format!("out_of_range: position box must be [x,y,w,h] (got {} numbers); fix_hint: pass 4 numbers or an anchor name", arr.len())); }
        let mut b = [0.0; 4]; for i in 0..4 { b[i] = arr[i].as_f64().filter(|f| f.is_finite()).ok_or_else(|| format!("out_of_range: position[{i}] must be a finite number; fix_hint: pass plain pixel numbers"))?; }
        if b[2] <= 0.0 || b[3] <= 0.0 { return Err("out_of_range: position box w and h must be > 0; fix_hint: pass a box with positive size".into()); }
        if card_w > b[2] + 1e-9 || card_h > b[3] + 1e-9 { return Err(format!("text_too_wide: the card is {card_w:.1} x {card_h:.1}px but the position box is {} x {}; fix_hint: enlarge the box, lower style.size or add \\n", b[2], b[3])); }
        px0 = b[0] + (b[2] - card_w) / 2.0; py0 = b[1] + (b[3] - card_h) / 2.0; pos_note = "box: card centred in the box";
    } else if let Some(name) = pos.as_str() {
        if let Some(near) = name.strip_prefix("near:") {
            let id = super::names::resolve_ref(&ss.project, "clip", near)?;
            let is_text = matches!(ss.project.find_clip(&id).map(|c| &c.source), Some(crate::model::Source::Text { .. }));
            let r = if is_text { text_rect(ss, json!({"clip_id": id}))? } else { target(ss, &json!({"shape_id": id}))? };
            let off = match a.get("offset") { None | Some(Value::Null) => [0.0, 12.0], Some(v) => { let o = v.as_array().filter(|x| x.len() == 2).ok_or("out_of_range: offset must be [dx,dy]; fix_hint: pass two pixel numbers")?; [o[0].as_f64().filter(|f| f.is_finite()).ok_or("out_of_range: offset must be finite")?, o[1].as_f64().filter(|f| f.is_finite()).ok_or("out_of_range: offset must be finite")?] } };
            px0 = r[0] + off[0]; py0 = r[1] + r[3] + off[1]; pos_note = "near: card top-left = target left + dx, target bottom + dy (default offset [0,12])";
        } else if ANCHORS.contains(&name) {
            px0 = if name.ends_with("left") { margin } else if name.ends_with("right") { cw - margin - card_w } else { (cw - card_w) / 2.0 };
            py0 = if name.starts_with("top") { margin } else if name.starts_with("bottom") { ch - margin - card_h } else { (ch - card_h) / 2.0 };
            pos_note = "anchor: card edge sits `margin` px from the canvas edge, centred on the other axis";
        } else { return Err(format!("out_of_range: position '{name}' is not allowed. Valid: {}, near:<clip_id>, or a box [x,y,w,h]; fix_hint: use one of those", ANCHORS.join(", "))); }
    } else { return Err("out_of_range: position must be an anchor name, near:<clip_id> or a box [x,y,w,h]; fix_hint: pass one of those".into()); }
    let card = [px0, py0, card_w, card_h];
    let mut badge_rect = None; let mut binfo = None;
    if let Some(bt) = &badge {
        let bi = measure(ss, bt, bsize, 0.0, "center")?;
        let (bw, bh) = (bi[2] + 2.0 * bpad, bi[3] + 2.0 * bpad);
        let br = [px0, py0 - bgap - bh, bw, bh];
        badge_rect = Some(br); binfo = Some(bi);
    }
    let all = [Some(card), badge_rect];
    for r in all.iter().flatten() {
        if r[0] < -1e-9 || r[1] < -1e-9 || r[0] + r[2] > cw + 1e-9 || r[1] + r[3] > ch + 1e-9 { return Err(format!("out_of_range: the callout rect [{:.1}, {:.1}, {:.1}, {:.1}] leaves the {cw} x {ch} canvas; fix_hint: move it with position, lower style.size, or shorten the text (the badge sits above the card)", r[0], r[1], r[2], r[3])); }
    }
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
    let txt = |id: &str, track: &str, t: &str, sz: f64, ls: f64, al: &str, col: [f64; 4], ink_now: [f64; 4], want_left_top: [f64; 2], ops: &mut Vec<Value>, font: &str| {
        // measured at x=y=0 the ink top-left is ink_now; shift so it lands on want_left_top
        ops.push(json!({"id": id, "tool": "add_text", "args": {"track_id": format!("$t{track}.track_id"), "text": t, "start": start, "duration": dur, "font": font, "size": sz, "color": [col[0], col[1], col[2]], "letter_spacing": ls, "align": al}}));
        ops.push(json!({"tool": "set_property", "args": {"clip_id": format!("${id}.clip_id"), "property": "x", "value": want_left_top[0] - ink_now[0]}}));
        ops.push(json!({"tool": "set_property", "args": {"clip_id": format!("${id}.clip_id"), "property": "y", "value": want_left_top[1] - ink_now[1]}}));
    };
    shape("card", "0", card, fill, radius, &mut ops);
    if let Some(sh) = &shadow { ops.push(json!({"tool": "add_effect", "args": {"clip_id": "$card.clip_id", "kind": "drop_shadow", "params": sh}})); }
    txt("text", "1", &text, size, letter, &align, text_color, ink, [px0 + padding, py0 + padding], &mut ops, &font);
    if let (Some(bt), Some(br), Some(bi)) = (&badge, badge_rect, binfo) {
        shape("badge", "2", br, badge_fill, br[3] / 2.0, &mut ops);
        txt("badge_text", "3", bt, bsize, 0.0, "center", badge_text_color, bi, [br[0] + bpad, br[1] + bpad], &mut ops, &font);
    }
    let ids: Vec<&str> = if badge.is_some() { vec!["card", "text", "badge", "badge_text"] } else { vec!["card", "text"] };
    for id in &ids { ops.push(json!({"tool": "set_meta", "args": {"id": format!("${id}.clip_id"), "role": "callout", "tags": [tag.clone()]}})); }
    let text_ink = [px0 + padding, py0 + padding, ink[2], ink[3]];
    let style = json!({"fill": fill, "text_color": text_color, "font": font, "size": size, "letter_spacing": letter, "align": align, "padding": padding, "corner_radius": radius, "margin": margin, "max_width": max_card_w, "shadow": shadow,
        "badge_fill": badge_fill, "badge_text_color": badge_text_color, "badge_size": bsize, "badge_padding": bpad, "badge_gap": bgap});
    let params = json!({"text": text, "badge": badge, "position": pos, "position_rule": pos_note, "style": style, "style_from": a.get("style_from"), "callout_id": cid, "tag": tag, "role": "callout", "start": start, "duration": dur,
        "card_rect": card, "text_rect": text_ink, "badge_rect": badge_rect,
        "layout": "tracks bottom to top: card, text, badge, badge text (later tracks draw on top); card_rect = measured text ink rect grown by style.padding; the badge sits above the card, left aligned, never over the text; text is never clipped or auto-wrapped (text_too_wide error)"});
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
        Command{name:"add_dim_mask",mutates:true,description:"Dim everything except a box: adds a full-canvas black layer at dim_factor opacity with a feathered inverted rect hole over target_box [x,y,w,h] canvas px or an axis-aligned shape_id (box clipped to the canvas). Built from add_track/add_clip/set_property/add_mask/set_keyframe/set_meta and applied as one undo step; returns clip_id, mask_id and the exact op list. feather_px max 256. Animate the hole with set_mask_prop_key.",
            schema:obj(vec![("target_box",json!({"type":"array","items":{"type":"number"},"description":"[x,y,w,h] canvas pixels"})),("shape_id",s("axis-aligned shape clip to spotlight")),("dim_factor",n("0..1, default 0.6")),("feather_px",n("0..256, default 0")),("corner_radius",n("px, default 0")),("start",n("seconds, default 0")),("duration",n("seconds, default 5")),("ease_in",ease("fade-in seconds, default 0")),("ease_out",ease("fade-out seconds, default 0")),("track_id",s("existing track; default a new top track"))],&[]),run:add_dim_mask},
        Command{name:"spotlight",mutates:true,description:"Wrapper over add_dim_mask: same parameters, returns the expansion (one add_dim_mask call with every parameter explicit) and params. apply:false (default) changes nothing; apply:true runs that call as one undo step.",
            schema:obj(vec![("target_box",json!({"type":"array","items":{"type":"number"},"description":"[x,y,w,h] canvas pixels"})),("shape_id",s("axis-aligned shape clip")),("dim_factor",n("0..1, default 0.6")),("feather_px",n("0..256, default 0")),("corner_radius",n("px, default 0")),("start",n("seconds, default 0")),("duration",n("seconds, default 5")),("ease_in",ease("seconds, default 0")),("ease_out",ease("seconds, default 0")),("track_id",s("existing track")),("apply",json!({"type":"boolean","description":"default false: return the expansion only"}))],&[]),run:spotlight},
        Command{name:"fit_box_to_text",mutates:false,description:"Read-only, nothing is moved: the canvas-pixel rect [x,y,w,h] of a text clip's measured ink (compute_text_bounds canvas_rect, at optional t) grown by padding px on every side. Use it to size a card or underline around text.",
            schema:obj(vec![("clip_id",s("text clip id or @name")),("padding",n("px, default 0")),("t",n("seconds, default 0"))],&["clip_id"]),run:fit_box_to_text},
        Command{name:"callout_card",mutates:true,description:"Wrapper: a rounded card around text with an optional badge, as a pure expansion into add_track/add_shape/add_text/set_property/add_effect/set_meta (card rect = measured text ink + style.padding). apply:false (default) returns the expansion and changes nothing; apply:true runs the same ops as one undo step. position is an anchor (top_left, top_center, top_right, center, bottom_left, bottom_center, bottom_right), near:<clip_id> plus offset [dx,dy], or a box [x,y,w,h]. style is an explicit object with documented neutral defaults echoed in params (no theme); style_from copies from a shape or text clip. Text is never clipped or wrapped: too wide gives text_too_wide with a fix_hint. Clips get role callout and tag callout:<callout_id>.",
            schema:obj(vec![("text",s("callout text, \\n for new lines")),("badge",s("optional short badge text, shown above the card")),("position",json!({"description":"anchor name, near:<clip_id>, or box [x,y,w,h] in canvas px; default bottom_center"})),("offset",json!({"type":"array","items":{"type":"number"},"description":"[dx,dy] for near:, default [0,12]"})),("style",json!({"type":"object","description":"fill [r,g,b,a], text_color, font, size, letter_spacing, align, padding, corner_radius, margin, max_width, shadow {dx,dy,blur,opacity}|null, badge_fill, badge_text_color, badge_size, badge_padding, badge_gap"})),("style_from",s("shape or text clip to copy fill/corner_radius or font/size/text_color from")),("callout_id",s("tag suffix, default c<N>")),("start",n("seconds, default 0")),("duration",n("seconds, default 5")),("apply",json!({"type":"boolean","description":"default false: return the expansion only"}))],&["text"]),run:callout_card},
    ]
}
