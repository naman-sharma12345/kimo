//! Provenance: moved unchanged (arithmetic and operation order) from kimo-core src/perception/audio_events.rs,
//! the radix-2 FFT behind audio_transients. Kimo's own code, MIT (see LICENSE). Not vendored from elsewhere.
//! Pure std, fixed 1024-point, deterministic: no threads, no random state.

pub const FRAME: usize = 1024;
pub struct Fft {
    pub re: [f64; FRAME],
    pub im: [f64; FRAME],
    rev: [usize; FRAME],
    cos: [f64; FRAME / 2],
    sin: [f64; FRAME / 2],
    hann: [f64; FRAME],
}
impl Fft {
    pub fn new() -> Self {
        let mut f = Self {
            re: [0.; FRAME],
            im: [0.; FRAME],
            rev: [0; FRAME],
            cos: [0.; FRAME / 2],
            sin: [0.; FRAME / 2],
            hann: [0.; FRAME],
        };
        for i in 0..FRAME {
            f.rev[i] = i.reverse_bits() >> (usize::BITS - 10);
            f.hann[i] = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (FRAME - 1) as f64).cos();
        }
        for i in 0..FRAME / 2 {
            let a = -std::f64::consts::TAU * i as f64 / FRAME as f64;
            f.cos[i] = a.cos();
            f.sin[i] = a.sin();
        }
        f
    }
    /// Hann-windowed frame of `pcm` centred on `center` (zero outside the signal), then the forward transform into `re` / `im`.
    pub fn run(&mut self, pcm: &[f32], center: usize) {
        for i in 0..FRAME {
            let pos = center as isize + i as isize - FRAME as isize / 2;
            self.re[self.rev[i]] = if pos >= 0 {
                pcm.get(pos as usize).copied().unwrap_or(0.) as f64 * self.hann[i]
            } else {
                0.
            };
            self.im[self.rev[i]] = 0.;
        }
        self.butterflies();
    }
    /// Forward FFT of a raw (unwindowed) real block of exactly FRAME samples into `re` / `im`.
    pub fn forward_real(&mut self, x: &[f64]) {
        assert_eq!(x.len(), FRAME, "forward_real needs exactly {FRAME} samples");
        for i in 0..FRAME {
            self.re[self.rev[i]] = x[i];
            self.im[self.rev[i]] = 0.;
        }
        self.butterflies();
    }
    fn butterflies(&mut self) {
        let mut size = 2;
        while size <= FRAME {
            let half = size / 2;
            let step = FRAME / size;
            for b in (0..FRAME).step_by(size) {
                for j in 0..half {
                    let (a, c) = (b + j, b + j + half);
                    let (co, si) = (self.cos[j * step], self.sin[j * step]);
                    let tr = co * self.re[c] - si * self.im[c];
                    let ti = co * self.im[c] + si * self.re[c];
                    let ar = self.re[a];
                    let ai = self.im[a];
                    self.re[a] = ar + tr;
                    self.im[a] = ai + ti;
                    self.re[c] = ar - tr;
                    self.im[c] = ai - ti;
                }
            }
            size *= 2;
        }
    }
}
