//! kimo's door to the effects vendored from EffectCraft (crate `kimo_ec`, MIT OR Apache-2.0):
//! lists each one in kimo's effect catalogue under a snake_case kind (the effect's name: "Card Dance" -> card_dance),
//! maps kimo's numeric params onto its parameters, and runs it on a layer frame.
//!
//! Parameter mapping (names are the EffectCraft ids in snake_case):
//! - numbers, sliders, angles (degrees), percents (0..100 as in After Effects) and pixels (1080p px; scaled with the
//!   frame in current render quality) keep their units;
//! - checkboxes are 0/1, popups are the 0-based option index (options listed in the description);
//! - points are `<name>_x`, `<name>_y` as fractions of the layer (0.5, 0.5 = centre); 3D points add `<name>_z` in px;
//! - colours are `<name>_r`, `_g`, `_b` (0..1) and `_a` where the colour has alpha.
//! Layer parameters (another layer as a map or texture) use the layer itself; masks, paths, text and file parameters
//! keep their defaults.
use crate::effects::{Frame, Params};
use kimo_ec::{ParamUi, Value};
use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

type C = (&'static str, f64, &'static str);
type Entry = (&'static str, &'static str, &'static [C]);

/// How one kimo param feeds one EffectCraft parameter.
#[derive(Clone)]
enum Map { Num, Bool, Enum, PointX, PointY, PointZ, Col(usize) }
struct Bridged { spec: &'static kimo_ec::EffectSpec, params: Vec<(String, &'static str, Map)> }

fn leak(s: String) -> &'static str { Box::leak(s.into_boxed_str()) }
fn snake(s: &str) -> String {
    let mut o = String::new(); let mut prev_us = true; let ch: Vec<char> = s.chars().collect();
    for (i, &c) in ch.iter().enumerate() {
        // a capital starts a word after a lowercase letter or digit (matchingTolerance), not inside an acronym (HLS, LUT)
        if c.is_ascii_uppercase() && i > 0 && !prev_us && ch[i - 1].is_ascii_lowercase() { o.push('_'); }
        if c.is_ascii_alphanumeric() { o.push(c.to_ascii_lowercase()); prev_us = false; } else if !prev_us { o.push('_'); prev_us = true; }
    }
    o.trim_matches('_').to_string()
}
fn kind_of(name: &str) -> String { snake(&name.replace("4-Color", "Four Color").replace('&', "and").replace('+', "plus").replace("(", " ").replace(")", " ")).replace("__", "_") }

