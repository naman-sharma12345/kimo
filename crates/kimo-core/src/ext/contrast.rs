//! v6 eyes item #3: contrast_ratio. WCAG 2.x contrast (L1 + 0.05) / (L2 + 0.05) between two colours or two pixel rects of the
//! frame at `time` (or one of each). Relative luminance L = 0.2126 R + 0.7152 G + 0.0722 B on linear RGB from the sRGB EOTF
//! (IEC 61966-2-1, 0.04045 knee); a rect's L is the mean of its per-pixel L. Optional colour-vision-deficiency simulation
//! with the Machado, Oliveira & Fernandes 2009 matrices (published supplementary tables, severity 0..1 in steps of 0.1,
//! linear interpolation between steps), applied to linear RGB and clamped to 0..1 before L. Raw numbers, no verdict.
use super::kit::*;
use super::flash_series::srgb_to_linear;

pub const CVD: [&str; 3] = ["protan", "deutan", "tritan"];
type M3 = [[f64; 3]; 3];
/// Machado et al. 2009, protanomaly severities 0.0, 0.1, ... 1.0
pub const PROTAN: [M3; 11] = [
    [[1.000000, 0.000000, -0.000000], [0.000000, 1.000000, 0.000000], [-0.000000, -0.000000, 1.000000]],
    [[0.856167, 0.182038, -0.038205], [0.029342, 0.955115, 0.015544], [-0.002880, -0.001563, 1.004443]],
    [[0.734766, 0.334872, -0.069637], [0.051840, 0.919198, 0.028963], [-0.004928, -0.004209, 1.009137]],
    [[0.630323, 0.465641, -0.095964], [0.069181, 0.890046, 0.040773], [-0.006308, -0.007724, 1.014032]],
    [[0.539009, 0.579343, -0.118352], [0.082546, 0.866121, 0.051332], [-0.007136, -0.011959, 1.019095]],
    [[0.458064, 0.679578, -0.137642], [0.092785, 0.846313, 0.060902], [-0.007494, -0.016807, 1.024301]],
    [[0.385450, 0.769005, -0.154455], [0.100526, 0.829802, 0.069673], [-0.007442, -0.022190, 1.029632]],
    [[0.319627, 0.849633, -0.169261], [0.106241, 0.815969, 0.077790], [-0.007025, -0.028051, 1.035076]],
    [[0.259411, 0.923008, -0.182420], [0.110296, 0.804340, 0.085364], [-0.006276, -0.034346, 1.040622]],
    [[0.203876, 0.990338, -0.194214], [0.112975, 0.794542, 0.092483], [-0.005222, -0.041043, 1.046265]],
    [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]],
];
/// Machado et al. 2009, deuteranomaly severities 0.0 .. 1.0
pub const DEUTAN: [M3; 11] = [
    [[1.000000, 0.000000, -0.000000], [0.000000, 1.000000, 0.000000], [-0.000000, -0.000000, 1.000000]],
    [[0.866435, 0.177704, -0.044139], [0.049567, 0.939063, 0.011370], [-0.003453, 0.007233, 0.996220]],
    [[0.760729, 0.319078, -0.079807], [0.090568, 0.889315, 0.020117], [-0.006027, 0.013325, 0.992702]],
    [[0.675425, 0.433850, -0.109275], [0.125303, 0.847755, 0.026942], [-0.007950, 0.018572, 0.989378]],
    [[0.605511, 0.528560, -0.134071], [0.155318, 0.812366, 0.032316], [-0.009376, 0.023176, 0.986200]],
    [[0.547494, 0.607765, -0.155259], [0.181692, 0.781742, 0.036566], [-0.010410, 0.027275, 0.983136]],
    [[0.498864, 0.674741, -0.173604], [0.205199, 0.754872, 0.039929], [-0.011131, 0.030969, 0.980162]],
    [[0.457771, 0.731899, -0.189670], [0.226409, 0.731012, 0.042579], [-0.011595, 0.034333, 0.977261]],
    [[0.422823, 0.781057, -0.203881], [0.245752, 0.709602, 0.044646], [-0.011843, 0.037423, 0.974421]],
    [[0.392952, 0.823610, -0.216562], [0.263559, 0.690210, 0.046232], [-0.011910, 0.040281, 0.971630]],
    [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.011820, 0.042940, 0.968881]],
];
/// Machado et al. 2009, tritanomaly severities 0.0 .. 1.0
pub const TRITAN: [M3; 11] = [
    [[1.000000, 0.000000, -0.000000], [0.000000, 1.000000, 0.000000], [-0.000000, -0.000000, 1.000000]],
    [[0.926670, 0.092514, -0.019184], [0.021191, 0.964503, 0.014306], [0.008437, 0.054813, 0.936750]],
    [[0.895720, 0.133330, -0.029050], [0.029997, 0.945400, 0.024603], [0.013027, 0.104707, 0.882266]],
    [[0.905871, 0.127791, -0.033662], [0.026856, 0.941251, 0.031893], [0.013410, 0.148296, 0.838294]],
    [[0.948035, 0.089490, -0.037526], [0.014364, 0.946792, 0.038844], [0.010853, 0.193991, 0.795156]],
    [[1.017277, 0.027029, -0.044306], [-0.006113, 0.958479, 0.047634], [0.006379, 0.248708, 0.744913]],
    [[1.104996, -0.046633, -0.058363], [-0.032137, 0.971635, 0.060503], [0.001336, 0.317922, 0.680742]],
    [[1.193214, -0.109812, -0.083402], [-0.058496, 0.979410, 0.079086], [-0.002346, 0.403492, 0.598854]],
    [[1.257728, -0.139648, -0.118081], [-0.078003, 0.975409, 0.102594], [-0.003316, 0.501214, 0.502102]],
    [[1.278864, -0.125333, -0.153531], [-0.084748, 0.957674, 0.127074], [-0.000989, 0.601151, 0.399838]],
    [[1.255528, -0.076749, -0.178779], [-0.078411, 0.930809, 0.147602], [0.004733, 0.691367, 0.303900]],
];

