//! RV-11 recipes: named, parameterised scenes that expand to ordinary tool calls and run as ONE atomic `batch`.
//! A recipe is a data table (params with types, ranges, enums, defaults) plus an `expand` function that returns plain ops. Recipes compose existing
//! primitives only. User-supplied text (title, label...) is only ever placed into the designated text params of `add_text`; it is never turned into tool
//! names, ids or references, and it is never echoed into error hints.
use crate::ext::batch::Op;
use serde_json::{json, Map, Value};

pub mod diagram;
pub mod product;
pub mod stat;
pub mod title;

#[derive(Clone, Copy)]
pub enum Kind { Num { min: f64, max: f64 }, Text { max: usize }, Color, Enum(&'static [&'static str]), Bool, Asset, TextList { max_items: usize, max: usize } }
pub struct P { pub name: &'static str, pub kind: Kind, pub required: bool, pub default: Option<Value>, pub desc: &'static str, pub space: Option<&'static str> }
pub struct Recipe { pub name: &'static str, pub version: u32, pub summary: &'static str, pub keywords: &'static [&'static str], pub params: fn() -> Vec<P>, pub roles: &'static [&'static str], pub expand: fn(&Ctx) -> Result<Vec<Op>, String> }
/// What an expansion needs: validated params (defaults filled), canvas size, start time and where layers go.
pub struct Ctx { pub p: Map<String, Value>, pub w: f64, pub h: f64, pub start: f64, pub track: String }
impl Ctx {
    pub fn num(&self, k: &str) -> f64 { self.p.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) }
    pub fn text(&self, k: &str) -> String { esc(self.p.get(k).and_then(|v| v.as_str()).unwrap_or("")) }
    pub fn col(&self, k: &str) -> Value { self.p.get(k).cloned().unwrap_or(json!([1, 1, 1])) }
    pub fn has(&self, k: &str) -> bool { self.p.get(k).map_or(false, |v| !v.is_null() && v.as_str() != Some("")) }
}
/// Batch treats a whole-string "$name.field" as a reference; user text starting with $ must stay literal, so it is escaped as "$$".
pub fn esc(s: &str) -> String { if s.starts_with('$') { format!("${s}") } else { s.to_string() } }
pub fn op(id: &str, tool: &str, args: Value) -> Op { Op { id: id.into(), tool: tool.into(), args } }
pub fn kf(id: &str, clip: &str, prop: &str, time: f64, value: f64, ease: &str) -> Op { op(id, "set_keyframe", json!({"clip_id": format!("${clip}.clip_id"), "property": prop, "time": time, "value": value, "ease": ease})) }
pub fn sety(clip: &str, y: f64) -> Op { op(&format!("{clip}_y"), "set_property", json!({"clip_id": format!("${clip}.clip_id"), "property": "y", "value": y})) }
pub fn setx(clip: &str, x: f64) -> Op { op(&format!("{clip}_x"), "set_property", json!({"clip_id": format!("${clip}.clip_id"), "property": "x", "value": x})) }
pub const EASES: &[&str] = &["linear", "in", "out", "in_out", "out_quart", "out_expo", "out_back", "in_out_quint", "in_expo", "in_back", "in_out_back", "in_elastic", "out_elastic", "in_out_elastic", "in_bounce", "out_bounce", "in_out_bounce"]; // keyframe::EASE_NAMES minus hold

pub fn all() -> Vec<&'static Recipe> {
    use std::sync::OnceLock; static R: OnceLock<Vec<Recipe>> = OnceLock::new();
    R.get_or_init(|| vec![title::title_card(), stat::stat_counter(), product::product_reveal(), diagram::explainer_diagram()]).iter().collect()
}
pub fn find(name: &str) -> Option<&'static Recipe> { all().into_iter().find(|r| r.name == name) }

