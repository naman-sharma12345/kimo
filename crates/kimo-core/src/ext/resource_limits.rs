use super::kit::*;
use crate::resource_limits::Limits;
fn report(ss: &Session) -> Value {
    let env: Vec<&str> = [
        "KIMO_EXPR_MAX_OPS",
        "KIMO_EXPR_MAX_PIXEL_OPS",
        "KIMO_EXPR_TIMEOUT_MS",
        "KIMO_MEM_MB",
        "KIMO_MEMORY_BUDGET_BYTES",
        "KIMO_MAX_PIXELS",
        "KIMO_MAX_SAMPLES",
    ]
    .into_iter()
    .filter(|k| std::env::var(k).is_ok())
    .collect();
    json!({"limits":ss.resource_limits,"defaults":Limits::defaults(),"env_overridden":env,"memory_ceiling":crate::resource_limits::memory_ceiling(),"memory_bytes":{"requested":ss.resource_limits.memory_bytes,"effective":ss.resource_limits.memory_bytes.min(crate::guard::BUDGET.load(std::sync::atomic::Ordering::Relaxed) as u64),"process_budget":crate::guard::BUDGET.load(std::sync::atomic::Ordering::Relaxed),"source":if std::env::var("KIMO_MEMORY_BUDGET_BYTES").is_ok(){"env"}else{"default"},"note":"session set_limits can only lower process memory budget; raise startup KIMO_MEMORY_BUDGET_BYTES"}})
}
fn set(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let mut l = ss.resource_limits;
    let defaults = Limits::startup();
    let unlimited = a
        .get("unlimited")
        .map(|v| v.as_bool().ok_or("unlimited must be boolean"))
        .transpose()?
        .unwrap_or(false);
    let listed = ["expr_max_ops", "expr_max_pixel_ops", "expr_timeout_ms"];
    for k in a.as_object().ok_or("limits must be object")?.keys() {
        if ![
            "expr_max_ops",
            "expr_max_pixel_ops",
            "expr_timeout_ms",
            "memory_bytes",
            "max_pixels",
            "max_samples",
            "unlimited",
        ]
        .contains(&k.as_str())
        {
            return Err(format!("unknown limit '{k}'"));
        }
    }
    for (k, dest, reset) in [
        ("expr_max_ops", &mut l.expr_max_ops, defaults.expr_max_ops),
        (
            "expr_max_pixel_ops",
            &mut l.expr_max_pixel_ops,
            defaults.expr_max_pixel_ops,
        ),
        (
            "expr_timeout_ms",
            &mut l.expr_timeout_ms,
            defaults.expr_timeout_ms,
        ),
    ] {
        if let Some(v) = a.get(k) {
            *dest = if unlimited {
                None
            } else if v.is_null() {
                reset
            } else {
                Some(
                    v.as_u64()
                        .filter(|v| *v > 0)
                        .ok_or_else(|| format!("{k} must be positive integer|null"))?,
                )
            }
        }
    }
    if unlimited && !listed.iter().any(|k| a.get(*k).is_some()) {
        return Err("unlimited:true requires listed expr limits only".into());
    }
    for (k, dest, reset) in [
        ("memory_bytes", &mut l.memory_bytes, defaults.memory_bytes),
        ("max_pixels", &mut l.max_pixels, defaults.max_pixels),
        ("max_samples", &mut l.max_samples, defaults.max_samples),
    ] {
        if let Some(v) = a.get(k) {
            if unlimited {
                return Err(format!("unlimited not allowed for {k}"));
            }
            *dest = if v.is_null() {
                reset
            } else {
                v.as_u64()
                    .filter(|v| *v > 0)
                    .ok_or_else(|| format!("{k} must be positive integer|null"))?
            };
            if k == "memory_bytes" && *dest > crate::resource_limits::memory_ceiling() {
                return Err(format!("memory_bytes exceeds hard ceiling {} (physical RAM minus 1 GiB); set_limits memory_bytes",crate::resource_limits::memory_ceiling()));
            }
        }
    }
    ss.resource_limits = l;
    Ok(report(ss))
}
pub fn commands() -> Vec<Command> {
    let props = vec![
        (
            "expr_max_ops",
            json!({"description":"per sample operation cap, int|null reset"}),
        ),
        (
            "expr_max_pixel_ops",
            json!({"description":"per frame total cap, int|null reset"}),
        ),
        (
            "expr_timeout_ms",
            json!({"description":"render time cap milliseconds, int|null reset"}),
        ),
        (
            "memory_bytes",
            json!({"description":"session allocation budget, int|null reset, no unlimited"}),
        ),
        (
            "max_pixels",
            json!({"description":"read_pixels cap, int|null reset"}),
        ),
        (
            "max_samples",
            json!({"description":"sample cap, int|null reset"}),
        ),
        ("unlimited", json!({"type":"boolean"})),
    ];
    vec![Command{name:"set_limits",mutates:false,description:"Set session resource limits, not saved and not undoable. Positive integer limits; null resets to startup defaults. unlimited:true applies only to listed expr limits. memory_bytes cannot exceed physical RAM minus 1 GiB; no unlimited memory. Existing process KIMO memory/cancel guards also remain in force.",schema:obj(props,&[]),run:set},Command{name:"get_limits",mutates:false,description:"Read effective session limits, defaults, environment startup overrides and physical memory ceiling. Session set wins over startup environment. Limits are not in project or undo history.",schema:obj(vec![],&[]),run:|ss,_|Ok(report(ss))}]
}
