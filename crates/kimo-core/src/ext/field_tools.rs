//! Raw independently chosen field operators. Does not create hidden presets.
use super::kit::*;
fn run(_: &mut Session, a: &Value) -> Result<Value, String> {
    let field = crate::noise_fields::parse(a)?;
    let points = a
        .get("points")
        .and_then(Value::as_array)
        .ok_or("points must be array of [x,y]")?;
    let lim = crate::resource_limits::current();
    if points.len() > 65536 || points.len() as u64 > lim.max_samples {
        return Err("field sample count exceeds min(65536,max_samples)".into());
    }
    if (points.len() as u64).saturating_mul(4096) > lim.expr_max_pixel_ops.unwrap_or(u64::MAX) {
        return Err("field samples exceed work budget".into());
    }
    crate::resource_limits::reserve(
        points
            .len()
            .checked_mul(384)
            .ok_or("field result allocation overflow")?,
    )?;
    let mut result = Vec::with_capacity(points.len());
    let start = std::time::Instant::now();
    for point in points {
        crate::guard::check();
        if lim
            .expr_timeout_ms
            .map(|t| start.elapsed().as_millis() > u128::from(t))
            .unwrap_or(false)
        {
            return Err("field samples timed out".into());
        }
        let q = point
            .as_array()
            .filter(|p| p.len() == 2)
            .ok_or("every point must be [x,y]")?;
        let x = q[0].as_f64().ok_or("x numeric")?;
        let y = q[1].as_f64().ok_or("y numeric")?;
        if let crate::noise_fields::Field::Worley { seed, .. } = field {
            let w = crate::noise_fields::worley(seed, x, y)?;
            result.push(json!({"f1":w.f1,"f2":w.f2,"gradient1":w.gradient1,"gradient2":w.gradient2,"cell1":w.cell1,"cell2":w.cell2}));
        } else {
            let (v, g) = field.values(x, y)?;
            result.push(json!({"value":v[0],"other_component":v[1],"gradient":g}));
        }
    }
    Ok(
        json!({"samples":result,"dimensions":2,"normalization":"explicit config only; cellular Euclidean unnormalized","differentiability":"Worley distance is not differentiable at feature points/ties; tie uses lexical cell ID, feature gradient zero"}),
    )
}
pub fn commands() -> Vec<Command> {
    vec![Command{name:"sample_field",mutates:false,description:"Sample exactly one raw 2D operator: Worley Euclidean F1/F2, explicit FBM, one domain warp, or scalar-potential curl. No bundled effect stack. Returns gradients where defined; curl returns vector components, not a scalar gradient.",schema:obj(vec![
("op",json!({"type":"string","enum":["worley","fbm","warp","curl"]})),("kind",json!({"type":"string","enum":["perlin","simplex"]})),("seed",json!({"type":"integer","minimum":0})),
("points",json!({"type":"array","maxItems":65536,"items":{"type":"array","minItems":2,"maxItems":2,"items":{"type":"number"}}})),
("channel",json!({"type":"integer","minimum":0,"maximum":1})),("octaves",json!({"type":"integer","minimum":1,"maximum":16})),("lacunarity",n("positive frequency multiplier")),("octave_gain",n("signed amplitude multiplier")),("normalize",json!({"type":"boolean"})),
("warp_kind",json!({"type":"string","enum":["perlin","simplex"]})),("warp_seed",json!({"type":"integer","minimum":0})),("warp_frequency",n("field frequency")),("warp_amplitude",n("coordinate units")),("curl_frequency",n("potential frequency")),("curl_amplitude",n("potential amplitude"))],&["op","seed","points"]),run}]
}