/// JSON schema for a recipe's params; `recipe_describe` returns it and `recipe_apply` validates against the same table.
pub fn schema(r: &Recipe) -> Value {
    let mut props = Map::new(); let mut req = vec![];
    for p in (r.params)() {
        let mut v = match p.kind { Kind::Num { min, max } => json!({"type": "number", "minimum": min, "maximum": max}), Kind::Text { max } => json!({"type": "string", "maxLength": max}), Kind::Color => json!({"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 4}),
            Kind::Enum(a) => json!({"type": "string", "enum": a}), Kind::Bool => json!({"type": "boolean"}), Kind::Asset => json!({"type": "string"}), Kind::TextList { max_items, max } => json!({"type": "array", "items": {"type": "string", "maxLength": max}, "minItems": 1, "maxItems": max_items}) };
        v["description"] = json!(p.desc); if let Some(d) = &p.default { v["default"] = d.clone(); } if let Some(sp) = p.space { v["space"] = json!(sp); } if p.required { req.push(p.name); }
        props.insert(p.name.into(), v);
    }
    json!({"type": "object", "properties": props, "required": req, "additionalProperties": false})
}
fn clean(s: &str) -> String { s.chars().filter(|c| !c.is_control()).take(40).collect() }
/// Validate `params` against the table, fill defaults. Errors name allowed values; user text is never echoed back (only its param name).
pub fn check_params(r: &Recipe, params: &Value) -> Result<Map<String, Value>, String> {
    let ps = (r.params)(); let empty = Map::new(); let given = match params { Value::Null => &empty, Value::Object(m) => m, _ => return Err("params must be an object".into()) };
    for k in given.keys() { if !ps.iter().any(|p| p.name == k) { return Err(format!("invalid_value: unknown param '{}' for recipe {}. Valid: {}", clean(k), r.name, ps.iter().map(|p| p.name).collect::<Vec<_>>().join(", "))); } }
    let mut out = Map::new();
    for p in &ps {
        let v = match given.get(p.name) { Some(v) if !v.is_null() => v.clone(), _ => { if p.required { return Err(format!("missing_arg: recipe {} needs param '{}' ({})", r.name, p.name, p.desc)); } if let Some(d) = &p.default { out.insert(p.name.into(), d.clone()); } continue; } };
        let bad = |what: &str| format!("invalid_value: param '{}' {what}", p.name);
        match p.kind {
            Kind::Num { min, max } => { let x = v.as_f64().ok_or_else(|| bad("must be a number"))?; if !x.is_finite() || x < min || x > max { return Err(bad(&format!("must be between {min} and {max}"))); } }
            Kind::Text { .. } | Kind::Asset => { let s = v.as_str().ok_or_else(|| bad("must be a string"))?; let m = if let Kind::Text { max } = p.kind { max } else { 200 }; if s.chars().count() > m { return Err(bad(&format!("is longer than {m} characters"))); } }
            Kind::Color => { let a = v.as_array().ok_or_else(|| bad("must be [r,g,b] or [r,g,b,a] with values 0..1"))?; if !(a.len() == 3 || a.len() == 4) || a.iter().any(|x| x.as_f64().map_or(true, |f| !(0.0..=1.0).contains(&f))) { return Err(bad("must be [r,g,b] or [r,g,b,a] with values 0..1")); } }
            Kind::Enum(allowed) => { let s = v.as_str().unwrap_or(""); if !allowed.contains(&s) { return Err(format!("invalid_value: param '{}' is not allowed. Valid: {}", p.name, allowed.join(", "))); } }
            Kind::Bool => { if !v.is_boolean() { return Err(bad("must be true or false")); } }
            Kind::TextList { max_items, max } => { let a = v.as_array().ok_or_else(|| bad("must be an array of strings"))?; if a.is_empty() || a.len() > max_items { return Err(bad(&format!("needs 1 to {max_items} items"))); } if a.iter().any(|x| x.as_str().map_or(true, |s| s.chars().count() > max)) { return Err(bad(&format!("items must be strings of at most {max} characters"))); } }
        }
        out.insert(p.name.into(), v);
    }
    Ok(out)
}
pub fn num(name: &'static str, min: f64, max: f64, def: f64, desc: &'static str) -> P { P { name, kind: Kind::Num { min, max }, required: false, default: Some(json!(def)), desc, space: None } }
pub fn px(name: &'static str, min: f64, max: f64, def: f64, desc: &'static str) -> P { P { space: Some("px, measured from the canvas centre (x right, y down)"), ..num(name, min, max, def, desc) } }
pub fn auto(name: &'static str, min: f64, max: f64, desc: &'static str) -> P { P { space: Some("px"), ..P { name, kind: Kind::Num { min, max }, required: false, default: None, desc, space: None } } }
pub fn text(name: &'static str, max: usize, required: bool, desc: &'static str) -> P { P { name, kind: Kind::Text { max }, required, default: None, desc, space: None } }
pub fn color(name: &'static str, def: Value, desc: &'static str) -> P { P { name, kind: Kind::Color, required: false, default: Some(def), desc, space: None } }
pub fn en(name: &'static str, a: &'static [&'static str], def: &str, desc: &'static str) -> P { P { name, kind: Kind::Enum(a), required: false, default: Some(json!(def)), desc, space: None } }
