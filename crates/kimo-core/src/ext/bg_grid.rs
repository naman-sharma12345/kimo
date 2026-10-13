//! V1c-D bg_grid: an animated grid or dot-field background, a PURE expansion into existing tools: add_clip (solid in bg_color) + add_effect (pixel expression)
//! + the common finish ops. Every number in the expression goes through one formatter (`n`), never a caller string. Pixel centres are at x+0.5.
use super::kit::*;
use super::bgcommon::{self as bg, bg_err, num, Common, E_EXPR_OPS, E_RANGE};

const OWN: &[&str] = &["mode", "spacing_px", "line_px", "color", "opacity_pattern", "bg_color", "bg", "drift", "origin", "fade", "antialias", "engine"];
/// The one number formatter: shortest round-trip f32 text, negatives parenthesised so they parse anywhere in the expression grammar.
pub(crate) fn n(x: f64) -> String { let v = x as f32; if v < 0.0 { format!("(-{})", -v) } else { format!("{v}") } }
fn rgb(a: &Value, k: &str, d: [f64; 3], call: &str) -> Result<[f64; 3], String> {
    let Some(v) = a.get(k).filter(|v| !v.is_null()) else { return Ok(d) };
    let arr = v.as_array().filter(|x| x.len() == 3).ok_or_else(|| bg_err(E_RANGE, &format!("{k} must be [r, g, b] with each 0..1 (got {v})"), call))?;
    let mut o = [0.0; 3]; for (i, c) in arr.iter().enumerate() { o[i] = c.as_f64().filter(|f| f.is_finite() && (0.0..=1.0).contains(f)).ok_or_else(|| bg_err(E_RANGE, &format!("{k}[{i}] must be a number 0..1 (got {c})"), call))?; } Ok(o)
}
fn pair(a: &Value, k: &str, lim: f64, call: &str) -> Result<[f64; 2], String> {
    let Some(v) = a.get(k).filter(|v| !v.is_null()) else { return Ok([0.0, 0.0]) };
    let arr = v.as_array().filter(|x| x.len() == 2).ok_or_else(|| bg_err(E_RANGE, &format!("{k} must be [x, y] (got {v})"), call))?;
    let mut o = [0.0; 2]; for (i, c) in arr.iter().enumerate() { o[i] = c.as_f64().filter(|f| f.is_finite() && f.abs() <= lim).ok_or_else(|| bg_err(E_RANGE, &format!("{k}[{i}] must be a number within +-{lim} (got {c})"), call))?; } Ok(o)
}
/// Build the expression text and the resolved params. Pure: numbers in, text out.
pub(crate) fn expression(mode: &str, s: f64, l: f64, col: [f64; 3], op: f64, drift: [f64; 2], origin: [f64; 2], fade: Option<[f64; 4]>, aa: bool) -> String {
    let px = format!("(x+0.5+{}*t-{})", n(drift[0]), n(origin[0])); let py = format!("(y+0.5+{}*t-{})", n(drift[1]), n(origin[1]));
    let (half, w) = (l / 2.0, if aa { 0.5 } else { 0.001 });
    let (e0, e1) = (n(half - w), n(half + w)); let sp = n(s);
    let cov = if mode == "lines" {
        let z = |p: &str| format!("(fract({p}/{sp}+0.5)-0.5)"); let d = |p: &str| { let zz = z(p); format!("(1-smoothstep({e0},{e1},max({zz},0-{zz})*{sp}))") };
        format!("max({},{})", d(&px), d(&py))
    } else {
        let z = |p: &str| { let q = format!("((fract({p}/{sp}+0.5)-0.5)*{sp})"); format!("({q}*{q})") };
        format!("(1-smoothstep({e0},{e1},sqrt({}+{})))", z(&px), z(&py))
    };
    let fadef = match fade { None => String::new(), Some([cx, cy, r, soft]) => { let e0 = if soft > 0.0 { r * (1.0 - soft) } else { r - 0.001 };
        format!("*(1-smoothstep({},{},sqrt((x+0.5-{})*(x+0.5-{})+(y+0.5-{})*(y+0.5-{}))))", n(e0), n(r), n(cx), n(cx), n(cy), n(cy)) } };
    let k = format!("({cov}*{}{fadef})", n(op));
    format!("vec4(r+({}-r)*{k},g+({}-g)*{k},b+({}-b)*{k},a)", n(col[0]), n(col[1]), n(col[2]))
}
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let c = Common::parse(ss, a, "bg_grid", "grid", OWN)?; let call = |s: &str| format!("bg_grid({s})");
    let mode = match a.get("mode").and_then(|v| if v.is_null() { None } else { Some(v) }) { None => "lines".to_string(), Some(v) => v.as_str().filter(|m| ["lines", "dots"].contains(m)).ok_or_else(|| bg_err(E_RANGE, &format!("mode must be lines or dots (got {v})"), &call("mode=\"lines\"")))?.to_string() };
    let spacing = num(a, "spacing_px", 64.0, 4.0, 2048.0, &call("spacing_px=64"))?; let line = num(a, "line_px", 1.0, 0.5, 16.0, &call("line_px=1"))?; let op = num(a, "opacity_pattern", 0.08, 0.0, 1.0, &call("opacity_pattern=0.08"))?;
    let color = rgb(a, "color", [1.0, 1.0, 1.0], &call("color=[1,1,1]"))?; let bgc = rgb(a, "bg_color", [0.04, 0.04, 0.06], &call("bg_color=[0.04,0.04,0.06]"))?;
    let bgk = match a.get("bg").filter(|v| !v.is_null()) { None => "solid", Some(v) => match v.as_str() { Some("solid") => "solid", Some("transparent") => return Err(bg_err(E_RANGE, "bg transparent is not available: a solid clip has no alpha, so the grid draws over bg_color", &call("blend=\"add\", bg_color=[0,0,0]"))), _ => return Err(bg_err(E_RANGE, &format!("bg must be solid (got {v})"), &call("bg=\"solid\""))) } };
    let drift = pair(a, "drift", 4096.0, &call("drift=[0,0]"))?; let origin = pair(a, "origin", 1e6, &call("origin=[0,0]"))?;
    let aa = match a.get("antialias").filter(|v| !v.is_null()) { None => true, Some(v) => v.as_bool().ok_or_else(|| bg_err(E_RANGE, "antialias must be a boolean", &call("antialias=true")))? };
    let engine = match a.get("engine").filter(|v| !v.is_null()) { None => "expr", Some(v) => match v.as_str() { Some("expr") => "expr", Some("shapes") => return Err(bg_err(E_RANGE, "engine shapes is not implemented; only the pixel expression engine is available", &call("engine=\"expr\""))), _ => return Err(bg_err(E_RANGE, &format!("engine must be expr (got {v})"), &call("engine=\"expr\""))) } };
    let fade = match a.get("fade").filter(|v| !v.is_null()) { None => None, Some(v) => {
        let o = v.as_object().ok_or_else(|| bg_err(E_RANGE, "fade must be {cx, cy, radius, softness}", &call("fade={cx:128,cy:128,radius:200,softness:0.5}")))?;
        for k in o.keys() { if !["cx", "cy", "radius", "softness"].contains(&k.as_str()) { return Err(bg_err(E_RANGE, &format!("unknown fade key '{k}'. Valid: cx, cy, radius, softness"), &call("fade={cx:128,cy:128,radius:200,softness:0.5}"))); } }
        let f = |k: &str, d: f64, lo: f64, hi: f64| num(v, k, d, lo, hi, &call("fade={cx:128,cy:128,radius:200,softness:0.5}"));
        Some([f("cx", ss.project.width as f64 / 2.0, -1e6, 1e6)?, f("cy", ss.project.height as f64 / 2.0, -1e6, 1e6)?, f("radius", 256.0, 1.0, 8192.0)?, f("softness", 0.5, 0.0, 1.0)?]) } };
    let expr = expression(&mode, spacing, line, color, op, drift, origin, fade, aa);
    let cap = crate::resource_limits::current().expr_max_ops.unwrap_or(u64::MAX).min(usize::MAX as u64) as usize;
    let ops_used = match crate::pixel_expr::compile(&expr, cap) { Ok(p) => p.ops, Err(e) if e.contains("expr_max_ops") => return Err(bg_err(E_EXPR_OPS, &format!("the grid expression needs more than the expr_max_ops limit of {cap}"), &format!("set_limits(expr_max_ops={})", cap * 2))), Err(e) => return Err(format!("internal: grid expression did not compile: {e}")) };
    let mut ops = vec![json!({"id":"bg","tool":"add_clip","args":{"track_id":c.track,"source_type":"solid","color":bgc,"start":c.start,"duration":c.duration}}),
        json!({"id":"fx","tool":"add_effect","args":{"clip_id":"$bg.clip_id","kind":"pixel_expr","strings":{"expr":expr}}})];
    ops.extend(c.finish_ops("bg"));
    let params = { let mut p = c.echo(); for (k, v) in [("mode", json!(mode)), ("spacing_px", json!(spacing)), ("line_px", json!(line)), ("opacity_pattern", json!(op)), ("color", json!(color)), ("bg_color", json!(bgc)), ("bg", json!(bgk)), ("drift", json!(drift)), ("origin", json!(origin)), ("fade", fade.map_or(Value::Null, |f| json!({"cx":f[0],"cy":f[1],"radius":f[2],"softness":f[3]}))), ("antialias", json!(aa)), ("engine", json!(engine))] { p[k] = v; }
        p["dot_radius_px"] = if mode == "dots" { json!(line / 2.0) } else { Value::Null }; p };
    let next = if c.apply { vec![json!({"tool":"lint_frame","args":{"time":c.start,"annotate":false},"why":format!("check text or layers placed over the grid at opacity_pattern {op}")})] } else { vec![] };
    bg::finish(ss, &c, params, ops, &["bg"], json!({"expr_ops": ops_used, "expr_max_ops": cap, "cost": "one pixel expression pass per frame", "keys": 0}), next)
}
pub fn commands() -> Vec<Command> {
    let mut props = bg::common_props(); props.extend([
        ("mode", json!({"type":"string","enum":["lines","dots"],"description":"lines (default) or dots; for dots line_px is the dot diameter"})), ("spacing_px", json!({"type":"number","minimum":4,"maximum":2048,"default":64,"description":"cell size in px"})), ("line_px", json!({"type":"number","minimum":0.5,"maximum":16,"default":1,"description":"line width in px (dot diameter for dots)"})),
        ("color", json!({"type":"array","items":{"type":"number"},"description":"grid colour [r,g,b] 0..1, default [1,1,1]"})), ("opacity_pattern", json!({"type":"number","minimum":0,"maximum":1,"default":0.08,"description":"grid strength over the background 0..1"})), ("bg_color", json!({"type":"array","items":{"type":"number"},"description":"background solid [r,g,b], default [0.04,0.04,0.06]"})),
        ("bg", json!({"type":"string","enum":["solid"],"description":"solid only: a solid clip has no alpha; use blend add or screen to overlay"})), ("drift", json!({"type":"array","items":{"type":"number"},"description":"[vx,vy] px per second, each within +-4096"})), ("origin", json!({"type":"array","items":{"type":"number"},"description":"[x,y] grid offset in px"})),
        ("fade", json!({"type":"object","description":"radial fade {cx,cy,radius 1..8192,softness 0..1}"})), ("antialias", json!({"type":"boolean","description":"default true"})), ("engine", json!({"type":"string","enum":["expr"],"description":"expr only"}))]);
    let mut m = serde_json::Map::new(); for (k, v) in props { m.insert(k.into(), v); }
    vec![Command{name:"bg_grid",mutates:true,description:"Animated grid or dot background: a solid colour with lines or dots, optional drift and radial fade, for titles and UI scenes (engine shapes is not implemented, only expr). A pure expansion into add_clip, add_effect with a pixel expression and set_meta; apply false (default) returns the calls and changes nothing, apply true runs them as one undo step. The solid has no alpha: to overlay video use blend add or screen. Numbers in the expression come from one formatter. Reports the expression op count against the expr_max_ops limit.",schema:json!({"type":"object","properties":m,"required":["track","duration"]}),run}]
}
