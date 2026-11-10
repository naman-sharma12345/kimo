//! Extension tools. Drop a file `src/ext/<name>.rs` exposing `pub fn commands() -> Vec<Command>` and it is compiled in and
//! registered automatically (see build.rs); no existing file needs to change. Helpers for writing commands live in `ext::kit`.
include!(concat!(env!("OUT_DIR"), "/ext_gen.rs"));
/// Small helpers for extension commands (schema builders and argument readers).
pub mod kit {
    pub use crate::commands::{Command, Session};
    pub use serde_json::{json, Value};
    /// Run a child with a deadline and an output cap (reaps the child on timeout); portable, no shell or coreutils needed.
    pub fn capture(cmd: &mut std::process::Command, max: usize, timeout: std::time::Duration) -> Result<Vec<u8>, String> { crate::color::process::capture(cmd, max, timeout) }
    pub fn s(desc: &str) -> Value { json!({"type":"string","description":desc}) }
    pub fn n(desc: &str) -> Value { json!({"type":"number","description":desc}) }
    pub fn obj(props: Vec<(&str, Value)>, req: &[&str]) -> Value { let mut m = serde_json::Map::new(); for (k, v) in props { m.insert(k.into(), v); } json!({"type":"object","properties":m,"required":req}) }
    pub fn str_arg<'a>(a: &'a Value, k: &str) -> Result<&'a str, String> { a.get(k).and_then(|v| v.as_str()).ok_or_else(|| crate::errors::arg_err(a, k, "string")) }
    pub fn num_arg(a: &Value, k: &str) -> Result<f64, String> { a.get(k).and_then(|v| v.as_f64()).ok_or_else(|| crate::errors::arg_err(a, k, "number")) }
    pub fn opt_num(a: &Value, k: &str, d: f64) -> f64 { a.get(k).and_then(|v| v.as_f64()).unwrap_or(d) }
}
