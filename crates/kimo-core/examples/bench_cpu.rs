use kimo_core::effects::{box_blur3, Frame};
use std::time::Instant;
// reference = the pre-optimisation implementation, kept verbatim for bit-exactness checks
fn ref_axis(f: &mut Frame, r: usize, horiz: bool) {
    let (w, h) = (f.w, f.h); let src = f.d.clone();
    let (len, lines) = if horiz { (w, h) } else { (h, w) };
    for l in 0..lines {
        let idx = |i: usize| if horiz { (l*w+i)*4 } else { (i*w+l)*4 };
        let mut acc = [0f32; 4]; let win = (2*r+1) as f32;
        for i in 0..=r.min(len-1) { for j in 0..4 { acc[j] += src[idx(i)+j]; } }
        for j in 0..4 { acc[j] += src[idx(0)+j] * r as f32; }
        for i in 0..len { let o = idx(i); for j in 0..4 { f.d[o+j] = acc[j]/win; }
            let add = src[idx((i+r+1).min(len-1))..][..4].to_vec(); let sub = src[idx(i.saturating_sub(r))..][..4].to_vec();
            for j in 0..4 { acc[j] += add[j] - sub[j]; } }
    }
}
fn ref_blur(f: &mut Frame, r: usize) { let r = (r/2).max(1); for _ in 0..3 { ref_axis(f, r, true); ref_axis(f, r, false); } }
fn mk(w: usize, h: usize) -> Frame { let mut f = Frame::new(w, h); let mut s = 12345u32; for v in f.d.iter_mut() { s = s.wrapping_mul(1664525).wrapping_add(1013904223); *v = (s >> 8) as f32 / 16777216.0; } f }
fn main() {
    let (w, h) = (1920, 1080);
    for r in [4usize, 24, 80] {
        let base = mk(w, h);
        let mut a = base.clone_f(); let t = Instant::now(); ref_blur(&mut a, r); let t_ref = t.elapsed().as_secs_f64();
        kimo_core::par::set_threads(1);
        let mut b = base.clone_f(); let t = Instant::now(); box_blur3(&mut b, r); let t1 = t.elapsed().as_secs_f64();
        kimo_core::par::set_threads(0);
        let mut c = base.clone_f(); let t = Instant::now(); box_blur3(&mut c, r); let tn = t.elapsed().as_secs_f64();
        let same = a.d.iter().zip(&b.d).all(|(x, y)| x.to_bits() == y.to_bits()) && a.d.iter().zip(&c.d).all(|(x, y)| x.to_bits() == y.to_bits());
        println!("box_blur3 1080p r={r}: old {t_ref:.3}s  new 1thr {t1:.3}s ({:.1}x)  new {}thr {tn:.3}s ({:.1}x)  bit-identical={same}", t_ref/t1, kimo_core::par::threads(), t_ref/tn);
    }
}
trait CF { fn clone_f(&self) -> Frame; }
impl CF for Frame { fn clone_f(&self) -> Frame { Frame { w: self.w, h: self.h, d: self.d.clone() } } }
