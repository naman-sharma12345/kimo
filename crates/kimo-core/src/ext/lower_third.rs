//! RV-12 lower_third: a pure primitive-op expansion. The recipe dispatcher owns application;
//! this module never mutates a session, renders, exports, or bypasses batch validation.
use super::kit::*;
use super::batch::Op;

pub const VERSION: u32 = 1;
pub const KEYWORDS: &[&str] = &["lower third", "title", "subtitle", "name", "speaker", "accent", "overlay"];
pub const SUMMARY: &str = "Title and optional subtitle over a styled panel with accent and clip-local enter/exit easing; expands to standard primitives.";
/// Param schema used by recipe_describe and recipe_apply. Layout uses pixels,
/// colors use 0..1 channels, timing uses seconds. No normalized layout coordinates.
pub fn schema() -> Value {
    let easing = json!({"type":"string","enum":crate::keyframe::EASE_NAMES,"description":"Easing on the key leaving each animated segment"});
    let color = json!({"type":"array","items":{"type":"number","minimum":0,"maximum":1},"minItems":3,"maxItems":4,"description":"RGB or RGBA channels in 0..1"});
    let mut v = obj(vec![
        ("title",s("Required nonempty title; literal text, including $")), ("subtitle",s("Optional subtitle; empty omits its clip")),
        ("start",n("Absolute timeline start seconds, >= 0; default 0")), ("duration",n("Positive seconds, default 5; greater than enter + exit")),
        ("track_id",s("Existing unlocked track; omitted creates a dedicated track")), ("name_prefix",s("Name of the newly created track; default lower_third; role names stay fixed")),
        ("width",n("Panel width in px, positive; default 45% of canvas width")), ("height",n("Panel height in px, positive; default 14% of canvas height")),
        ("x",n("Panel LEFT edge in px from canvas center; default -45% canvas width")), ("y",n("Panel vertical center in px from canvas center; default 34% canvas height")),
        ("padding",n("Nonnegative inner padding in px; default 12% panel height")), ("accent_width",n("Positive accent width in px; default 3.5% panel height")),
        ("corner_radius",n("Nonnegative panel radius in px; default 6% panel height")), ("title_size",n("Positive title font size in px")), ("subtitle_size",n("Positive subtitle font size in px")),
        ("font",s("Title font path/family; default Montserrat")), ("subtitle_font",s("Subtitle font path/family; default same as title")),
        ("title_color",color.clone()), ("subtitle_color",color.clone()), ("background",color.clone()), ("accent",color),
        ("enter",n("Positive enter seconds; default 0.4")), ("exit",n("Positive exit seconds; default 0.4")),
        ("enter_ease",easing.clone()), ("exit_ease",easing), ("slide",n("Nonnegative vertical enter/exit distance in px; default 25% panel height"))
    ], &["title"]);
    v["space"] = json!("Layout: pixels from canvas center (x is panel left edge, y is panel center). Colors: normalized 0..1. Start: absolute timeline seconds. Keyframes: clip-local seconds.");
    v["additionalProperties"] = json!(false);
    for k in ["width","height","accent_width","title_size","subtitle_size","duration","enter","exit"] { v["properties"][k]["exclusiveMinimum"] = json!(0); }
    for k in ["padding","corner_radius","slide","start"] { v["properties"][k]["minimum"] = json!(0); }
    v
}