/// The Machado matrix for a type at severity 0..1 (linear interpolation between the published 0.1 steps).
pub fn machado(kind: &str, severity: f64) -> Option<M3> {
    let t = match kind { "protan" => &PROTAN, "deutan" => &DEUTAN, "tritan" => &TRITAN, _ => return None };
    if !(0.0..=1.0).contains(&severity) { return None; }
    let s = severity * 10.0; let lo = (s.floor() as usize).min(10); if lo == 10 { return Some(t[10]); }
    let f = s - lo as f64; let mut m = [[0.0; 3]; 3];
    for i in 0..3 { for j in 0..3 { m[i][j] = (1.0 - f) * t[lo][i][j] + f * t[lo + 1][i][j]; } } Some(m)
}
/// WCAG 2.x relative luminance of an encoded sRGB colour (0..1 per channel), optionally through a CVD matrix.
pub fn rel_lum(rgb: [f64; 3], m: Option<&M3>) -> f64 {
    let lin = [srgb_to_linear(rgb[0]), srgb_to_linear(rgb[1]), srgb_to_linear(rgb[2])];
    let l = match m { None => lin, Some(m) => { let mut o = [0.0; 3]; for i in 0..3 { o[i] = (m[i][0] * lin[0] + m[i][1] * lin[1] + m[i][2] * lin[2]).clamp(0.0, 1.0); } o } };
    0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2]
}
pub fn ratio(la: f64, lb: f64) -> f64 { let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) }; (hi + 0.05) / (lo + 0.05) }