/// The bridged effects (kind -> effect), skipping any kind kimo already has.
fn table(existing: &BTreeSet<String>) -> &'static (Vec<Entry>, HashMap<&'static str, Bridged>) {
    static T: OnceLock<(Vec<Entry>, HashMap<&'static str, Bridged>)> = OnceLock::new();
    T.get_or_init(|| {
        let (mut cat, mut map) = (vec![], HashMap::new());
        for spec in kimo_ec::registry() {
            let kind = kind_of(spec.name); if kind.is_empty() || existing.contains(&kind) || map.contains_key(kind.as_str()) { continue; }
            let kind = leak(kind); let mut ps: Vec<C> = vec![]; let mut mp = vec![];
            let mut layered = false;
            for p in &spec.params {
                let base = snake(&p.id.replace('/', "_"));
                let range = |ui: &ParamUi| match ui { ParamUi::Slider { min, max, .. } => format!("{min}..{max}"), ParamUi::Percent => "0..100 %".into(), ParamUi::Angle => "degrees".into(), ParamUi::Pixels => "px".into(), _ => String::new() };
                match (&p.ui, &p.default) {
                    (ParamUi::Hidden, _) | (ParamUi::Mask, _) | (ParamUi::Path, _) | (ParamUi::Text, _) | (ParamUi::File { .. }, _) | (ParamUi::Gradient, _) => {}
                    (ParamUi::Layer, _) | (_, Value::Layer(_)) => layered = true,
                    (ParamUi::Popup { options }, v) => { ps.push((leak(base.clone()), v.as_f64(), leak(format!("{}: {}", p.name, options.iter().enumerate().map(|(i, o)| format!("{i} {o}")).collect::<Vec<_>>().join(", "))))); mp.push((base, p.id, Map::Enum)); }
                    (_, Value::Bool(b)) => { ps.push((leak(base.clone()), if *b { 1.0 } else { 0.0 }, leak(format!("{} (0/1)", p.name)))); mp.push((base, p.id, Map::Bool)); }
                    (_, Value::Vec2(v)) => { for (k, m, d) in [("x", Map::PointX, v[0]), ("y", Map::PointY, v[1])] { let n = format!("{base}_{k}"); ps.push((leak(n.clone()), d, leak(format!("{} {k} (0..1 of the layer)", p.name)))); mp.push((n, p.id, m)); } }
                    (_, Value::Vec3(v)) => { for (k, m, d) in [("x", Map::PointX, v[0]), ("y", Map::PointY, v[1]), ("z", Map::PointZ, v[2])] { let n = format!("{base}_{k}"); ps.push((leak(n.clone()), d, leak(format!("{} {k}{}", p.name, if k == "z" { " (px)" } else { " (0..1 of the layer)" })))); mp.push((n, p.id, m)); } }
                    (_, Value::Color(c)) => { for (i, k) in ["r", "g", "b", "a"].iter().enumerate() { if i == 3 && (c[3] - 1.0).abs() < 1e-9 { continue; } let n = format!("{base}_{k}"); ps.push((leak(n.clone()), c[i], leak(format!("{} {k} (0..1)", p.name)))); mp.push((n, p.id, Map::Col(i))); } }
                    (ui, Value::Scalar(v)) => { ps.push((leak(base.clone()), *v, leak(format!("{} {}", p.name, range(ui)).trim().to_string()))); mp.push((base, p.id, Map::Num)); }
                    (_, Value::Enum(e)) => { ps.push((leak(base.clone()), *e as f64, leak(p.name.to_string()))); mp.push((base, p.id, Map::Enum)); }
                    _ => {}
                }
            }
            // kimo param names must be unique per effect
            let mut seen = BTreeSet::new(); let keep: Vec<bool> = ps.iter().map(|c| seen.insert(c.0)).collect();
            let ps: Vec<C> = ps.into_iter().zip(&keep).filter(|(_, k)| **k).map(|(c, _)| c).collect(); let mp = mp.into_iter().zip(&keep).filter(|(_, k)| **k).map(|(m, _)| m).collect();
            let desc = leak(format!("{} (After Effects {}, from EffectCraft){}", spec.name, spec.category, if layered { ". Its layer input uses this layer itself" } else { "" }));
            cat.push((kind, desc, &*Box::leak(ps.into_boxed_slice())));
            map.insert(kind, Bridged { spec, params: mp });
        }
        (cat, map)
    })
}

/// Catalogue entries for the bridged effects not already named in `existing`.
pub fn catalog(existing: &BTreeSet<String>) -> Vec<Entry> { table(existing).0.clone() }

/// Run `kind` when it is a bridged effect.
pub fn apply(kind: &str, params: &Params, f: &mut Frame, t: f64, seed: u64) -> Option<Result<(), String>> {
    let _ = crate::color::catalog3(); // builds the table with kimo's own kinds excluded
    let (_, map) = table(&BTreeSet::new()); let b = map.get(kind)?;
    let (w, h) = (f.w, f.h); if w == 0 || h == 0 { return Some(Ok(())); }
    let s = crate::quality::px(1.0) as f64; let s = if s > 0.0 && s.is_finite() { s } else { 1.0 };
    let layer = [w as f64 / s, h as f64 / s];
    let mut vals: HashMap<String, Value> = b.spec.params.iter().map(|p| (p.id.to_string(), kimo_ec::default_value(p, layer))).collect();
    for (name, id, m) in &b.params {
        let Some(prop) = params.get(name) else { continue }; let x = prop.at(t);
        let v = vals.entry(id.to_string()).or_insert(Value::Scalar(0.0));
        *v = match (m, v.clone()) {
            (Map::Num, _) => Value::Scalar(x), (Map::Bool, _) => Value::Bool(x >= 0.5), (Map::Enum, _) => Value::Enum(x.round().max(0.0) as u32),
            (Map::PointX, Value::Vec2(c)) => Value::Vec2([x * layer[0], c[1]]), (Map::PointY, Value::Vec2(c)) => Value::Vec2([c[0], x * layer[1]]),
            (Map::PointX, Value::Vec3(c)) => Value::Vec3([x * layer[0], c[1], c[2]]), (Map::PointY, Value::Vec3(c)) => Value::Vec3([c[0], x * layer[1], c[2]]), (Map::PointZ, Value::Vec3(c)) => Value::Vec3([c[0], c[1], x]),
            (Map::Col(i), Value::Color(mut c)) => { c[*i] = x; Value::Color(c) }
            (_, old) => old,
        };
    }
    let prm = kimo_ec::Params { values: vals };
    let img = kimo_ec::Image { width: w as u32, height: h as u32, data: f.d.chunks_exact(4).map(|c| { let a = c[3].clamp(0.0, 1.0); [c[0] * a, c[1] * a, c[2] * a, a] }).collect() };
    let ctx = kimo_ec::EffectCtx { params: &prm, time: t, layer_size: layer, seed: (seed ^ (seed >> 32)) as u32, adjustment: false, env: Default::default() };
    let out = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| kimo_ec::apply(b.spec, &ctx, kimo_ec::Buf { img, offset: [0.0, 0.0], scale: s }))) { Ok(o) => o, Err(_) => return Some(Err(format!("effect {kind} failed on this frame"))) };
    // the effect may have grown its buffer (blur margins, displacement padding): read back the layer's own rectangle
    let (ox, oy) = (out.offset[0].round() as i64, out.offset[1].round() as i64); let k = out.scale / s;
    for y in 0..h { for x in 0..w {
        let px = if (k - 1.0).abs() < 1e-9 { out.img.get(x as i64 + ox, y as i64 + oy) } else { out.img.sample_bilinear((x as f64 + 0.5) * k + out.offset[0], (y as f64 + 0.5) * k + out.offset[1]) };
        let o = (y * w + x) * 4; let a = px[3]; if a > 1e-6 { for j in 0..3 { f.d[o + j] = px[j] / a; } } else { for j in 0..3 { f.d[o + j] = 0.0; } } f.d[o + 3] = a.clamp(0.0, 1.0); } }
    Some(Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_snake_case() { assert_eq!(kind_of("Card Dance"), "card_dance"); assert_eq!(kind_of("Color Balance (HLS)"), "color_balance_hls"); assert_eq!(kind_of("Black & White"), "black_and_white"); assert_eq!(snake("matchingTolerance"), "matching_tolerance"); assert_eq!(kind_of("Particle Emitter 2D"), "particle_emitter_2d"); assert_eq!(kind_of("3D Glasses"), "3d_glasses"); assert_eq!(kind_of("4-Color Gradient"), "four_color_gradient"); assert_eq!(kind_of("CC Ball Action"), "cc_ball_action"); }
}
