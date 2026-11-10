//! 3D particle advection fields composed from the original 2D analytic potentials.
//! curl3 is divergence-free by construction: three orthogonal planar C2 potentials
//! A1(y,z), A2(z,x), A3(x,y), v = curl(A); divergence vanishes by equality of mixed
//! partials (quintic Perlin and the simplex kernel are C2 here). potential3 and
//! worley3 are compressible gradient fields and are labelled as such. No imported
//! algorithms or tables; reuses noise/noise_fields analytic gradients only.
use serde::{Deserialize, Serialize};
use crate::noise::Kind;
use crate::noise_fields::{worley, Fbm};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Advect {
    /// Divergence-free curl of three planar FBM potentials.
    Curl3 { noise: String, seed: u64, frequency: f64, amplitude: f64, octaves: u32 },
    /// Compressible gradient of the summed planar potentials (NOT solenoidal).
    Potential3 { noise: String, seed: u64, frequency: f64, amplitude: f64, octaves: u32 },
    /// Compressible Worley F2-F1 ridge-gradient flow (NOT solenoidal).
    Worley3 { seed: u64, frequency: f64, amplitude: f64 },
}

const LANE: [u64; 3] = [0x9e3779b97f4a7c15, 0xc2b2ae3d27d4eb4f, 0x165667b19e3779f9];

fn lane_seed(seed: u64, lane: usize) -> u64 {
    seed.wrapping_mul(0xFF51AFD7ED558CCD).wrapping_add(LANE[lane])
}

fn fbm(noise: &str, seed: u64, octaves: u32) -> Result<Fbm, String> {
    let f = Fbm { kind: Kind::parse(noise)?, seed, octaves, lacunarity: 2.0, gain: 0.5, normalize: false };
    f.validate()?;
    Ok(f)
}

