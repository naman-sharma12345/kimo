//! A small, fast, dependency-free PNG encoder for the live preview frames (8-bit RGB, Sub filter, zlib with
//! fixed-Huffman deflate and greedy LZ77). It trades a few percent of size for speed: a 640x360 frame encodes in a
//! few milliseconds in a release build, in process (no encoder child to spawn per frame).

const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const WINDOW: usize = 32768;
const HBITS: u32 = 15;

struct Bits { out: Vec<u8>, acc: u64, n: u32 }
impl Bits {
    #[inline] fn put(&mut self, v: u32, len: u32) { self.acc |= (v as u64) << self.n; self.n += len; while self.n >= 8 { self.out.push(self.acc as u8); self.acc >>= 8; self.n -= 8; } }
    /// Huffman codes go most-significant bit first.
    #[inline] fn code(&mut self, c: u32, len: u32) { self.put(c.reverse_bits() >> (32 - len), len); }
    fn finish(mut self) -> Vec<u8> { if self.n > 0 { self.out.push(self.acc as u8); } self.out }
}

#[inline]
fn lit(b: &mut Bits, v: u32) {
    match v { 0..=143 => b.code(0x30 + v, 8), 144..=255 => b.code(0x190 + v - 144, 9), 256..=279 => b.code(v - 256, 7), _ => b.code(0xC0 + v - 280, 8) }
}

const LBASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEXT: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DBASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DEXT: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

#[inline]
fn matched(b: &mut Bits, len: usize, dist: usize) {
    let li = LBASE.partition_point(|&x| x as usize <= len) - 1;
    lit(b, 257 + li as u32);
    if LEXT[li] > 0 { b.put((len - LBASE[li] as usize) as u32, LEXT[li] as u32); }
    let di = DBASE.partition_point(|&x| x as usize <= dist) - 1;
    b.code(di as u32, 5);
    if DEXT[di] > 0 { b.put((dist - DBASE[di] as usize) as u32, DEXT[di] as u32); }
}

/// Raw deflate stream (one final fixed-Huffman block).
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut b = Bits { out: Vec::with_capacity(data.len() / 2 + 64), acc: 0, n: 0 };
    b.put(1, 1); b.put(1, 2); // BFINAL, BTYPE = 01 fixed Huffman
    let mut head = vec![usize::MAX; 1 << HBITS];
    let hash = |i: usize| -> usize { let v = (data[i] as u32) | (data[i + 1] as u32) << 8 | (data[i + 2] as u32) << 16; (v.wrapping_mul(2654435761) >> (32 - HBITS)) as usize };
    let n = data.len();
    let mut i = 0;
    while i < n {
        let mut best = (0usize, 0usize);
        if i + MIN_MATCH <= n {
            let h = hash(i);
            let cand = head[h];
            head[h] = i;
            // the previous byte position too: catches runs (distance 1) even when the hash slot was overwritten
            for c in [cand, i.wrapping_sub(1), i.wrapping_sub(3)] {
                if c == usize::MAX || c >= i || i - c > WINDOW { continue; }
                let max = (n - i).min(MAX_MATCH);
                let mut l = 0; while l < max && data[c + l] == data[i + l] { l += 1; }
                if l > best.0 { best = (l, i - c); if l == max { break; } }
            }
        }
        if best.0 >= MIN_MATCH {
            matched(&mut b, best.0, best.1);
            // index a few positions inside the match so later data can refer back into it
            let end = i + best.0;
            let mut j = i + 1; while j < end && j + MIN_MATCH <= n { head[hash(j)] = j; j += if best.0 > 32 { 8 } else { 1 }; }
            i = end;
        } else { lit(&mut b, data[i] as u32); i += 1; }
    }
    lit(&mut b, 256);
    b.finish()
}

fn adler32(d: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for c in d.chunks(5552) { for &x in c { a += x as u32; b += a; } a %= 65521; b %= 65521; }
    b << 16 | a
}

fn crc32(parts: &[&[u8]]) -> u32 {
    static T: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| { let mut t = [0u32; 256]; for (n, e) in t.iter_mut().enumerate() { let mut c = n as u32; for _ in 0..8 { c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 }; } *e = c; } t });
    let mut c = 0xFFFF_FFFFu32;
    for p in parts { for &x in *p { c = t[((c ^ x as u32) & 0xFF) as usize] ^ (c >> 8); } }
    c ^ 0xFFFF_FFFF
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind); out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
}

