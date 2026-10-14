//! V1c-D bg_orbs: soft glowing orbs on a solid or gradient base, a PURE expansion into add_clip + one add_effect (pixel expression) per orb + an optional dither
//! effect + the common finish ops. One effect per orb keeps every expression far below expr_max_ops (no shared subexpressions exist in the grammar) and orbs are
//! sorted into a canonical order before expansion, so swapping orbs in the call gives a byte-identical expansion and render. Pixel centres are at x+0.5.
use super::kit::*;
use super::bg_grid::n;
use super::bgcommon::{self as bg, bg_err, num, Common, E_EXPR_OPS, E_KEYCAP, E_ORBS, E_RANGE};

const OWN: &[&str] = &["orbs", "base", "dither", "combine", "vignette", "grain", "ceiling"];
const ORB_KEYS: &[&str] = &["cx", "cy", "radius", "color", "intensity", "falloff", "orbit", "keys", "layer"];
/// Depth layers. mid is the identity so a plain orb is unchanged. relative to a mid orb at intensity 0.5: far = small radius x0.5, alpha x0.55 (0.275), orbit x0.5 (slow drift), long-tail inverse_square falloff unless one is given (heavy dispersion); near = larger radius x1.5, alpha x2.8 (1.4, a core that the shoulder holds at 230-240), orbit x1.67, gaussian falloff (tighter); parallax ratio far:mid:near = 0.3:0.6:1.0.
const LAYERS: &[&str] = &["far", "mid", "near"];
fn layer_mults(l: &str) -> (f64, f64, f64) { match l { "far" => (0.5, 0.55, 0.5), "near" => (1.5, 2.8, 1.0 / 0.6), _ => (1.0, 1.0, 1.0) } }
const FALLOFFS: &[&str] = &["gaussian", "inverse_square", "smooth"];
#[derive(Clone, Debug)]
pub(crate) struct Orb { pub cx: f64, pub cy: f64, pub radius: f64, pub color: [f64; 3], pub intensity: f64, pub falloff: String, pub orbit: Option<[f64; 4]>, pub keys: Vec<[f64; 3]>, pub layer: String }
impl Orb { fn sort_key(&self) -> String { format!("{:?}", (self.cx, self.cy, self.radius, self.color, self.intensity, &self.falloff, self.orbit, &self.keys, &self.layer)) } }
/// Expression for one orb added onto the running pixel. Centre is a literal, an orbit (cos(x)=sin(x+pi/2)) or the animated params p0, p1 (keyed).
pub(crate) fn orb_expression(o: &Orb, combine: &str) -> String {
    let (cx, cy) = if !o.keys.is_empty() { ("p0".to_string(), "p1".to_string()) } else if let Some([rx, ry, hz, ph]) = o.orbit {
        let a = format!("6.2831853*{}*t+{}", n(hz), n(ph));
        (format!("({}+{}*sin({a}+1.5707963))", n(o.cx), n(rx)), format!("({}+{}*sin({a}))", n(o.cy), n(ry)))
    } else { (n(o.cx), n(o.cy)) };
    let d2 = format!("((x+0.5-{cx})*(x+0.5-{cx})+(y+0.5-{cy})*(y+0.5-{cy}))"); let s2 = n(o.radius * o.radius);
    let fall = match o.falloff.as_str() {
        "gaussian" => format!("pow(2.7182818,0-{d2}/{})", n(2.0 * o.radius * o.radius)),
        "inverse_square" => format!("(1/(1+{d2}/{s2}))"),
        _ => format!("(1-smoothstep(0,{},sqrt({d2})))", n(o.radius)),
    };
    let g = format!("({}*{fall})", n(o.intensity));
    let ch = |r: &str, c: f64| if combine == "screen" { format!("(1-(1-{r})*(1-min({}*{g},1)))", n(c)) } else { format!("({r}+{}*{g})", n(c)) };
    format!("vec4({},{},{},a)", ch("r", o.color[0]), ch("g", o.color[1]), ch("b", o.color[2]))
}
/// Dither: +-amp LSB of seeded noise on every channel. amp 0 means no effect at all (bit-identical).
pub(crate) fn dither_expression(amp: f64, seed: u64) -> String {
    // TPDF dither, +-amp LSB peak (2 LSB peak-to-peak at amp 1), zero mean: the sum of two independent uniform draws minus 1.
    // Each draw hashes (seed, frame time) first, so every frame gets a fresh pattern and the same frame repeats bit for bit.
    let s = n((seed % 65521) as f64); let k = n(amp / 255.0);
    let d = format!("((noise(noise({s},t,1),x,y)+noise(noise({s},t,2),x,y)-1)*{k})");
    format!("vec4(r+{d},g+{d},b+{d},a)")
}
fn rgb(v: Option<&Value>, d: [f64; 3], what: &str, call: &str) -> Result<[f64; 3], String> {
    let Some(v) = v.filter(|v| !v.is_null()) else { return Ok(d) };
    let arr = v.as_array().filter(|x| x.len() == 3).ok_or_else(|| bg_err(E_RANGE, &format!("{what} must be [r, g, b] with each 0..1 (got {v})"), call))?;
    let mut o = [0.0; 3]; for (i, c) in arr.iter().enumerate() { o[i] = c.as_f64().filter(|f| f.is_finite() && (0.0..=1.0).contains(f)).ok_or_else(|| bg_err(E_RANGE, &format!("{what}[{i}] must be a number 0..1 (got {c})"), call))?; } Ok(o)
}
fn parse_orb(i: usize, v: &Value) -> Result<Orb, String> {
    let call = |s: &str| format!("bg_orbs(orbs=[{{{s}}}])"); let w = format!("orbs[{i}]");
    let o = v.as_object().ok_or_else(|| bg_err(E_RANGE, &format!("{w} must be an object with cx, cy, radius"), &call("cx:960,cy:540,radius:200")))?;
    for k in o.keys() { if !ORB_KEYS.contains(&k.as_str()) { return Err(bg_err(super::bgcommon::E_UNKNOWN, &format!("unknown orb property '{k}' in {w}. Valid: {}", ORB_KEYS.join(", ")), &call("cx:960,cy:540,radius:200"))); } }
    for k in ["cx", "cy", "radius"] { if o.get(k).map_or(true, |x| x.is_null()) { return Err(bg_err(E_RANGE, &format!("{w}.{k} is required"), &call("cx:960,cy:540,radius:200"))); } }
    let f = |k: &str, d: f64, lo: f64, hi: f64| num(v, k, d, lo, hi, &call(&format!("{k}={d}")));
    let (cx, cy, radius, intensity) = (f("cx", 0.0, -1e6, 1e6)?, f("cy", 0.0, -1e6, 1e6)?, f("radius", 1.0, 1.0, 8192.0)?, f("intensity", 1.0, 0.0, 4.0)?);
    let color = rgb(o.get("color"), [1.0, 1.0, 1.0], &format!("{w}.color"), &call("color:[1,0.6,0.2]"))?;
    let falloff = match o.get("falloff").filter(|x| !x.is_null()) { None => "gaussian".to_string(), Some(x) => x.as_str().filter(|s| FALLOFFS.contains(s)).ok_or_else(|| bg_err(E_RANGE, &format!("{w}.falloff must be one of {} (got {x})", FALLOFFS.join(", ")), &call("falloff:\"gaussian\"")))?.to_string() };
    let has_orbit = o.get("orbit").map_or(false, |x| !x.is_null()); let has_keys = o.get("keys").map_or(false, |x| !x.is_null());
    if has_orbit && has_keys { return Err(bg_err(E_RANGE, &format!("{w}: orbit and keys are mutually exclusive"), &call("orbit:{rx:100,ry:60,rate_hz:0.2,phase:0}"))); }
    let orbit = if has_orbit { let ob = &o["orbit"]; let oc = |s: &str| call(&format!("orbit:{{{s}}}"));
        let ok = ob.as_object().ok_or_else(|| bg_err(E_RANGE, &format!("{w}.orbit must be {{rx, ry, rate_hz, phase}}"), &oc("rx:100,ry:60,rate_hz:0.2,phase:0")))?;
        for k in ok.keys() { if !["rx", "ry", "rate_hz", "phase"].contains(&k.as_str()) { return Err(bg_err(super::bgcommon::E_UNKNOWN, &format!("unknown orbit property '{k}' in {w}. Valid: rx, ry, rate_hz, phase"), &oc("rx:100,ry:60,rate_hz:0.2,phase:0"))); } }
        let g = |k: &str, d: f64, lo: f64, hi: f64| num(ob, k, d, lo, hi, &oc(&format!("{k}={d}")));
        Some([g("rx", 0.0, -1e6, 1e6)?, g("ry", 0.0, -1e6, 1e6)?, g("rate_hz", 0.0, -20.0, 20.0)?, g("phase", 0.0, -1e4, 1e4)?]) } else { None };
    let layer = match o.get("layer").filter(|x| !x.is_null()) { None => "mid".to_string(), Some(x) => x.as_str().filter(|s| LAYERS.contains(s)).ok_or_else(|| bg_err(E_RANGE, &format!("{w}.layer must be one of {} (got {x})", LAYERS.join(", ")), &call("layer:\"far\"")))?.to_string() };
    let (rm, im, pm) = layer_mults(&layer); let radius = (radius * rm).clamp(1.0, 8192.0); let intensity = (intensity * im).min(4.0); let orbit = orbit.map(|[rx, ry, hz, ph]| [rx * pm, ry * pm, hz, ph]);
    // aerial perspective: a far orb sits behind more air, so its colour is pulled 30% toward a cool haze (mid and near are unchanged)
    let color = if layer == "far" { let hz = [0.45, 0.5, 0.62]; [color[0] * 0.7 + hz[0] * 0.3, color[1] * 0.7 + hz[1] * 0.3, color[2] * 0.7 + hz[2] * 0.3] } else { color };
    let falloff = if layer == "far" && o.get("falloff").map_or(true, |x| x.is_null()) { "inverse_square".to_string() } else { falloff };
    let mut keys = vec![];
    if has_keys { let arr = o["keys"].as_array().ok_or_else(|| bg_err(E_RANGE, &format!("{w}.keys must be an array of {{t, cx, cy}}"), &call("keys:[{t:0,cx:0,cy:0},{t:2,cx:500,cy:300}]")))?;
        if arr.len() < 2 || arr.len() > 64 { return Err(bg_err(E_RANGE, &format!("{w}.keys needs 2..64 entries (got {})", arr.len()), &call("keys:[{t:0,cx:0,cy:0},{t:2,cx:500,cy:300}]"))); }
        for k in arr { let kc = |s: &str| call(&format!("keys:[{{{s}}}]")); if let Some(m) = k.as_object() { for kk in m.keys() { if !["t", "cx", "cy"].contains(&kk.as_str()) { return Err(bg_err(super::bgcommon::E_UNKNOWN, &format!("unknown key property '{kk}' in {w}.keys. Valid: t, cx, cy"), &kc("t:0,cx:0,cy:0"))); } } }
            keys.push([num(k, "t", 0.0, 0.0, 3600.0, &kc("t=0"))?, num(k, "cx", 0.0, -1e6, 1e6, &kc("cx=0"))?, num(k, "cy", 0.0, -1e6, 1e6, &kc("cy=0"))?]); }
        keys.sort_by(|a, b| a[0].total_cmp(&b[0])); }
    Ok(Orb { cx, cy, radius, color, intensity, falloff, orbit, keys, layer })
}
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let c = Common::parse(ss, a, "bg_orbs", "orbs", OWN)?; let call = |s: &str| format!("bg_orbs({s})");
    let arr = a.get("orbs").and_then(|v| v.as_array()).ok_or_else(|| bg_err(E_RANGE, "orbs is required: an array of 1..16 orbs {cx, cy, radius, color}", &call("orbs=[{cx:960,cy:540,radius:300,color:[1,0.5,0.2]}]")))?;
    if arr.is_empty() { return Err(bg_err(E_RANGE, "orbs needs at least 1 orb", &call("orbs=[{cx:960,cy:540,radius:300}]"))); }
    if arr.len() > 16 { return Err(bg_err(E_ORBS, &format!("orbs has {} entries, the maximum is 16", arr.len()), &call("orbs=[...16 or fewer]"))); }
    let mut orbs = vec![]; for (i, v) in arr.iter().enumerate() { orbs.push(parse_orb(i, v)?); }
    let keyed = orbs.iter().filter(|o| !o.keys.is_empty()).count();
    if keyed > 4 { return Err(bg_err(E_KEYCAP, &format!("keys are allowed on at most 4 orbs (got {keyed})"), &call("orbs=[...keys on 4 orbs or fewer, use orbit for the rest]"))); }
    let combine = match a.get("combine").filter(|v| !v.is_null()) { None => "add", Some(v) => v.as_str().filter(|s| ["add", "screen"].contains(s)).ok_or_else(|| bg_err(E_RANGE, &format!("combine must be add or screen (got {v})"), &call("combine=\"add\"")))? };
    let dither = num(a, "dither", 0.5, 0.0, 2.0, &call("dither=0.5"))?;
    let vignette = num(a, "vignette", 0.25, 0.0, 1.0, &call("vignette=0.25"))?; let grain = num(a, "grain", 0.02, 0.0, 0.5, &call("grain=0.02"))?; let ceiling = num(a, "ceiling", 0.92, 0.5, 1.0, &call("ceiling=0.92"))?;
    let (base_kind, c1, c2) = match a.get("base").filter(|v| !v.is_null()) { None => ("solid".to_string(), [0.02, 0.02, 0.04], [0.02, 0.02, 0.04]), Some(b) => {
        let bc = |s: &str| call(&format!("base={{{s}}}")); let o = b.as_object().ok_or_else(|| bg_err(E_RANGE, "base must be {type, c1, c2}", &bc("type:\"solid\",c1:[0.02,0.02,0.04]")))?;
        for k in o.keys() { if !["type", "c1", "c2"].contains(&k.as_str()) { return Err(bg_err(super::bgcommon::E_UNKNOWN, &format!("unknown base property '{k}'. Valid: type, c1, c2"), &bc("type:\"solid\",c1:[0.02,0.02,0.04]"))); } }
        let k = match o.get("type").filter(|v| !v.is_null()) { None => "solid", Some(t) => t.as_str().filter(|s| ["solid", "gradient"].contains(s)).ok_or_else(|| bg_err(E_RANGE, &format!("base.type must be solid or gradient (got {t})"), &bc("type:\"solid\"")))? };
        let c1 = rgb(o.get("c1"), [0.02, 0.02, 0.04], "base.c1", &bc("c1:[0.02,0.02,0.04]"))?; let c2 = rgb(o.get("c2"), c1, "base.c2", &bc("c2:[0.0,0.0,0.0]"))?; (k.to_string(), c1, c2) } };
    orbs.sort_by(|x, y| x.sort_key().cmp(&y.sort_key()));
    let cap = crate::resource_limits::current().expr_max_ops.unwrap_or(u64::MAX).min(usize::MAX as u64) as usize;
    let mut exprs = vec![]; let (mut total, mut worst) = (0usize, 0usize);
    let mut all: Vec<String> = orbs.iter().map(|o| orb_expression(o, combine)).collect(); if dither > 0.0 { all.push(dither_expression(dither, c.seed)); }
    for e in &all { let ops = match crate::pixel_expr::compile(e, cap) { Ok(p) => p.ops, Err(m) if m.contains("expr_max_ops") => return Err(bg_err(E_EXPR_OPS, &format!("one orb expression needs more than the expr_max_ops limit of {cap}"), &format!("set_limits(expr_max_ops={})", cap * 2))), Err(m) => return Err(format!("internal: orb expression did not compile: {m}")) }; total += ops; worst = worst.max(ops); exprs.push(e.clone()); }
    let base_args = if base_kind == "solid" { json!({"track_id":c.track,"source_type":"solid","color":c1,"start":c.start,"duration":c.duration}) } else { json!({"track_id":c.track,"source_type":"gradient","color":c1,"color2":c2,"start":c.start,"duration":c.duration}) };
    let mut ops = vec![json!({"id":"bg","tool":"add_clip","args":base_args})]; let mut keyed_n = 0usize;
    for (i, o) in orbs.iter().enumerate() {
        let id = format!("o{i}"); let mut args = json!({"clip_id":"$bg.clip_id","kind":"pixel_expr","strings":{"expr":exprs[i]}});
        if !o.keys.is_empty() { args["params"] = json!({"p0":o.keys[0][1],"p1":o.keys[0][2]}); }
        ops.push(json!({"id":id,"tool":"add_effect","args":args}));
        for k in &o.keys { for (p, v) in [("p0", k[1]), ("p1", k[2])] { ops.push(json!({"tool":"set_keyframe","args":{"clip_id":"$bg.clip_id","effect_id":format!("${id}.effect_id"),"property":p,"time":k[0],"value":v}})); keyed_n += 1; } }
    }
    // finishing stack, in order: vignette (ambient falloff), grain (luma noise, seeded), shoulder LAST so nothing passes the ceiling
    if vignette > 0.0 { ops.push(json!({"tool":"add_effect","args":{"clip_id":"$bg.clip_id","kind":"vignette","params":{"amount":vignette,"softness":0.8}}})); }
    if grain > 0.0 { ops.push(json!({"tool":"add_effect","args":{"clip_id":"$bg.clip_id","kind":"grain","params":{"amount":grain}}})); }
    if ceiling < 1.0 { ops.push(json!({"tool":"add_effect","args":{"clip_id":"$bg.clip_id","kind":"shoulder","params":{"start":(ceiling - 0.12).max(0.0),"ceiling":ceiling}}})); }
    // dither is the last stage of the stack, after the shoulder, so the 8-bit quantiser sees noise that nothing else reshapes
    if dither > 0.0 { ops.push(json!({"id":"dither","tool":"add_effect","args":{"clip_id":"$bg.clip_id","kind":"pixel_expr","strings":{"expr":exprs[orbs.len()]}}})); }
    ops.extend(c.finish_ops("bg"));
    let params = { let mut p = c.echo(); p["orbs"] = json!(orbs.iter().map(|o| json!({"cx":o.cx,"cy":o.cy,"radius":o.radius,"color":o.color,"intensity":o.intensity,"falloff":o.falloff,"layer":o.layer,"orbit":o.orbit.map(|v| json!({"rx":v[0],"ry":v[1],"rate_hz":v[2],"phase":v[3]})),"keys":if o.keys.is_empty() { Value::Null } else { json!(o.keys.iter().map(|k| json!({"t":k[0],"cx":k[1],"cy":k[2]})).collect::<Vec<_>>()) }})).collect::<Vec<_>>());
        p["base"] = json!({"type":base_kind,"c1":c1,"c2":c2}); p["dither"] = json!(dither); p["vignette"] = json!(vignette); p["grain"] = json!(grain); p["ceiling"] = json!(ceiling); p["combine"] = json!(combine); p["order"] = json!("canonical: orbs are sorted before expansion, call order does not matter"); p };
    let next = if c.apply { vec![json!({"tool":"lint_frame","args":{"time":c.start,"annotate":false},"why":"check text or layers placed over the glow"})] } else { vec![] };
    bg::finish(ss, &c, params, ops, &["bg"], json!({"expr_ops": worst, "expr_ops_total": total, "expr_max_ops": cap, "effects": all.len(), "cost": "one pixel expression pass per orb per frame", "keys": keyed_n}), next)
}
pub fn commands() -> Vec<Command> {
    let mut props = bg::common_props(); props.extend([
        ("orbs", json!({"type":"array","minItems":1,"maxItems":16,"items":{"type":"object","description":"{cx, cy, radius 1..8192, color [r,g,b], intensity 0..4 default 1, layer far|mid|near default mid (mid = as given; far: radius x0.5, intensity x0.55, orbit x0.5, long-tail inverse_square falloff; near: radius x1.5, intensity x2.8, orbit x1.67, gaussian; with mid at 0.5 that is far 0.2, near 1.4; the echoed values are the multiplied ones), falloff gaussian|inverse_square|smooth, orbit {rx, ry, rate_hz -20..20, phase} or keys [{t, cx, cy}] (not both, keys on at most 4 orbs)}"},"description":"1 to 16 orbs"})),
        ("base", json!({"type":"object","description":"{type solid|gradient, c1, c2} default solid [0.02,0.02,0.04]"})), ("dither", json!({"type":"number","minimum":0,"maximum":2,"default":0.5,"description":"seeded noise amplitude in 8-bit steps, 0 disables"})), ("vignette", json!({"type":"number","minimum":0,"maximum":1,"default":0.25,"description":"ambient edge darkening, 0 disables"})), ("grain", json!({"type":"number","minimum":0,"maximum":0.5,"default":0.02,"description":"film grain amplitude, luma only, seeded per frame, 0 disables"})), ("ceiling", json!({"type":"number","minimum":0.5,"maximum":1,"default":0.92,"description":"highest value a channel can reach (highlight shoulder, 0.92 = 235 of 255); 1 disables the shoulder and lets stacked light clip to 255"})),
        ("combine", json!({"type":"string","enum":["add","screen"],"description":"how orbs add onto the base, default add"}))]);
    let mut m = serde_json::Map::new(); for (k, v) in props { m.insert(k.into(), v); }
    vec![Command{name:"bg_orbs",mutates:true,description:"Glowing orbs background: soft light orbs on a solid or gradient base, optionally orbiting or keyframed, with dither against banding. A pure expansion into add_clip, one add_effect pixel expression per orb and set_meta; apply false (default) returns the calls and changes nothing, apply true runs them as one undo step. Orbs are sorted into a canonical order, so call order never changes the result. Combine add pushes overlaps toward white, so use combine screen when several orbs overlap. The solid has no alpha: to overlay video use blend add or screen. Reports the expression op count against the expr_max_ops limit.",schema:json!({"type":"object","properties":m,"required":["track","duration","orbs"]}),run}]
}
