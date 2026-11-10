//! U2.5: `session_stats` - read-only call counter for this session, so call reduction is measured, never asserted.
use super::kit::*;
use crate::commands::Session;
use std::collections::BTreeMap;

pub struct Stats { pub per_tool: BTreeMap<String, (u64, u64, u64)>, pub since: u64 }
impl Default for Stats { fn default() -> Self { Stats { per_tool: BTreeMap::new(), since: now() } } }
fn now() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }
impl Stats { pub fn record(&mut self, tool: &str, bytes: usize, failed: bool) { if self.per_tool.get(tool).is_none() { self.per_tool.insert(tool.to_string(), (0, 0, 0)); } if let Some(e) = self.per_tool.get_mut(tool) { e.0 += 1; e.1 += bytes as u64; if failed { e.2 += 1; } } } }
struct Count(usize);
impl std::io::Write for Count { fn write(&mut self, b: &[u8]) -> std::io::Result<usize> { self.0 += b.len(); Ok(b.len()) } fn flush(&mut self) -> std::io::Result<()> { Ok(()) } }
/// Serialized size of a result without building the string.
pub fn json_len(v: &Value) -> usize { let mut c = Count(0); let _ = serde_json::to_writer(&mut c, v); c.0 }
fn session_stats(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let reset = match a.get("reset") { None | Some(Value::Null) => false, Some(v) => v.as_bool().ok_or("out_of_range: reset must be a boolean")? };
    let top = match a.get("top") { None | Some(Value::Null) => 50usize, Some(v) => v.as_u64().filter(|n| (1..=500).contains(n)).ok_or("out_of_range: top must be a whole number 1..500")? as usize };
    // this call is counted too, after it answers; report what was counted before it
    let mut rows: Vec<(&String, &(u64, u64, u64))> = ss.stats.per_tool.iter().collect(); rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
    let (calls, bytes, errors): (u64, u64, u64) = rows.iter().fold((0, 0, 0), |t, r| (t.0 + r.1 .0, t.1 + r.1 .1, t.2 + r.1 .2));
    let out = json!({"calls_total": calls, "bytes_returned_total": bytes, "errors_total": errors, "since_unix": ss.stats.since, "tools_used": rows.len(), "per_tool": rows.iter().take(top).map(|(n, v)| json!({"tool": n, "calls": v.0, "bytes": v.1, "errors": v.2})).collect::<Vec<_>>(), "truncated": rows.len() > top, "reset": reset, "note": "read-only counter of top-level calls in this session; calls made inside a batch count once, as the batch"});
    if reset { ss.stats = Stats::default(); }
    Ok(out)
}
pub fn commands() -> Vec<Command> { vec![
    Command { name: "session_stats", mutates: false, description: "Read-only call counter for this session: total top-level calls, bytes returned and errors, and the same per tool (most called first, top default 50). Calls made inside a batch count once, as the batch. reset:true returns the numbers and then starts a fresh count. Use it to measure how many calls and bytes a piece of work took.",
        schema: obj(vec![("reset", json!({"type":"boolean","description":"return the current numbers, then zero the counter"})), ("top", json!({"type":"integer","minimum":1,"maximum":500}))], &[]), run: session_stats },
] }
