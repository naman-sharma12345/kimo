//! V1c-D plumbing shared by the background generators (bg_grid, bg_orbs, bg_wave): one table of the common arguments (the single source of truth for the
//! schema, the validation range and the echoed default), typed `bg_*` errors, unknown-property rejection, and one apply/undo-group wrapper.
//! Conventions (ARCH): canvas px origin top-left, colours [r,g,b] 0..1, times in seconds, every number finite, out of range is a typed error and is never clamped.
use super::kit::*;
use super::presentation::range;

pub const E_RANGE: &str = "bg_param_out_of_range";
pub const E_ORBS: &str = "bg_too_many_orbs";
pub const E_KEYCAP: &str = "bg_key_cap";
pub const E_AUDIO_RATE: &str = "bg_audio_rate";
pub const E_EXPR_OPS: &str = "bg_expr_ops_limit";
pub const E_UNKNOWN: &str = "bg_unknown_property";
pub const CODES: &[&str] = &[E_RANGE, E_ORBS, E_KEYCAP, E_AUDIO_RATE, E_EXPR_OPS, E_UNKNOWN];

/// One common argument: name, JSON type, numeric range (None = not numeric), default (None = required) and a one-line description.
pub struct ArgSpec { pub name: &'static str, pub ty: &'static str, pub min: Option<f64>, pub max: Option<f64>, pub default: Option<f64>, pub about: &'static str }
pub const COMMON: &[ArgSpec] = &[
    ArgSpec { name: "track", ty: "string", min: None, max: None, default: None, about: "track id or @name the background clips are added to (required)" },
    ArgSpec { name: "start", ty: "number", min: Some(0.0), max: Some(1e7), default: Some(0.0), about: "timeline seconds, >= 0, default 0" },
    ArgSpec { name: "duration", ty: "number", min: Some(1e-3), max: Some(3600.0), default: None, about: "seconds, > 0 and <= 3600 (required)" },
    ArgSpec { name: "name", ty: "string", min: None, max: None, default: None, about: "optional clip name, 1..64 characters of A-Z a-z 0-9 _ . -" },
    ArgSpec { name: "tags", ty: "array", min: None, max: None, default: None, about: "optional tags, at most 8, each 1..32 characters of A-Z a-z 0-9 _ : . - (bg:<kind> is always added)" },
    ArgSpec { name: "seed", ty: "integer", min: Some(0.0), max: Some(9007199254740991.0), default: Some(0.0), about: "integer 0..2^53-1, default 0" },
    ArgSpec { name: "apply", ty: "boolean", min: None, max: None, default: None, about: "default false: return the expansion only; true runs it as one undo group" },
    ArgSpec { name: "blend", ty: "string", min: None, max: None, default: None, about: "normal (default), add, screen or multiply" },
    ArgSpec { name: "opacity", ty: "number", min: Some(0.0), max: Some(1.0), default: Some(1.0), about: "layer opacity 0..1, default 1" },
];
pub const BLENDS: &[&str] = &["normal", "add", "screen", "multiply"];
pub fn spec(name: &str) -> &'static ArgSpec { COMMON.iter().find(|a| a.name == name).expect("common arg") }
/// Schema properties for the common arguments, generated from COMMON so the schema, the validation and the echoed defaults cannot drift.
pub fn common_props() -> Vec<(&'static str, Value)> {
    COMMON.iter().map(|a| {
        let mut p = json!({"type": a.ty, "description": a.about});
        if let Some(m) = a.min { p["minimum"] = json!(m); } if let Some(m) = a.max { p["maximum"] = json!(m); } if let Some(d) = a.default { p["default"] = json!(d); }
        if a.name == "blend" { p["enum"] = json!(BLENDS); p["default"] = json!("normal"); }
        if a.name == "apply" { p["default"] = json!(false); }
        if a.name == "tags" { p["items"] = json!({"type": "string"}); p["maxItems"] = json!(8); }
        (a.name, p)
    }).collect()
}
/// Typed error text: `<code>: <message>; fix_hint: try <call>`. errors::classify maps the prefix to the code.
pub fn bg_err(code: &str, msg: &str, try_call: &str) -> String { format!("{code}: {msg}; fix_hint: try {try_call}") }
/// Typed range check with the bg code. `d` is the default used when the key is absent or null.
pub fn num(a: &Value, k: &str, d: f64, lo: f64, hi: f64, try_call: &str) -> Result<f64, String> {
    match range(a, k, d, lo, hi, try_call) { Ok(v) => Ok(v), Err(e) => Err(bg_err(E_RANGE, e.split_once(": ").map(|x| x.1).unwrap_or(&e).split("; fix_hint").next().unwrap_or(&e), try_call)) }
}
/// Reject any property that is neither common nor in `own`, listing the valid names (typed bg_unknown_property).
pub fn reject_unknown(a: &Value, tool: &str, own: &[&str]) -> Result<(), String> {
    let Some(o) = a.as_object() else { return Err("arguments must be an object".into()) };
    for k in o.keys() {
        if !COMMON.iter().any(|c| c.name == k) && !own.contains(&k.as_str()) {
            let mut valid: Vec<&str> = COMMON.iter().map(|c| c.name).chain(own.iter().copied()).collect(); valid.sort_unstable();
            return Err(bg_err(E_UNKNOWN, &format!("unknown property '{k}' for {tool}. Valid: {}", valid.join(", ")), &format!("{tool} without '{k}'")));
        }
    }
    Ok(())
}
fn name_ok(s: &str, extra: &[char], max: usize) -> bool { !s.is_empty() && s.chars().count() <= max && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' || extra.contains(&c)) }
#[derive(Debug, Clone)]
pub struct Common { pub track: String, pub start: f64, pub duration: f64, pub name: Option<String>, pub tags: Vec<String>, pub seed: u64, pub apply: bool, pub blend: String, pub opacity: f64, pub kind: String }
impl Common {
    /// Validate the common arguments of tool `tool` (kind = the bg:<kind> tag). `own` lists the tool's own property names for unknown-property rejection.
    pub fn parse(ss: &Session, a: &Value, tool: &str, kind: &str, own: &[&str]) -> Result<Common, String> {
        reject_unknown(a, tool, own)?;
        let call = |s: &str| format!("{tool}({s})");
        let track = a.get("track").and_then(|v| v.as_str()).ok_or_else(|| "missing string 'track'; fix_hint: pass the track id or @name the background goes on".to_string())?;
        let track = super::names::resolve_ref(&ss.project, "track", track)?;
        let start = num(a, "start", 0.0, 0.0, 1e7, &call("start=0"))?;
        if a.get("duration").map_or(true, |v| v.is_null()) { return Err("missing number 'duration'; fix_hint: pass duration in seconds, > 0 and <= 3600".into()); }
        let duration = num(a, "duration", 0.0, 1e-3, 3600.0, &call("duration=10"))?;
        let name = match a.get("name") { None | Some(Value::Null) => None, Some(v) => { let s = v.as_str().filter(|s| name_ok(s, &[], 64)).ok_or_else(|| bg_err(E_RANGE, &format!("name must be 1..64 characters of A-Z a-z 0-9 _ . - (got {v})"), &call("name=\"bg1\"")))?; Some(s.to_string()) } };
        let mut tags: Vec<String> = vec![];
        if let Some(v) = a.get("tags").filter(|v| !v.is_null()) {
            let arr = v.as_array().ok_or_else(|| bg_err(E_RANGE, "tags must be an array of strings", &call("tags=[\"hero\"]")))?;
            if arr.len() > 8 { return Err(bg_err(E_RANGE, &format!("tags has {} entries, the maximum is 8", arr.len()), &call("tags=[...8 or fewer]"))); }
            for t in arr { let s = t.as_str().filter(|s| name_ok(s, &[':'], 32)).ok_or_else(|| bg_err(E_RANGE, &format!("each tag must be 1..32 characters of A-Z a-z 0-9 _ : . - (got {t})"), &call("tags=[\"hero\"]")))?; tags.push(s.to_string()); }
        }
        let auto = format!("bg:{kind}"); if !tags.contains(&auto) { tags.push(auto); }
        let seed = match a.get("seed") { None | Some(Value::Null) => 0u64, Some(v) => {
            let f = v.as_f64().filter(|f| f.is_finite() && *f >= 0.0 && *f <= 9007199254740991.0 && f.fract() == 0.0).ok_or_else(|| bg_err(E_RANGE, &format!("seed must be an integer 0..9007199254740991 (got {v})"), &call("seed=0")))?; f as u64 } };
        let apply = match a.get("apply") { None | Some(Value::Null) => false, Some(v) => v.as_bool().ok_or("argument 'apply' must be a boolean")? };
        let blend = match a.get("blend") { None | Some(Value::Null) => "normal".to_string(), Some(v) => { let s = v.as_str().filter(|s| BLENDS.contains(s)).ok_or_else(|| bg_err(E_RANGE, &format!("blend is not allowed (got {v}). Valid: {}", BLENDS.join(", ")), &call("blend=\"normal\"")))?; s.to_string() } };
        let opacity = num(a, "opacity", 1.0, 0.0, 1.0, &call("opacity=1"))?;
        Ok(Common { track, start, duration, name, tags, seed, apply, blend, opacity, kind: kind.to_string() })
    }
    /// Every common value resolved, for the params echo.
    pub fn echo(&self) -> Value { json!({"track": self.track, "start": self.start, "duration": self.duration, "name": self.name, "tags": self.tags, "seed": self.seed, "apply": self.apply, "blend": self.blend, "opacity": self.opacity}) }
    /// Ops that finish a clip created by op `id`: opacity, blend, name and tags (only what differs from the defaults, plus the always-present tag).
    pub fn finish_ops(&self, id: &str) -> Vec<Value> {
        let r = format!("${id}.clip_id"); let mut v = vec![];
        if self.opacity != 1.0 { v.push(json!({"tool": "set_property", "args": {"clip_id": r, "property": "opacity", "value": self.opacity}})); }
        if self.blend != "normal" { v.push(json!({"tool": "set_blend", "args": {"clip_id": r, "mode": self.blend}})); }
        let mut m = json!({"id": r, "tags": self.tags}); if let Some(n) = &self.name { m["name"] = json!(n); }
        v.push(json!({"tool": "set_meta", "args": m})); v
    }
}
/// The one apply path: dry run returns the expansion untouched (project unchanged, no history); apply runs the same ops as ONE atomic undo group.
/// `clip_ops` names the ops whose clip_id is reported. Return shape (ARCH): {expansion, params, estimate, clip_ids|null, applied, next_steps}.
pub fn finish(ss: &mut Session, c: &Common, params: Value, ops: Vec<Value>, clip_ops: &[&str], estimate: Value, next_steps: Vec<Value>) -> Result<Value, String> {
    if !c.apply { return Ok(json!({"applied": false, "no_history": true, "expansion": ops, "params": params, "estimate": estimate, "clip_ids": null, "next_steps": next_steps})); }
    let r = crate::commands::dispatch_nested(ss, "batch", &json!({"ops": ops, "atomic": true, "return": "full"}))?;
    if r["ok"] != true { return Err(format!("batch_failed: {}", r["failed"])); }
    let res = r["results"].as_array().ok_or("batch returned no results")?;
    let ids: Vec<Value> = clip_ops.iter().filter_map(|op| res.iter().find(|x| x["id"] == *op).and_then(|x| x["result"]["clip_id"].as_str()).map(|s| json!(s))).collect();
    if ids.len() != clip_ops.len() { return Err("internal: a background clip was not created".into()); }
    Ok(json!({"applied": true, "no_history": false, "expansion": ops, "params": params, "estimate": estimate, "clip_ids": ids, "next_steps": next_steps}))
}
pub fn commands() -> Vec<Command> { vec![] }
