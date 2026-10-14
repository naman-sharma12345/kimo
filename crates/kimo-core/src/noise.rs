//! Original 2D coherent field equations. No imported implementation or permutation tables.
//! Unit gradients; conservative value bounds: Perlin sqrt(2), simplex 3/(4 sqrt(2)).
//! Same-build deterministic, not a cross-architecture bit-exact transcendental contract.
use crate::effects::{Frame, Params};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub value: f64,
    pub gradient: [f64; 2],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Perlin,
    Simplex,
}
impl Kind {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "perlin" => Ok(Self::Perlin),
            "simplex" => Ok(Self::Simplex),
            _ => Err(format!("kind must be perlin|simplex, got '{s}'")),
        }
    }
    pub fn bound(self) -> f64 {
        match self {
            Self::Perlin => std::f64::consts::SQRT_2,
            Self::Simplex => 3.0 / (4.0 * std::f64::consts::SQRT_2),
        }
    }
}
// Finite safe integer conversion, with margin for skew and adjacent lattice vertices.
pub const COORD_LIMIT: f64 = 1_000_000_000.0;
fn validate(x: f64, y: f64) -> Result<(), String> {
    if !x.is_finite() || !y.is_finite() || x.abs() > COORD_LIMIT || y.abs() > COORD_LIMIT {
        return Err("noise coordinates must be finite and within +/-1e9 field units".into());
    }
    Ok(())
}
fn gradient(seed: u64, x: i64, y: i64) -> [f64; 2] {
    // FNV-1a over explicit little-endian seed and lattice coordinates; no lookup table.
    let mut h = 0xcbf29ce484222325u64;
    for b in seed
        .to_le_bytes()
        .into_iter()
        .chain(x.to_le_bytes())
        .chain(y.to_le_bytes())
    {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    // Fold high/low bits before selecting a direction to avoid low-bit lattice parity.
    h ^= h.rotate_left(21);
    h ^= h >> 31;
    // fmix64-style avalanche: FNV-1a leaves a low-entropy bottleneck for small coordinates
    // (trailing zero bytes dominate the multiply's 2^32 shift), which correlated lattice
    // gradient angles into visible stripe bands. Measured: 9-10/16 near-horizontal gradients
    // in the worst column for seeds 0, 1, 20261014; 3-4/16 after this finalizer.
    h ^= h >> 33; h = h.wrapping_mul(0xff51afd7ed558ccd); h ^= h >> 33; h = h.wrapping_mul(0xc4ceb9fe1a85ec53); h ^= h >> 33;
    let a = ((h >> 32) as u32 as f64 + 0.5) * (std::f64::consts::TAU / 4294967296.0);
    [a.cos(), a.sin()]
}
fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn fade(t: f64) -> (f64, f64) {
    (
        t * t * t * (10.0 + t * (-15.0 + 6.0 * t)),
        30.0 * t * t * (t - 1.0) * (t - 1.0),
    )
}
pub fn sample(kind: Kind, seed: u64, x: f64, y: f64) -> Result<Sample, String> {
    validate(x, y)?;
    Ok(match kind {
        Kind::Perlin => perlin(seed, x, y),
        Kind::Simplex => simplex(seed, x, y),
    })
}
fn perlin(seed: u64, x: f64, y: f64) -> Sample {
    let (ix, iy) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let (u, du) = fade(fx);
    let (v, dv) = fade(fy);
    let mut out = Sample {
        value: 0.0,
        gradient: [0.0; 2],
    };
    for j in 0..2 {
        for i in 0..2 {
            let g = gradient(seed, ix + i, iy + j);
            let d = [fx - i as f64, fy - j as f64];
            let n = dot(g, d);
            let (wx, dx) = if i == 0 { (1.0 - u, -du) } else { (u, du) };
            let (wy, dy) = if j == 0 { (1.0 - v, -dv) } else { (v, dv) };
            out.value += wx * wy * n;
            out.gradient[0] += dx * wy * n + wx * wy * g[0];
            out.gradient[1] += wx * dy * n + wx * wy * g[1];
        }
    }
    out
}
fn simplex(seed: u64, x: f64, y: f64) -> Sample {
    // Equilateral triangular lattice: skew/unskew obtained by solving the 60-degree basis.
    let f = (3.0f64.sqrt() - 1.0) / 2.0;
    let g = (1.0 - 1.0 / 3.0f64.sqrt()) / 2.0;
    let s = (x + y) * f;
    let (i, j) = ((x + s).floor() as i64, (y + s).floor() as i64);
    let t = (i + j) as f64 * g;
    let q = [x - (i as f64 - t), y - (j as f64 - t)];
    let middle = if q[0] > q[1] { (1i64, 0i64) } else { (0, 1) };
    let mut out = Sample {
        value: 0.0,
        gradient: [0.0; 2],
    };
    for (di, dj) in [(0, 0), middle, (1, 1)] {
        let shift = (di + dj) as f64 * g;
        let r = [q[0] - di as f64 + shift, q[1] - dj as f64 + shift];
        let k = 0.5 - dot(r, r);
        if k <= 0.0 {
            continue;
        }
        let grad = gradient(seed, i + di, j + dj);
        let d = dot(grad, r);
        // Compact radial kernel k^4. Scale 4 is our explicit low-amplitude convention,
        // NOT the conventional empirical scale 70 and NOT unit-range normalization.
        out.value += 4.0 * k.powi(4) * d;
        for a in 0..2 {
            out.gradient[a] += 4.0 * (k.powi(4) * grad[a] - 8.0 * k.powi(3) * r[a] * d);
        }
    }
    out
}
pub fn apply(
    params: &Params,
    strings: &BTreeMap<String, String>,
    frame: &mut Frame,
    time: f64,
) -> Result<(), String> {
    let get = |n: &str, d: f64| params.get(n).map(|p| p.at(time)).unwrap_or(d);
    let kind = Kind::parse(strings.get("kind").map(String::as_str).unwrap_or("perlin"))?;
    let seed = strings
        .get("seed")
        .map(|s| {
            s.parse::<u64>()
                .map_err(|_| "seed must be u64 decimal".to_string())
        })
        .transpose()?
        .unwrap_or(0);
    let (frequency, ox, oy, rx, ry, amount, gain, bias) = (
        get("frequency", 8.0),
        get("offset_x", 0.0),
        get("offset_y", 0.0),
        get("rate_x", 0.0),
        get("rate_y", 0.0),
        get("amount", 1.0),
        get("gain", 1.0),
        get("bias", 0.0),
    );
    if [time, frequency, ox, oy, rx, ry, amount, gain, bias]
        .iter()
        .any(|x| !x.is_finite())
        || !(0.0..=1.0).contains(&amount)
    {
        return Err("noise effect values must be finite; amount must be 0..1".into());
    }
    if frame.w == 0
        || frame.h == 0
        || frame.d.len()
            != frame
                .w
                .checked_mul(frame.h)
                .and_then(|v| v.checked_mul(4))
                .ok_or("noise frame dimensions overflow")?
    {
        return Err("noise needs a valid nonempty RGBA frame".into());
    }
    let lim = crate::resource_limits::current();
    let pixels = frame.w.checked_mul(frame.h).ok_or("noise pixel overflow")?;
    if pixels as u64 > lim.max_pixels
        || (pixels as u64).saturating_mul(256) > lim.expr_max_pixel_ops.unwrap_or(u64::MAX)
    {
        return Err("noise field exceeds max_pixels or expression work budget".into());
    }
    let h = frame.h as f64;
    let qx = ox + rx * time;
    let qy = oy + ry * time;
    // Validate whole affine footprint before touching any pixels, avoiding partial frames.
    for x in [0.5 / h, (frame.w as f64 - 0.5) / h] {
        for y in [0.5 / h, (frame.h as f64 - 0.5) / h] {
            validate(x * frequency + qx, y * frequency + qy)?;
        }
    }
    if amount == 0.0 {
        return Ok(());
    }
    crate::resource_limits::reserve(
        frame
            .d
            .len()
            .checked_mul(4)
            .ok_or("noise output allocation overflow")?,
    )?;
    let mut output = Frame::try_new(frame.w, frame.h)?;
    output.d.copy_from_slice(&frame.d);
    let start = std::time::Instant::now();
    for y in 0..frame.h {
        crate::guard::check();
        if lim
            .expr_timeout_ms
            .map(|ms| start.elapsed().as_millis() > u128::from(ms))
            .unwrap_or(false)
        {
            return Err("noise field timed out".into());
        }
        for x in 0..frame.w {
            let s = sample(
                kind,
                seed,
                (x as f64 + 0.5) / h * frequency + qx,
                (y as f64 + 0.5) / h * frequency + qy,
            )?;
            // Signed raw value: no clamp, no built-in look, alpha remains exactly unchanged.
            let value = (s.value * gain + bias) as f32;
            if !value.is_finite() {
                return Err("noise output exceeds finite f32 range".into());
            }
            let i = (y * frame.w + x) * 4;
            for c in 0..3 {
                let mixed =
                    (frame.d[i + c] as f64 * (1.0 - amount) + f64::from(value) * amount) as f32;
                if !mixed.is_finite() {
                    return Err("noise mix exceeds finite f32 range".into());
                }
                output.d[i + c] = mixed;
            }
        }
    }
    frame.d = output.d;
    Ok(())
}
