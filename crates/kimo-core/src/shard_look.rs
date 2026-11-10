//! Opt-in shard looks. Pure, bounded functions shared by analytic and physics paths.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Material { #[default] Legacy, Glass, Concrete, Metal }
impl Material {
    pub fn transmissive(self) -> bool { matches!(self, Self::Legacy | Self::Glass) }
}
fn bounded(x: f64, lo: f64, hi: f64, fallback: f64) -> f64 {
    if x.is_finite() { x.clamp(lo, hi) } else { fallback }
}
/// Log-uniform at bias=1 (legacy); bias>1 increases the proportion of fine chips.
/// Scale reduces all dimensions, not just the maximum. Inputs are bounded before powf.
pub fn size(min: f64, max: f64, bias: f64, scale: f64, u: f64) -> f64 {
    let lo = bounded(min, 0.1, 4096.0, 5.0);
    let hi = bounded(max, lo, 4096.0, 58.0_f64.max(lo));
    let bias = bounded(bias, 0.125, 8.0, 1.0);
    let scale = bounded(scale, 0.01, 4.0, 1.0);
    lo * (hi / lo).powf(bounded(u, 0.0, 1.0, 0.5).powf(bias)) * scale
}
fn unit(v: [f64; 3], fallback: [f64; 3]) -> [f64; 3] {
    let m = v.iter().map(|x| x*x).sum::<f64>().sqrt();
    if !m.is_finite() || m < 1e-12 { fallback } else { v.map(|x| x/m) }
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 { a[0]*b[0]+a[1]*b[1]+a[2]*b[2] }
/// World-space sky/ground with a wide studio softbox. +y is down in Kimo.
/// No textures, random state, IO or time dependence; evaluate once per facet.
pub fn environment(normal: [f64; 3], to_eye: [f64; 3], roughness: f64, linear: bool) -> ([f64; 3], f64) {
    let n = unit(normal, [0.0, 0.0, -1.0]); let v = unit(to_eye, [0.0, 0.0, -1.0]);
    let nv = dot(n, v); let r = [2.0*nv*n[0]-v[0], 2.0*nv*n[1]-v[1], 2.0*nv*n[2]-v[2]];
    let rough = bounded(roughness, 0.0, 1.0, 0.15);
    let sky = bounded(0.5-0.5*r[1], 0.0, 1.0, 0.5);
    let sky = sky*sky*(3.0-2.0*sky);
    let width = 0.16+0.6*rough;
    let box_light = (-((r[0]-0.30)/width).powi(2)-((r[1]+0.35)/(width*1.8)).powi(2)).exp()*(1.0-rough)*0.75;
    let ground = [0.28, 0.24, 0.20]; let blue = [0.48, 0.66, 0.92];
    let rgb = std::array::from_fn(|j| {
        let c: f64 = (ground[j]*(1.0-sky)+blue[j]*sky+box_light).min(1.5);
        if !linear { c } else if c <= 0.04045 { c/12.92 } else { ((c+0.055)/1.055).powf(2.4) }
    });
    (rgb, 0.04+0.96*(1.0-nv.abs().min(1.0)).powi(5))
}
/// Distinct diffuse/transmissive/metal response, leaving Legacy bit-for-bit alone.
/// Incoming/outgoing RGB is straight alpha. Coverage and life alpha stay outside.
pub fn shade(material: Material, legacy: [f64; 3], alpha: f64, tint: [f64; 3], brightness: f64,
    normal: [f64; 3], light: [f64; 3], spec: f64, rim: f64, grain: f64, coverage_life: f64) -> ([f64; 3], f64) {
    if material == Material::Legacy { return (legacy, alpha); }
    let tint = tint.map(|x| bounded(x, 0.0, 1.5, 1.0));
    let br = bounded(brightness, 0.0, 8.0, 1.0);
    let diffuse = dot(unit(normal, [0.0,0.0,-1.0]), unit(light, [0.0,0.0,-1.0])).abs();
    let spec = bounded(spec, 0.0, 2.0, 0.0); let rim = bounded(rim, 0.0, 1.0, 0.0);
    let grain = bounded(grain, 0.0, 1.0, 0.5);
    let (rgb, a): ([f64; 3], f64) = match material {
        Material::Glass => (std::array::from_fn(|j| tint[j]*(0.60+0.18*diffuse+0.18*rim)+0.50*spec),
            (0.17+0.15*rim+0.15*spec)*coverage_life),
        Material::Concrete => (std::array::from_fn(|j| tint[j]*(0.46+0.47*diffuse)*(0.86+0.20*grain)+0.025*spec),
            0.985*coverage_life),
        Material::Metal => (std::array::from_fn(|j| tint[j]*(0.24+0.22*diffuse)+spec*(0.45+0.25*tint[j])+0.12*rim),
            0.985*coverage_life),
        Material::Legacy => unreachable!(),
    };
    (rgb.map(|x| (x*br).min(1.5)), a.clamp(0.0,0.985))
}
/// Keep alpha exactly unchanged. Reflections pass through the existing coverage,
/// occlusion and premultiplied over operator, so empty pixels cannot acquire light.
pub fn reflect(rgb: [f64; 3], env: ([f64; 3], f64), amount: f64, material: Material, tint: [f64; 3]) -> [f64; 3] {
    let amount = bounded(amount, 0.0, 1.0, 0.0);
    if amount == 0.0 { return rgb; }
    let response = match material { Material::Concrete => 0.025, Material::Metal => 0.65+0.35*env.1, _ => 0.18+0.82*env.1 };
    let w = amount*response;
    std::array::from_fn(|j| {
        let coat = if material == Material::Metal { bounded(tint[j],0.0,1.5,1.0) } else { 1.0 };
        (rgb[j]*(1.0-w)+env.0[j]*coat*w).min(1.5)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn legacy_sizes_and_zero_identity() {
        for u in [0.0,0.1,0.5,0.9,1.0] {
            assert_eq!(size(5.0,58.0,1.0,1.0,u),5.0*(58.0/5.0_f64).powf(u.powf(1.0)));
        }
        let rgb = [0.25,0.51,1.2];
        assert_eq!(reflect(rgb,([f64::NAN;3],f64::NAN),0.0,Material::Glass,[1.0;3]),rgb);
        assert_eq!(shade(Material::Legacy,rgb,0.37,[f64::NAN;3],f64::NAN,[0.0;3],[0.0;3],0.0,0.0,0.0,0.0),(rgb,0.37));
    }
    #[test] fn fine_distribution() {
        let mean = |b| (0..1000).map(|i| size(2.0,32.0,b,1.0,i as f64/999.0)).sum::<f64>()/1000.0;
        assert!(mean(2.8) < mean(1.0)*0.65);
        assert_eq!(size(2.0,32.0,2.8,0.5,0.7),size(2.0,32.0,2.8,1.0,0.7)*0.5);
    }
    #[test] fn bounded_hostile_inputs() {
        for v in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY,-1e30,0.0,1e30] {
            assert!(size(v,v,v,v,v).is_finite());
            let env = environment([v;3],[v;3],v,true);
            assert!(env.0.iter().all(|x| x.is_finite()));
            let (rgb,a) = shade(Material::Glass,[0.0;3],0.0,[v;3],v,[v;3],[v;3],v,v,v,0.0);
            assert_eq!(a,0.0); assert!(rgb.iter().all(|x| x.is_finite()));
            assert!(reflect(rgb,env,v,Material::Metal,[v;3]).iter().all(|x| x.is_finite()));
        }
    }
    #[test] fn reflections_deterministic_orientation_sensitive() {
        let a = environment([0.0,0.0,-1.0],[0.0,0.0,-1.0],0.15,false);
        assert_eq!(a,environment([0.0,0.0,-1.0],[0.0,0.0,-1.0],0.15,false));
        assert_ne!(a,environment([0.0,0.7,-0.7],[0.0,0.0,-1.0],0.15,false));
    }
}
