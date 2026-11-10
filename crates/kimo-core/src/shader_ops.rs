//! Float-buffer shader primitives. Signed/HDR values stay unclipped.
use crate::{
    effects::{p, Frame, Params},
    pixel_expr,
};
use std::collections::BTreeMap;
fn apply_inner(
    kind: &str,
    params: &Params,
    strings: &BTreeMap<String, String>,
    f: &mut Frame,
    t: f64,
) -> Result<(), String> {
    crate::resource_limits::reserve(f.d.len().checked_mul(4).ok_or("shader buffer overflow")?)?;
    let src = Frame {
        w: f.w,
        h: f.h,
        d: f.d.clone(),
    };
    let begin = std::time::Instant::now();
    let timeout = crate::resource_limits::current().expr_timeout_ms;
    let timed_out = std::sync::atomic::AtomicBool::new(false);
    let edge = strings.get("edge").map(String::as_str).unwrap_or("clamp");
    if !["clamp", "wrap", "transparent"].contains(&edge) {
        return Err("edge must be clamp|wrap|transparent".into());
    }
    if kind == "convolution" {
        let text = strings.get("kernel").map(String::as_str).unwrap_or("[1]");
        let session = crate::resource_limits::current();
        if text.len() as u64
            > session
                .memory_bytes
                .min(crate::guard::BUDGET.load(std::sync::atomic::Ordering::Relaxed) as u64)
                / 16
        {
            return Err("convolution kernel memory_bytes limit exceeded; set_limits memory_bytes / startup KIMO_MEMORY_BUDGET_BYTES".into());
        }
        crate::resource_limits::reserve(
            text.len()
                .checked_mul(16)
                .ok_or("kernel allocation overflow")?,
        )?;
        let kernel: Vec<f32> =
            serde_json::from_str(strings.get("kernel").map(String::as_str).unwrap_or("[1]"))
                .map_err(|e| format!("kernel: {e}"))?;
        let size = (kernel.len() as f64).sqrt() as usize;
        if size == 0
            || size % 2 == 0
            || size * size != kernel.len()
            || kernel.iter().any(|v| !v.is_finite())
        {
            return Err("kernel must be a finite odd square array".into());
        }
        let cost =
            f.w.checked_mul(f.h)
                .and_then(|n| n.checked_mul(kernel.len()))
                .ok_or("convolution cost overflow")?;
        let l = crate::resource_limits::current();
        if l.expr_max_pixel_ops.map_or(false, |v| cost as u64 > v) {
            return Err(
                "convolution expr_max_pixel_ops exceeded; set_limits expr_max_pixel_ops".into(),
            );
        }
        let div = p(params, "divisor", 1., t);
        if div == 0. {
            return Err("convolution divisor must be nonzero".into());
        }
        let bias = p(params, "bias", 0., t);
        let half = (size / 2) as f32;
        crate::par::rows_mut(&mut f.d, f.w * 4, |y, row| {
            crate::guard::check();
            if timeout.map_or(false, |ms| begin.elapsed().as_millis() > ms as u128) {
                timed_out.store(true, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            for (x, out) in row.chunks_mut(4).enumerate() {
                let mut acc = [0.; 4];
                for (ki, k) in kernel.iter().enumerate() {
                    let u = (x as f32 + ki as f32 % size as f32 - half) / (src.w - 1).max(1) as f32;
                    let v = (y as f32 + (ki / size) as f32 - half) / (src.h - 1).max(1) as f32;
                    let c = pixel_expr::sample(&src, u, v, edge);
                    for j in 0..4 {
                        acc[j] += c[j] * k
                    }
                }
                for j in 0..3 {
                    out[j] = acc[j] / div + bias
                }
                out[3] = src.d[(y * src.w + x) * 4 + 3];
            }
        });
        if timed_out.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("expr_timeout_ms exceeded; set_limits expr_timeout_ms".into());
        }
        return Ok(());
    }
    if kind == "grade" {
        let gain = [
            p(params, "gain_r", 1., t),
            p(params, "gain_g", 1., t),
            p(params, "gain_b", 1., t),
        ];
        let lift = [
            p(params, "lift_r", 0., t),
            p(params, "lift_g", 0., t),
            p(params, "lift_b", 0., t),
        ];
        crate::par::rows_mut(&mut f.d, f.w * 4, |_, row| {
            crate::guard::check();
            if timeout.map_or(false, |ms| begin.elapsed().as_millis() > ms as u128) {
                timed_out.store(true, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            for c in row.chunks_mut(4) {
                for j in 0..3 {
                    c[j] = c[j] * gain[j] + lift[j]
                }
            }
        });
        if timed_out.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("expr_timeout_ms exceeded; set_limits expr_timeout_ms".into());
        }
        return Ok(());
    }
    if kind == "displace"
        && p(params, "amount_x", 0., t) == 0.
        && p(params, "amount_y", 0., t) == 0.
    {
        if timed_out.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("expr_timeout_ms exceeded; set_limits expr_timeout_ms".into());
        }
        return Ok(());
    }
    let aux = strings
        .get("aux_path")
        .map(|s| crate::pixels::read(&crate::deps::resolve_str(s)))
        .transpose()?;
    let program = if let Some(e) = strings.get("expr") {
        Some(pixel_expr::compile(
            e,
            crate::resource_limits::current()
                .expr_max_ops
                .unwrap_or(u64::MAX)
                .min(usize::MAX as u64) as usize,
        )?)
    } else {
        None
    };
    if aux.is_none() && program.is_none() {
        return Err("displace requires aux_path or expr field".into());
    }
    let expr_field = program
        .map(|pr| pixel_expr::apply(&pr, &src, aux.as_ref(), [0.; 8], t as f32))
        .transpose()?;
    let map = expr_field.as_ref().or(aux.as_ref()).unwrap();
    let amp_x = p(params, "amount_x", 0., t);
    let amp_y = p(params, "amount_y", 0., t);
    let neutral = p(params, "neutral", 0.5, t);
    crate::par::rows_mut(&mut f.d, f.w * 4, |y, row| {
        crate::guard::check();
        if timeout.map_or(false, |ms| begin.elapsed().as_millis() > ms as u128) {
            timed_out.store(true, std::sync::atomic::Ordering::Relaxed);
            return;
        }
        for (x, out) in row.chunks_mut(4).enumerate() {
            let (u, v) = (
                x as f32 / (src.w - 1).max(1) as f32,
                y as f32 / (src.h - 1).max(1) as f32,
            );
            let d = pixel_expr::sample(map, u, v, "clamp");
            out.copy_from_slice(&pixel_expr::sample(
                &src,
                u + (d[0] - neutral) * amp_x,
                v + (d[1] - neutral) * amp_y,
                edge,
            ));
        }
    });
    if timed_out.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("expr_timeout_ms exceeded; set_limits expr_timeout_ms".into());
    }
    Ok(())
}

/// Transactional shader commit: errors/timeouts never expose partially written frame pixels.
pub fn apply(
    kind: &str,
    params: &Params,
    strings: &BTreeMap<String, String>,
    f: &mut Frame,
    t: f64,
) -> Result<(), String> {
    crate::resource_limits::reserve(
        f.d.len()
            .checked_mul(4)
            .ok_or("shader allocation overflow")?,
    )?;
    let mut out = Frame {
        w: f.w,
        h: f.h,
        d: f.d.clone(),
    };
    let before = std::time::Instant::now();
    apply_inner(kind, params, strings, &mut out, t)?;
    if crate::resource_limits::current()
        .expr_timeout_ms
        .map_or(false, |ms| before.elapsed().as_millis() > ms as u128)
    {
        return Err("expr_timeout_ms exceeded; set_limits expr_timeout_ms".into());
    }
    if out.d.iter().any(|v| !v.is_finite()) {
        return Err("shader produced NaN/Inf (check finite parameters and divisor)".into());
    }
    f.d = out.d;
    Ok(())
}