/// Encode RGBA8 pixels as an 8-bit RGB PNG (alpha dropped: preview frames are composited on opaque black).
pub fn encode_rgb(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
    assert_eq!(rgba.len(), w * h * 4);
    let stride = w * 3;
    let mut raw = Vec::with_capacity(h * (stride + 1));
    for y in 0..h {
        raw.push(1); // Sub filter: each byte minus the same channel of the pixel to its left
        let row = &rgba[y * w * 4..(y + 1) * w * 4];
        let mut prev = [0u8; 3];
        for px in row.chunks_exact(4) { for k in 0..3 { raw.push(px[k].wrapping_sub(prev[k])); prev[k] = px[k]; } }
    }
    let mut z = vec![0x78, 0x01];
    z.extend(deflate(&raw));
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(w as u32).to_be_bytes()); ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Minimal inflate for fixed-Huffman blocks, enough to prove the encoder round-trips.
    fn inflate_fixed(d: &[u8]) -> Vec<u8> {
        let mut pos = 0usize; let bit = |pos: &mut usize| -> u32 { let b = (d[*pos / 8] >> (*pos % 8)) & 1; *pos += 1; b as u32 };
        let bits = |pos: &mut usize, n: u32| -> u32 { let mut v = 0; for k in 0..n { v |= bit(pos) << k; } v };
        let code = |pos: &mut usize, n: u32| -> u32 { let mut v = 0; for _ in 0..n { v = v << 1 | bit(pos); } v };
        assert_eq!(bits(&mut pos, 3), 0b011, "final fixed block");
        let mut out: Vec<u8> = vec![];
        loop {
            let mut c = code(&mut pos, 7);
            let sym = if c <= 0x17 { c + 256 } else { c = c << 1 | bit(&mut pos); if (0x30..=0xBF).contains(&c) { c - 0x30 } else if (0xC0..=0xC7).contains(&c) { c - 0xC0 + 280 } else { c = c << 1 | bit(&mut pos); c - 0x190 + 144 } };
            if sym < 256 { out.push(sym as u8); continue; }
            if sym == 256 { break; }
            let li = (sym - 257) as usize; let len = LBASE[li] as usize + bits(&mut pos, LEXT[li] as u32) as usize;
            let di = code(&mut pos, 5) as usize; let dist = DBASE[di] as usize + bits(&mut pos, DEXT[di] as u32) as usize;
            for _ in 0..len { let b = out[out.len() - dist]; out.push(b); }
        }
        out
    }
    #[test]
    fn deflate_round_trips() {
        let mut data: Vec<u8> = (0..50_000u32).map(|i| ((i * 7919) % 251) as u8).collect();
        data.extend(std::iter::repeat(9u8).take(70_000));
        data.extend((0..20_000u32).map(|i| (i / 300) as u8));
        let z = deflate(&data);
        assert!(z.len() < data.len() / 2, "compresses: {} -> {}", data.len(), z.len());
        assert_eq!(inflate_fixed(&z), data);
        assert_eq!(inflate_fixed(&deflate(b"")), b"");
        assert_eq!(adler32(b"Wikipedia"), 0x11E60398);
        assert_eq!(crc32(&[b"IEND"]), 0xAE426082);
    }
    #[test]
    fn png_structure() {
        let (w, h) = (64, 36);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [(i % w * 4) as u8, (i / w * 7) as u8, 128, 255]).collect();
        let p = encode_rgb(&rgba, w, h);
        assert_eq!(&p[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&p[12..16], b"IHDR");
        assert_eq!(&p[p.len() - 12..], &[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]);
        // the IDAT payload inflates back to the filtered rows
        let idat_len = u32::from_be_bytes(p[33..37].try_into().unwrap()) as usize;
        assert_eq!(&p[37..41], b"IDAT");
        let z = &p[41..41 + idat_len];
        let raw = inflate_fixed(&z[2..z.len() - 4]);
        assert_eq!(raw.len(), h * (w * 3 + 1));
        assert_eq!(u32::from_be_bytes(z[z.len() - 4..].try_into().unwrap()), adler32(&raw));
    }
}