impl Advect {
    pub fn solenoidal(&self) -> bool {
        matches!(self, Self::Curl3 { .. })
    }
    fn params(&self) -> (f64, f64) {
        match self {
            Self::Curl3 { frequency, amplitude, .. }
            | Self::Potential3 { frequency, amplitude, .. }
            | Self::Worley3 { frequency, amplitude, .. } => (*frequency, *amplitude),
        }
    }
    fn validate(&self) -> Result<(), String> {
        let (frequency, amplitude) = self.params();
        if !frequency.is_finite() || frequency <= 0.0 || frequency > 1e6 {
            return Err("frequency must be in (0, 1e6]".into());
        }
        if !amplitude.is_finite() || amplitude.abs() > 1e12 {
            return Err("amplitude must be finite within +/-1e12".into());
        }
        if let Self::Curl3 { noise, seed, octaves, .. } | Self::Potential3 { noise, seed, octaves, .. } = self {
            for lane in 0..3 {
                fbm(noise, lane_seed(*seed, lane), *octaves)?;
            }
        }
        Ok(())
    }
    /// Conservative absolute velocity bound (budget/oracle use; intentionally loose).
    pub fn bound(&self) -> f64 {
        let (frequency, amplitude) = self.params();
        match self {
            // Per-octave gradient: amplitude 0.5^o, frequency 2^o (net 1.0); component bound 32 (noise.rs).
            Self::Curl3 { octaves, .. } | Self::Potential3 { octaves, .. } => {
                2.0 * amplitude.abs() * frequency * 32.0 * *octaves as f64
            }
            // Worley gradients are unit-or-less; F2-F1 difference <= 2 per component per
            // plane, and each output component sums two planes: component <= 4*A*f.
            Self::Worley3 { .. } => 4.0 * amplitude.abs() * frequency,
        }
    }
    /// Sample the advection velocity at a 3D position.
    pub fn velocity(&self, p: [f64; 3]) -> Result<[f64; 3], String> {
        self.validate()?;
        if p.iter().any(|v| !v.is_finite() || v.abs() > 1e6) {
            return Err("position must be finite within +/-1e6".into());
        }
        let (frequency, amplitude) = self.params();
        let (x, y, z) = (p[0] * frequency, p[1] * frequency, p[2] * frequency);
        if [x, y, z].iter().any(|v| v.abs() > 1e9) {
            return Err("scaled field coordinates exceed +/-1e9".into());
        }
        let out = match self {
            Self::Curl3 { noise, seed, octaves, .. } | Self::Potential3 { noise, seed, octaves, .. } => {
                let g = |lane: usize, u: f64, v: f64| -> Result<[f64; 2], String> {
                    Ok(fbm(noise, lane_seed(*seed, lane), *octaves)?.sample(u, v)?.gradient)
                };
                let g1 = g(0, y, z)?; // A1(y,z): g1 = (dA1/dy, dA1/dz)
                let g2 = g(1, z, x)?; // A2(z,x): g2 = (dA2/dz, dA2/dx)
                let g3 = g(2, x, y)?; // A3(x,y): g3 = (dA3/dx, dA3/dy)
                if self.solenoidal() {
                    [g3[1] - g2[0], g1[1] - g3[0], g2[1] - g1[0]]
                } else {
                    [g2[1] + g3[0], g1[0] + g3[1], g1[1] + g2[0]]
                }
            }
            Self::Worley3 { seed, .. } => {
                let g = |lane: usize, u: f64, v: f64| -> Result<[f64; 2], String> {
                    let w = worley(lane_seed(*seed, lane), u, v)?;
                    Ok([w.gradient2[0] - w.gradient1[0], w.gradient2[1] - w.gradient1[1]])
                };
                let g1 = g(0, y, z)?;
                let g2 = g(1, z, x)?;
                let g3 = g(2, x, y)?;
                [g2[1] + g3[0], g1[0] + g3[1], g1[1] + g2[0]]
            }
        };
        let v = out.map(|v| v * frequency * amplitude);
        if v.iter().any(|v| !v.is_finite()) {
            return Err("advection velocity overflow".into());
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fbm_at(f: &Advect, lane: usize) -> Fbm {
        match f {
            Advect::Curl3 { noise, seed, octaves, .. } | Advect::Potential3 { noise, seed, octaves, .. } => {
                fbm(noise, lane_seed(*seed, lane), *octaves).unwrap()
            }
            Advect::Worley3 { .. } => unreachable!(),
        }
    }
    /// Phi(p) = A1(y,z)+A2(z,x)+A3(x,y) from values only (independent of gradients).
    fn potential_value(f: &Advect, p: [f64; 3]) -> f64 {
        let (frequency, _) = f.params();
        let (x, y, z) = (p[0] * frequency, p[1] * frequency, p[2] * frequency);
        fbm_at(f, 0).sample(y, z).unwrap().value
            + fbm_at(f, 1).sample(z, x).unwrap().value
            + fbm_at(f, 2).sample(x, y).unwrap().value
    }

    #[test]
    fn curl3_components_match_value_only_finite_differences() {
        // v = amplitude*frequency*(dA3/dy - dA2/dz, dA1/dz - dA3/dx, dA2/dx - dA1/dy)
        // checked against central differences of potential VALUES (no analytic gradients).
        let f = Advect::Curl3 { noise: "perlin".into(), seed: 42, frequency: 0.7, amplitude: 2.0, octaves: 3 };
        let (frequency, amplitude) = f.params();
        let h = 1e-5;
        for p in [[0.3, -1.2, 0.8], [-0.9, 0.2, 1.7], [2.1, 0.4, -0.6]] {
            let (x, y, z) = (p[0] * frequency, p[1] * frequency, p[2] * frequency);
            let cd = |s: &Fbm, a: f64, b: f64, c: f64, d: f64| {
                (s.sample(a, b).unwrap().value - s.sample(c, d).unwrap().value) / (2.0 * h)
            };
            let (f1, f2, f3) = (fbm_at(&f, 0), fbm_at(&f, 1), fbm_at(&f, 2));
            let expect = [
                cd(&f3, x, y + h, x, y - h) - cd(&f2, z + h, x, z - h, x),
                cd(&f1, y, z + h, y, z - h) - cd(&f3, x + h, y, x - h, y),
                cd(&f2, z, x + h, z, x - h) - cd(&f1, y + h, z, y - h, z),
            ]
            .map(|v| v * frequency * amplitude);
            let got = f.velocity(p).unwrap();
            for axis in 0..3 {
                assert!((got[axis] - expect[axis]).abs() < 1e-4, "axis {axis}: got {} expect {}", got[axis], expect[axis]);
            }
        }
    }
    #[test]
    fn curl3_divergence_vanishes_finite_difference() {
        let f = Advect::Curl3 { noise: "simplex".into(), seed: 5, frequency: 0.9, amplitude: 1.0, octaves: 4 };
        // h=1e-5: central-difference truncation O(h^2 * D3) ~1e-7, roundoff O(eps*|v|/h) ~1e-9.
        let h = 1e-5;
        for p in [[0.3, -1.2, 0.8], [-0.9, 0.2, 1.7]] {
            let div = (0..3)
                .map(|axis| {
                    let mut pp = p;
                    pp[axis] += h;
                    let mut pm = p;
                    pm[axis] -= h;
                    (f.velocity(pp).unwrap()[axis] - f.velocity(pm).unwrap()[axis]) / (2.0 * h)
                })
                .sum::<f64>();
            eprintln!("divergence {div:e} at {p:?}");
            assert!(div.abs() < 1e-5, "divergence {div} at {p:?}");
        }
    }
    #[test]
    fn potential3_is_value_gradient() {
        let f = Advect::Potential3 { noise: "simplex".into(), seed: 9, frequency: 0.9, amplitude: 1.2, octaves: 2 };
        let (_, amplitude) = f.params();
        let h = 1e-5;
        for p in [[0.4, 0.7, -1.1], [-1.3, 0.5, 0.6]] {
            let v = f.velocity(p).unwrap();
            for axis in 0..3 {
                let mut pp = p;
                pp[axis] += h;
                let mut pm = p;
                pm[axis] -= h;
                // v = amplitude * dPhi/dp (frequency chain rule already inside velocity).
                let expect = (potential_value(&f, pp) - potential_value(&f, pm)) / (2.0 * h) * amplitude;
                assert!((v[axis] - expect).abs() < 1e-4, "axis {axis}: got {} expect {expect}", v[axis]);
            }
        }
    }
    #[test]
    fn deterministic_repeat_and_rejections() {
        let f = Advect::Curl3 { noise: "perlin".into(), seed: 42, frequency: 0.7, amplitude: 2.0, octaves: 3 };
        assert_eq!(f.velocity([0.1, 0.2, 0.3]).unwrap(), f.velocity([0.1, 0.2, 0.3]).unwrap());
        assert!(f.velocity([f64::NAN, 0., 0.]).is_err());
        assert!(f.velocity([2e6, 0., 0.]).is_err());
        assert!(Advect::Curl3 { noise: "perlin".into(), seed: 1, frequency: 0.0, amplitude: 1.0, octaves: 3 }.velocity([0.; 3]).is_err());
        assert!(Advect::Curl3 { noise: "perlin".into(), seed: 1, frequency: 1.0, amplitude: 1.0, octaves: 17 }.velocity([0.; 3]).is_err());
        assert!(Advect::Curl3 { noise: "bogus".into(), seed: 1, frequency: 1.0, amplitude: 1.0, octaves: 3 }.velocity([0.; 3]).is_err());
    }
    #[test]
    fn worley3_bounded_deterministic() {
        let f = Advect::Worley3 { seed: 7, frequency: 1.3, amplitude: 1.5 };
        let b = f.bound();
        for i in 0..40 {
            let p = [i as f64 * 0.37 - 6.0, i as f64 * 0.11 - 2.0, i as f64 * 0.23 - 4.0];
            let v = f.velocity(p).unwrap();
            assert!(v.iter().all(|c| c.abs() <= b * (1.0 + 1e-12)), "{v:?} exceeds {b}");
            assert_eq!(v, f.velocity(p).unwrap());
        }
        assert!(!f.solenoidal());
        assert!(Advect::Curl3 { noise: "perlin".into(), seed: 1, frequency: 1.0, amplitude: 1.0, octaves: 1 }.solenoidal());
    }
}