fn number(a: &Value, k: &str, default: f64) -> Result<f64, String> {
    match a.get(k) { None => Ok(default), Some(v) => v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| format!("lower_third: '{k}' must be a finite number")) }
}
fn string<'a>(a: &'a Value, k: &str, default: &'a str) -> Result<&'a str, String> {
    match a.get(k) { None => Ok(default), Some(v) => v.as_str().ok_or_else(|| format!("lower_third: '{k}' must be a string")) }
}
fn rgba(a: &Value, k: &str, default: Value) -> Result<Value, String> {
    let Some(v) = a.get(k) else { return Ok(default) };
    let arr = v.as_array().ok_or_else(|| format!("lower_third: '{k}' must be RGB or RGBA"))?;
    if !(3..=4).contains(&arr.len()) || arr.iter().any(|v| v.as_f64().map_or(true, |x| !x.is_finite() || !(0.0..=1.0).contains(&x))) {
        return Err(format!("lower_third: '{k}' must be RGB or RGBA with channels in 0..1"));
    }
    Ok(v.clone())
}
fn literal(s: &str) -> String { if s.starts_with('$') { format!("${s}") } else { s.to_string() } }
fn push(ops: &mut Vec<Op>, id: &str, tool: &str, args: Value) { ops.push(Op { id: id.into(), tool: tool.into(), args }); }
fn key(ops: &mut Vec<Op>, id: &str, property: &str, time: f64, value: f64, ease: &str) {
    let oid = format!("key{}", ops.len());
    push(ops, &oid, "set_keyframe", json!({"clip_id":format!("${id}.clip_id"),"property":property,"time":time,"value":value,"ease":ease}));
}
/// Read-only expansion, for `recipe_expand` and `recipe_apply`. Op IDs are local to this list;
/// callers composing multiple recipes must namespace them and rewrite their references.
/// x is the panel's left edge relative to canvas center; y is its vertical center.
pub fn expand(ss: &Session, a: &Value) -> Result<Vec<Op>, String> {
    if !a.is_object() { return Err("lower_third: params must be an object".into()); }
    let known = ["title","subtitle","track_id","start","duration","width","height","x","y","padding","accent_width","corner_radius","title_size","subtitle_size","font","subtitle_font","title_color","subtitle_color","background","accent","enter","exit","enter_ease","exit_ease","slide","name_prefix"];
    for k in a.as_object().unwrap().keys() { if !known.contains(&k.as_str()) { return Err(format!("lower_third: unknown parameter '{k}'")); } }
    let title = string(a,"title", "")?;
    if title.trim().is_empty() { return Err("lower_third: title must not be empty".into()); }
    let subtitle = string(a,"subtitle", "")?;
    let start = number(a,"start",0.0)?;
    let duration = number(a,"duration",5.0)?;
    let enter = number(a,"enter",0.4)?;
    let exit = number(a,"exit",0.4)?;
    if start < 0.0 || duration <= 0.0 || enter <= 0.0 || exit <= 0.0 || enter + exit >= duration {
        return Err("lower_third: start >= 0, duration > 0 and positive enter + exit < duration are required".into());
    }
    let width = number(a,"width",ss.project.width as f64 * 0.45)?;
    let height = number(a,"height",ss.project.height as f64 * 0.14)?;
    let x = number(a,"x",-(ss.project.width as f64)*0.45)?;
    let y = number(a,"y",ss.project.height as f64 * 0.34)?;
    let pad = number(a,"padding",height*0.12)?;
    let accent_width = number(a,"accent_width",height*0.035)?;
    let radius = number(a,"corner_radius",height*0.06)?;
    let ts = number(a,"title_size",height * if subtitle.is_empty() {0.44} else {0.32})?;
    let subs = number(a,"subtitle_size",height*0.22)?;
    let slide = number(a,"slide",height*0.25)?;
    if width <= 0.0 || height <= 0.0 || pad < 0.0 || accent_width <= 0.0 || radius < 0.0 || ts <= 0.0 || subs <= 0.0 || slide < 0.0 || 2.0*pad+accent_width >= width || 2.0*pad >= height {
        return Err("lower_third: invalid layout; sizes must be positive, padding/radius/slide nonnegative, and padding + accent must fit inside the panel".into());
    }
    let enter_ease = string(a,"enter_ease","out_quart")?;
    let exit_ease = string(a,"exit_ease","in")?;
    for e in [enter_ease,exit_ease] { serde_json::from_value::<crate::keyframe::Ease>(json!(e)).map_err(|_| format!("lower_third: easing '{e}' not allowed. Valid: {}", crate::keyframe::EASE_NAMES.join(", ")))?; }
    let font = string(a,"font","Montserrat")?;
    let subfont = string(a,"subtitle_font",font)?;
    let title_color = rgba(a,"title_color",json!([1,1,1]))?;
    let subtitle_color = rgba(a,"subtitle_color",json!([0.8,0.85,0.9]))?;
    let bg = rgba(a,"background",json!([0.04,0.05,0.08,0.9]))?;
    let accent = rgba(a,"accent",json!([1,0.36,0.15]))?;
    let prefix = string(a,"name_prefix","lower_third")?;
    let mut ops = vec![];
    let track = if let Some(v) = a.get("track_id") {
        let tid = v.as_str().ok_or("lower_third: track_id must be a string")?;
        let t = ss.project.tracks.iter().find(|t| t.id==tid).ok_or_else(|| format!("lower_third: no track '{tid}'"))?;
        if t.locked { return Err(format!("lower_third: track '{tid}' is locked")); }
        literal(tid)
    } else { push(&mut ops,"trk","add_track",json!({}));
        push(&mut ops,"track_name","set_track",json!({"track_id":"$trk.track_id","name":literal(prefix)})); "$trk.track_id".into() };
    push(&mut ops,"bar","add_shape",json!({"track_id":track,"shape":"rect","start":start,"duration":duration,"width":width,"height":height,"fill":bg,"corner_radius":radius}));
    push(&mut ops,"accent","add_shape",json!({"track_id":track,"shape":"rect","start":start,"duration":duration,"width":accent_width,"height":height-2.0*pad,"fill":accent}));
    let left = x + pad + accent_width + pad;
    let mut placements = vec![("bar",x+width*0.5,y,1.0),("accent",x+pad+accent_width*0.5,y,1.0)];
    let mut title_bottom = None;
    for (id,text,f,size,col,ty) in [("title",title,font,ts,title_color,y-if subtitle.is_empty(){0.0}else{height*0.17}), ("subtitle",subtitle,subfont,subs,subtitle_color,y+height*0.2)] {
        if text.is_empty() { continue; }
        // Center anchoring lets the primitive's x be keyed directly without losing the
        // measured left-anchor offset. Match add_text's escaped-newline interpretation.
        let (bw,bh) = crate::text::measure(&text.replace("\\n","\n"), f,size,0.0,0.0,"left",ss.project.width as f64)?;
        if bw > width-(left-x)-pad || bh > height-2.0*pad || (ty-y).abs()+bh*0.5 > height*0.5-pad { return Err(format!("lower_third: {id} text does not fit; increase width/height or reduce text size")); }
        push(&mut ops,id,"add_text",json!({"track_id":track,"text":literal(text),"font":literal(f),"size":size,"color":col,"anchor":"center","align":"left","start":start,"duration":duration}));
        if id == "title" { title_bottom = Some(ty+bh*0.5); }
        if id == "subtitle" && title_bottom.map_or(false, |bottom| bottom > ty-bh*0.5) { return Err("lower_third: title and subtitle overlap; increase height or reduce text size".into()); }
        let alpha = col.get(3).and_then(|v|v.as_f64()).unwrap_or(1.0);
        placements.push((id,left+bw*0.5,ty,alpha));
    }
    for (id,px,py,alpha) in placements {
        let oid=format!("{id}_x");push(&mut ops,&oid,"set_property",json!({"clip_id":format!("${id}.clip_id"),"property":"x","value":px}));
        for (t,opacity,ease) in [(0.0,0.0,enter_ease),(enter,1.0,"hold"),(duration-exit,1.0,exit_ease),(duration,0.0,"linear")] { key(&mut ops,id,"opacity",t,opacity*alpha,ease); }
        for (t,v,ease) in [(0.0,py+slide,enter_ease),(enter,py,"hold"),(duration-exit,py,exit_ease),(duration,py+slide,"linear")] { key(&mut ops,id,"y",t,v,ease); }
    }
    Ok(ops)
}
// Pure expansion module. The shared recipe dispatcher registers the user-facing tools.
pub fn commands() -> Vec<Command> { vec![] }