enum Side { Color([f64; 3], String), Rect([usize; 4]) }
fn parse_hex(s: &str) -> Option<[f64; 3]> {
    let h = s.strip_prefix('#')?; if !h.chars().all(|c| c.is_ascii_hexdigit()) { return None; }
    let v: Vec<u8> = match h.len() { 3 => h.chars().map(|c| { let d = c.to_digit(16).unwrap() as u8; d * 17 }).collect(), 6 => (0..3).map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).unwrap()).collect(), _ => return None };
    Some([v[0] as f64 / 255.0, v[1] as f64 / 255.0, v[2] as f64 / 255.0])
}
fn side(a: &Value, k: &str) -> Result<Side, String> {
    let bad = || format!("out_of_range: {k} must be a colour (\"#rrggbb\", \"#rgb\" or [r,g,b] 0..1) or {{\"rect\": [x,y,w,h]}} in whole pixels");
    match a.get(k) {
        Some(Value::String(s)) => { let c = parse_hex(s).ok_or_else(bad)?; Ok(Side::Color(c, s.to_ascii_lowercase())) }
        Some(Value::Array(v)) => { if v.len() != 3 { return Err(bad()); } let c: Vec<f64> = v.iter().filter_map(|x| x.as_f64().filter(|x| x.is_finite() && (0.0..=1.0).contains(x))).collect(); if c.len() != 3 { return Err(bad()); }
            let hx = format!("#{:02x}{:02x}{:02x}", (c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8); Ok(Side::Color([c[0], c[1], c[2]], hx)) }
        Some(Value::Object(o)) => { if o.len() != 1 { return Err(bad()); } let r = o.get("rect").and_then(|v| v.as_array()).filter(|v| v.len() == 4).ok_or_else(bad)?;
            let mut q = [0usize; 4]; for (i, x) in r.iter().enumerate() { q[i] = x.as_f64().filter(|x| x.is_finite() && x.fract() == 0.0 && *x >= 0.0 && *x <= 65536.0).ok_or_else(bad)? as usize; }
            if q[2] == 0 || q[3] == 0 { return Err(format!("out_of_range: {k}.rect width and height must be at least 1")); } Ok(Side::Rect(q)) }
        _ => Err(bad()),
    }
}
fn contrast_ratio(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let (sa, sb) = (side(a, "a")?, side(a, "b")?);
    let kinds: Vec<&str> = match a.get("cvd").filter(|v| !v.is_null()) { None => vec![], Some(v) => {
        let arr = v.as_array().filter(|x| !x.is_empty() && x.len() <= 3).ok_or("out_of_range: cvd must be an array of 1..3 of protan, deutan, tritan")?; let mut out = vec![];
        for x in arr { let s = x.as_str().filter(|s| CVD.contains(s)).ok_or(format!("out_of_range: cvd entry {x} is not allowed. Valid: {}", CVD.join(", ")))?; if out.contains(&s) { return Err(format!("out_of_range: cvd lists {s} twice")); } out.push(s); }
        out } };
    let severity = match a.get("severity").filter(|v| !v.is_null()) { None => 1.0, Some(v) => { if kinds.is_empty() { return Err("out_of_range: severity applies to cvd; give cvd".into()); } v.as_f64().filter(|x| x.is_finite() && (0.0..=1.0).contains(x)).ok_or("out_of_range: severity must be a number in 0..1")? } };
    let mats: Vec<(&str, M3)> = kinds.iter().map(|k| (*k, machado(k, severity).unwrap())).collect();
    let need_frame = matches!(sa, Side::Rect(_)) || matches!(sb, Side::Rect(_));
    let time = match a.get("time").filter(|v| !v.is_null()) { None => None, Some(v) => Some(v.as_f64().filter(|t| t.is_finite() && *t >= 0.0).ok_or("out_of_range: time must be a number of seconds >= 0")?) };
    if need_frame && time.is_none() { return Err("out_of_range: a rect needs time (seconds)".into()); }
    if !need_frame && time.is_some() { return Err("out_of_range: time applies to a rect; both sides are colours".into()); }
    let frame = if let Some(t) = time { crate::render::prepare_at(&ss.project, &mut ss.cache, Some(t))?; Some(crate::framecache::render_cached(&mut ss.fcache, &ss.project, t, &ss.cache)?) } else { None };
    // per side: (description object, luminance, [luminance under each cvd matrix])
    let eval = |s: &Side, k: &str| -> Result<(Value, f64, Vec<f64>), String> { match s {
        Side::Color(c, hx) => { let l = rel_lum(*c, None); Ok((json!({"kind": "color", "hex": hx, "rgb": c, "luminance": l}), l, mats.iter().map(|(_, m)| rel_lum(*c, Some(m))).collect())) }
        Side::Rect(q) => { let f = frame.as_ref().unwrap(); let [x, y, w, h] = *q;
            if x + w > f.w || y + h > f.h { return Err(format!("out_of_range: {k}.rect [{x},{y},{w},{h}] must lie inside the {}x{} frame", f.w, f.h)); }
            let n = (w * h) as f64; let (mut ls, mut rgb) = (Vec::with_capacity(w * h), [0f64; 3]); let mut cv = vec![0f64; mats.len()];
            for yy in y..y + h { for xx in x..x + w { let i = (yy * f.w + xx) * 4; let c = [f.d[i].clamp(0.0, 1.0) as f64, f.d[i + 1].clamp(0.0, 1.0) as f64, f.d[i + 2].clamp(0.0, 1.0) as f64];
                for j in 0..3 { rgb[j] += c[j]; } ls.push(rel_lum(c, None)); for (m, acc) in mats.iter().zip(cv.iter_mut()) { *acc += rel_lum(c, Some(&m.1)); } } }
            let mean = ls.iter().sum::<f64>() / n; ls.sort_by(|a, b| a.total_cmp(b));
            let p = |q: f64| super::scopes_numeric::percentile_sorted(&ls, q);
            Ok((json!({"kind": "rect", "rect": q, "pixels": w * h, "mean_rgb": [rgb[0] / n, rgb[1] / n, rgb[2] / n], "luminance": mean, "luminance_p5": p(5.0), "luminance_p95": p(95.0), "luminance_min": ls[0], "luminance_max": ls[ls.len() - 1]}), mean, cv.iter().map(|v| v / n).collect())) } } };
    let (da, la, ca) = eval(&sa, "a")?; let (db, lb, cb) = eval(&sb, "b")?;
    let mut out = json!({"ratio": ratio(la, lb), "luminance_a": la, "luminance_b": lb, "lighter": if la >= lb { "a" } else { "b" }, "a": da, "b": db,
        "formula": "WCAG 2.x: (L_lighter + 0.05) / (L_darker + 0.05); L = 0.2126 R + 0.7152 G + 0.0722 B on linear RGB (sRGB EOTF, 0.04045 knee); a rect's L is the mean of its per-pixel L"});
    if let Some(t) = time { out["time"] = json!(t); }
    if !mats.is_empty() {
        let m = out.as_object_mut().unwrap(); m.insert("severity".into(), json!(severity));
        m.insert("cvd_model".into(), json!("Machado, Oliveira & Fernandes 2009 matrices (severity 0..1, linear interpolation between the published 0.1 steps) on linear RGB, clamped to 0..1"));
        let mut per = serde_json::Map::new();
        for (i, (k, _)) in mats.iter().enumerate() { m.insert(format!("ratio_{k}"), json!(ratio(ca[i], cb[i]))); per.insert(k.to_string(), json!({"ratio": ratio(ca[i], cb[i]), "luminance_a": ca[i], "luminance_b": cb[i]})); }
        m.insert("cvd".into(), Value::Object(per));
    }
    Ok(out)
}
pub fn commands() -> Vec<Command> { vec![Command { name: "contrast_ratio", mutates: false,
    description: "WCAG 2.x contrast ratio between two colours or two pixel rects of the frame at time, with each side's relative luminance. Optional cvd (protan, deutan, tritan) adds the same ratio under the Machado 2009 colour-vision simulation at severity 0..1 (default 1). A rect's luminance is the mean of its per-pixel relative luminance; p5 / p95 / min / max ride along. Raw numbers, no pass/fail.",
    schema: obj(vec![
        ("a", json!({"description": "\"#rrggbb\", \"#rgb\", [r,g,b] 0..1, or {\"rect\": [x,y,w,h]} whole pixels of the frame at time"})),
        ("b", json!({"description": "same forms as a"})),
        ("time", n("seconds; required when a or b is a rect")),
        ("cvd", json!({"type": "array", "items": {"type": "string", "enum": CVD}, "minItems": 1, "maxItems": 3})),
        ("severity", json!({"type": "number", "description": "0..1 (Machado 2009 steps of 0.1, interpolated), default 1; needs cvd"})),
    ], &["a", "b"]), run: contrast_ratio }] }
