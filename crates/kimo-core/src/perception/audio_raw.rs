//! Native raw planar-f32 operations. No clipping, gain shaping or hidden limiter.
#[derive(Clone, Debug)]
pub struct Samples {
    pub rate: u32,
    pub planes: Vec<Vec<f32>>,
}
impl Samples {
    pub fn validate(&self) -> Result<usize, String> {
        if self.rate == 0 || self.planes.is_empty() {
            return Err("invalid_samples: positive rate and >=1 channel required".into());
        }
        let n = self.planes[0].len();
        if self
            .planes
            .iter()
            .any(|p| p.len() != n || p.iter().any(|v| !v.is_finite()))
        {
            return Err("invalid_samples: equal-length finite planar f32 channels required".into());
        }
        Ok(n)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Section {
    pub b: [f64; 3],
    pub a: [f64; 3],
}
pub fn biquad(input: &Samples, sections: &[Section]) -> Result<Samples, String> {
    input.validate()?;
    for s in sections {
        if s.b.iter().chain(s.a.iter()).any(|v| !v.is_finite()) || s.a[0] == 0. {
            return Err("invalid_filter: finite coefficients and nonzero a0 required".into());
        }
    }
    let mut out = input.clone();
    for plane in &mut out.planes {
        for s in sections {
            let mut state = [0f64; 2];
            for (i, x) in plane.iter_mut().enumerate() {
                if i % 16384 == 0 {
                    crate::guard::check();
                }
                let v = *x as f64;
                let y = s.b[0] / s.a[0] * v + state[0];
                state[0] = s.b[1] / s.a[0] * v - s.a[1] / s.a[0] * y + state[1];
                state[1] = s.b[2] / s.a[0] * v - s.a[2] / s.a[0] * y;
                *x = y as f32;
                if !x.is_finite() {
                    return Err("invalid_filter: biquad overflow (no clamp applied)".into());
                }
            }
        }
    }
    Ok(out)
}
pub fn fir(input: &Samples, coeffs: &[f64]) -> Result<Samples, String> {
    let n = input.validate()?;
    if coeffs.is_empty() || coeffs.iter().any(|v| !v.is_finite()) {
        return Err("invalid_filter: finite nonempty fir coefficients required".into());
    }
    let mut out = input.clone();
    for (ch, plane) in out.planes.iter_mut().enumerate() {
        for i in 0..n {
            if i % 16384 == 0 {
                crate::guard::check();
            }
            let mut sum = 0f64;
            for (k, &c) in coeffs.iter().enumerate().take(i + 1) {
                sum += c * input.planes[ch][i - k] as f64;
            }
            plane[i] = sum as f32;
            if !plane[i].is_finite() {
                return Err("invalid_filter: fir overflow (no clamp applied)".into());
            }
        }
    }
    Ok(out)
}
pub fn mix_matrix(input: &Samples, matrix: &[Vec<f64>]) -> Result<Samples, String> {
    let n = input.validate()?;
    if matrix.is_empty()
        || matrix
            .iter()
            .any(|r| r.len() != input.planes.len() || r.iter().any(|v| !v.is_finite()))
    {
        return Err(
            "invalid_filter: matrix rows require one finite coefficient per input channel".into(),
        );
    }
    crate::guard::reserve(
        n.checked_mul(matrix.len())
            .and_then(|v| v.checked_mul(4))
            .ok_or("invalid_filter: matrix allocation overflow")?,
    );
    let mut planes = Vec::with_capacity(matrix.len());
    for row in matrix {
        let mut plane = vec![0f32; n];
        for i in 0..n {
            if i % 16384 == 0 {
                crate::guard::check();
            }
            let sum = row
                .iter()
                .zip(&input.planes)
                .map(|(c, p)| c * p[i] as f64)
                .sum::<f64>();
            plane[i] = sum as f32;
            if !plane[i].is_finite() {
                return Err("invalid_filter: mix_matrix overflow (no clamp applied)".into());
            }
        }
        planes.push(plane);
    }
    Ok(Samples {
        rate: input.rate,
        planes,
    })
}
/// Canonical bytes are channel-major little-endian f32, shared by hash/base64/raw/NPY.
pub fn bytes(s: &Samples) -> Vec<u8> {
    s.planes
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}
pub fn from_bytes(
    rate: u32,
    channels: usize,
    data: &[u8],
    planar: bool,
    width: usize,
) -> Result<Samples, String> {
    if channels == 0
        || channels > 64
        || ![2, 4, 8].contains(&width)
        || data.len() % (width * channels) != 0
    {
        return Err("invalid_samples: length not divisible by channels and sample width".into());
    }
    let n = data.len() / width / channels;
    let mut planes = vec![vec![0.; n]; channels];
    for (i, raw) in data.chunks_exact(width).enumerate() {
        let v = match width {
            2 => i16::from_le_bytes(raw.try_into().unwrap()) as f64 / 32768.,
            4 => f32::from_le_bytes(raw.try_into().unwrap()) as f64,
            _ => f64::from_le_bytes(raw.try_into().unwrap()),
        };
        let v = v as f32;
        if !v.is_finite() {
            return Err(format!("invalid_samples: non-finite sample index {i}"));
        }
        let (c, t) = if planar {
            (i / n.max(1), i % n.max(1))
        } else {
            (i % channels, i / channels)
        };
        planes[c][t] = v;
    }
    Ok(Samples { rate, planes })
}
pub fn b64(data: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | ((c.get(1).copied().unwrap_or(0) as u32) << 8)
            | c.get(2).copied().unwrap_or(0) as u32;
        out.push(A[(n >> 18) as usize] as char);
        out.push(A[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 {
            A[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            A[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
pub fn unb64(s: &str) -> Result<Vec<u8>, String> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    if s.len() % 4 != 0 {
        return Err("invalid_samples: base64 length must be multiple of 4".into());
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let cs = s.as_bytes();
    for (j, c) in cs.chunks_exact(4).enumerate() {
        let pad = usize::from(c[3] == b'=') + usize::from(c[2] == b'=');
        if pad > 0 && j + 1 != cs.len() / 4
            || c[0] == b'='
            || c[1] == b'='
            || c[2] == b'=' && c[3] != b'='
        {
            return Err("invalid_samples: invalid base64 padding".into());
        }
        let mut n = 0u32;
        for &b in c {
            let v = if b == b'=' {
                0
            } else {
                A.iter()
                    .position(|&x| x == b)
                    .ok_or("invalid_samples: invalid base64 character")?
            };
            n = n << 6 | v as u32;
        }
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}
pub fn wav(s: &Samples) -> Result<Vec<u8>, String> {
    let n = s.validate()?;
    let channels = s.planes.len();
    let size = n
        .checked_mul(channels)
        .and_then(|v| v.checked_mul(4))
        .filter(|v| *v < u32::MAX as usize - 36)
        .ok_or("invalid_samples: WAV RIFF32 size overflow")?;
    let mut out = Vec::with_capacity(size + 44);
    out.extend(b"RIFF");
    out.extend((size as u32 + 36).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(3u16.to_le_bytes());
    out.extend((channels as u16).to_le_bytes());
    out.extend(s.rate.to_le_bytes());
    out.extend((s.rate * channels as u32 * 4).to_le_bytes());
    out.extend((channels as u16 * 4).to_le_bytes());
    out.extend(32u16.to_le_bytes());
    out.extend(b"data");
    out.extend((size as u32).to_le_bytes());
    for i in 0..n {
        for p in &s.planes {
            out.extend(p[i].to_le_bytes());
        }
    }
    Ok(out)
}
pub fn npy(s: &Samples) -> Vec<u8> {
    let header = format!(
        "{{'descr': '<f4', 'fortran_order': False, 'shape': ({}, {}), }}",
        s.planes.len(),
        s.planes.first().map(Vec::len).unwrap_or(0)
    );
    let pad = (64 - (10 + header.len() + 1) % 64) % 64;
    let header = header + &" ".repeat(pad) + "\n";
    let mut out = b"\x93NUMPY\x01\x00".to_vec();
    out.extend((header.len() as u16).to_le_bytes());
    out.extend(header.bytes());
    out.extend(bytes(s));
    out
}
pub fn npy_read(rate: u32, channels: usize, b: &[u8]) -> Result<Samples, String> {
    if b.len() < 10 || &b[..8] != b"\x93NUMPY\x01\x00" {
        return Err("invalid_samples: NPY v1 little-endian planar f32 required".into());
    }
    let n = u16::from_le_bytes([b[8], b[9]]) as usize;
    let h = b
        .get(10..10 + n)
        .and_then(|b| std::str::from_utf8(b).ok())
        .ok_or("invalid_samples: bad NPY header")?;
    if !h.contains("'<f4'") || !h.contains("False") {
        return Err("invalid_samples: NPY requires <f4 C order".into());
    }
    let shape = h
        .split("'shape': (")
        .nth(1)
        .and_then(|s| s.split(')').next())
        .ok_or("invalid_samples: NPY shape missing")?;
    let dims = shape
        .split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .collect::<Vec<_>>();
    if dims.len() != 2
        || dims[0] != channels
        || dims[0].checked_mul(dims[1]).and_then(|v| v.checked_mul(4)) != Some(b.len() - 10 - n)
    {
        return Err("invalid_samples: NPY channel/shape mismatch".into());
    }
    from_bytes(rate, channels, &b[10 + n..], true, 4)
}
pub fn peaks(s: &Samples, buckets: usize) -> Vec<Vec<[f32; 3]>> {
    let n = s.planes[0].len();
    s.planes
        .iter()
        .map(|p| {
            (0..buckets)
                .map(|i| {
                    let (lo, hi) = (i * n / buckets, (i + 1) * n / buckets);
                    if lo == hi {
                        return [0.; 3];
                    }
                    let (mut min, mut max, mut sum) = (f32::INFINITY, f32::NEG_INFINITY, 0f64);
                    for &v in &p[lo..hi] {
                        min = min.min(v);
                        max = max.max(v);
                        sum += (v as f64).powi(2);
                    }
                    [min, max, (sum / (hi - lo) as f64).sqrt() as f32]
                })
                .collect()
        })
        .collect()
}
