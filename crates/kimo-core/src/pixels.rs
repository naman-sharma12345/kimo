//! Lossless working-buffer pixel interchange. No display/output transform.
use crate::effects::Frame;
const MAGIC: &[u8; 8] = b"KIMOPX01";
pub fn encode(f: &Frame) -> Result<Vec<u8>, String> {
    let n =
        f.w.checked_mul(f.h)
            .and_then(|n| n.checked_mul(4))
            .ok_or("pixel size overflow")?;
    if f.w == 0 || f.h == 0 || f.d.len() != n || f.d.iter().any(|v| !v.is_finite()) {
        return Err("pixels need positive dimensions and finite RGBA f32".into());
    }
    crate::guard::reserve(
        n.checked_mul(4)
            .and_then(|v| v.checked_add(16))
            .ok_or("pixel allocation overflow")?,
    );
    let mut b = Vec::with_capacity(n * 4 + 16);
    b.extend(MAGIC);
    b.extend((f.w as u32).to_le_bytes());
    b.extend((f.h as u32).to_le_bytes());
    for (i, v) in f.d.iter().enumerate() {
        if i % 16384 == 0 {
            crate::guard::check()
        }
        b.extend(v.to_le_bytes())
    }
    Ok(b)
}
pub fn decode(b: &[u8]) -> Result<Frame, String> {
    if b.len() < 16 || &b[..8] != MAGIC {
        return Err("invalid KIMOPX01 header".into());
    }
    let w = u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize;
    let h = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
    let n = w
        .checked_mul(h)
        .and_then(|v| v.checked_mul(4))
        .ok_or("pixel size overflow")?;
    if w == 0 || h == 0 || n.checked_mul(4).and_then(|v| v.checked_add(16)) != Some(b.len()) {
        return Err("invalid pixel dimensions/byte count".into());
    }
    crate::guard::reserve(n * 4);
    let mut d = Vec::with_capacity(n);
    for (i, c) in b[16..].chunks_exact(4).enumerate() {
        if i % 16384 == 0 {
            crate::guard::check()
        }
        let v = f32::from_le_bytes(c.try_into().unwrap());
        if !v.is_finite() {
            return Err("nonfinite pixel".into());
        }
        d.push(v)
    }
    Ok(Frame { w, h, d })
}
pub fn read(path: &str) -> Result<Frame, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > crate::guard::limits().mem_bytes as u64 / 2 {
        return Err("pixel file exceeds memory budget".into());
    }
    crate::guard::reserve(meta.len() as usize);
    decode(&std::fs::read(path).map_err(|e| e.to_string())?)
}
pub fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 4 != 0 {
        return Err("base64 length must be multiple of 4".into());
    }
    crate::guard::reserve(s.len() / 4 * 3);
    let val = |b: u8| -> Result<u32, String> {
        match b {
            b'A'..=b'Z' => Ok((b - b'A') as u32),
            b'a'..=b'z' => Ok((b - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((b - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err("invalid base64 character".into()),
        }
    };
    let mut out = vec![];
    for (i, c) in s.as_bytes().chunks_exact(4).enumerate() {
        crate::guard::check();
        let pad = if c[2] == b'=' {
            2
        } else if c[3] == b'=' {
            1
        } else {
            0
        };
        if pad > 0 && i + 1 != s.len() / 4 {
            return Err("base64 padding only allowed at end".into());
        }
        if pad == 2 && c[3] != b'=' {
            return Err("invalid base64 padding".into());
        }
        let (a, b) = (val(c[0])?, val(c[1])?);
        let d = if pad < 2 { val(c[2])? } else { 0 };
        let e = if pad == 0 { val(c[3])? } else { 0 };
        if (pad == 2 && b & 15 != 0) || (pad == 1 && d & 3 != 0) {
            return Err("noncanonical base64 padding".into());
        }
        let x = (a << 18) | (b << 12) | (d << 6) | e;
        out.push((x >> 16) as u8);
        if pad < 2 {
            out.push((x >> 8) as u8)
        }
        if pad == 0 {
            out.push(x as u8)
        }
    }
    Ok(out)
}
