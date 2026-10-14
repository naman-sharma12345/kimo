//! Raw point sampling, independent from render effects. No side effects or state.
use super::kit::*;
fn sample_noise(_: &mut Session, a: &Value) -> Result<Value, String> {
    let kind = crate::noise::Kind::parse(str_arg(a, "kind")?)?;
    let seed = a
        .get("seed")
        .and_then(Value::as_u64)
        .ok_or("seed must be a u64 integer")?;
    let pts = a
        .get("points")
        .and_then(Value::as_array)
        .ok_or("points must be array of [x,y]")?;
    let lim = crate::resource_limits::current();
    if pts.len() > 65536 || pts.len() as u64 > lim.max_samples {
        return Err("noise sample count exceeds min(65536,max_samples)".into());
    }
    if (pts.len() as u64).saturating_mul(256) > lim.expr_max_pixel_ops.unwrap_or(u64::MAX) {
        return Err("noise samples exceed expression work budget".into());
    }
    crate::resource_limits::reserve(
        pts.len()
            .checked_mul(256)
            .ok_or("noise result size overflow")?,
    )?;
    let start = std::time::Instant::now();
    let mut values = Vec::with_capacity(pts.len());
    for p in pts {
        crate::guard::check();
        if lim
            .expr_timeout_ms
            .map(|ms| start.elapsed().as_millis() > u128::from(ms))
            .unwrap_or(false)
        {
            return Err("noise samples timed out".into());
        }
        let q = p
            .as_array()
            .filter(|v| v.len() == 2)
            .ok_or("every point must be [x,y]")?;
        let x = q[0].as_f64().ok_or("x must be numeric")?;
        let y = q[1].as_f64().ok_or("y must be numeric")?;
        let v = crate::noise::sample(kind, seed, x, y)?;
        values.push(json!({"value":v.value,"gradient":v.gradient}));
    }
    Ok(
        json!({"samples":values,"absolute_value_bound":kind.bound(),"gradient_component_bound":32.0,"coordinate_limit":crate::noise::COORD_LIMIT,"normalization":"none; simplex radial kernel scale=4","dimensions":2}),
    )
}
pub fn commands() -> Vec<Command> {
    vec![Command{name:"sample_noise",mutates:false,description:"Sample raw signed 2D Perlin or simplex noise and analytic coordinate gradients. Explicit seed; no hidden fractals or presets. Same-build deterministic; point-sampled. Conservative value bounds are returned, not unit-range normalization.",schema:obj(vec![("kind",json!({"type":"string","enum":["perlin","simplex"]})),("seed",json!({"type":"integer","minimum":0})),("points",json!({"type":"array","maxItems":65536,"items":{"type":"array","minItems":2,"maxItems":2,"items":{"type":"number"}}}))],&["kind","seed","points"]),run:sample_noise}]
}
