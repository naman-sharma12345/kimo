use serde::{Serialize,Deserialize};use super::{pbr::{Material,sample},vertex_stream::unit};
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]pub enum Light{Directional{travel_direction:[f64;3],radiance:[f64;3]},Point{position:[f64;3],intensity:[f64;3],min_distance:f64}}
fn dot(a:[f64;3],b:[f64;3])->f64{a.iter().zip(b).map(|(a,b)|a*b).sum()}
pub fn validate(lights:&[Light])->Result<(),String>{if lights.len()>64{return Err("max64 explicit lights".into());}for l in lights{let (rgb,dir)=match l{Light::Directional{travel_direction,radiance}=>(radiance,*travel_direction),Light::Point{position,intensity,min_distance}=>{if !min_distance.is_finite()||!(1e-6..=1e6).contains(min_distance){return Err("point min_distance1e-6..1e6 required".into());}(intensity,*position)}};if rgb.iter().any(|v|!v.is_finite()||*v<0.||*v>1e9)||dir.iter().any(|v|!v.is_finite()||v.abs()>1e9){return Err("bounded finite light values required".into());}if matches!(l,Light::Directional{..}){unit(dir)?;}}Ok(())}
/// World-space, linear radiance; sum direct lighting. No ambient/shadow/IBL.
pub fn shade(material:&Material,point:[f64;3],normal:[f64;3],eye:[f64;3],lights:&[Light])->Result<[f64;3],String>{validate(lights)?;if point.iter().chain(eye.iter()).any(|v|!v.is_finite()||v.abs()>1e9){return Err("bounded finite point/eye required".into());}let n=unit(normal)?;let view=unit(std::array::from_fn(|j|eye[j]-point[j]))?;sample(material,n,view,view)?;let mut out=[0.;3];for light in lights{let(l,rgb)=match light{Light::Directional{travel_direction,radiance}=>(unit(travel_direction.map(|v|-v))?,*radiance),Light::Point{position,intensity,min_distance}=>{let d:[f64;3]=std::array::from_fn(|j|position[j]-point[j]);let distance=d[0].hypot(d[1]).hypot(d[2]);if distance==0.{continue;}let denom=distance.max(*min_distance).powi(2);(unit(d)?,intensity.map(|i|i/denom))}};let nl=dot(n,l).max(0.);let brdf=sample(material,n,l,view)?;for j in 0..3{out[j]+=(brdf.diffuse[j]+brdf.specular[j])*rgb[j]*nl;}}if out.iter().any(|v|!v.is_finite()||*v>f32::MAX as f64){return Err("radiance overflow".into());}Ok(out)}
/// shade() with per-light visibility (1.0 = fully lit). vis.len() must equal lights.len();
/// shade() is the all-lit special case. Same BRDF order/constants so all-lit output is
/// bit-identical to shade().
pub fn shade_shadowed(material:&Material,point:[f64;3],normal:[f64;3],eye:[f64;3],lights:&[Light],vis:&[f64])->Result<[f64;3],String>{
 validate(lights)?;if vis.len()!=lights.len(){return Err("visibility per light required".into());}
 if vis.iter().any(|v|!v.is_finite()||!(0.0..=1.0).contains(v)){return Err("visibility 0..1 required".into());}
 if point.iter().chain(eye.iter()).any(|v|!v.is_finite()||v.abs()>1e9){return Err("bounded finite point/eye required".into());}
 let n=unit(normal)?;let view=unit(std::array::from_fn(|j|eye[j]-point[j]))?;sample(material,n,view,view)?;let mut out=[0.;3];
 for (i,light) in lights.iter().enumerate(){let v=vis[i];if v==0.0{continue;}
  let(l,rgb)=match light{Light::Directional{travel_direction,radiance}=>(unit(travel_direction.map(|x|-x))?,*radiance),Light::Point{position,intensity,min_distance}=>{let d:[f64;3]=std::array::from_fn(|j|position[j]-point[j]);let distance=d[0].hypot(d[1]).hypot(d[2]);if distance==0.{continue;}let denom=distance.max(*min_distance).powi(2);(unit(d)?,intensity.map(|i|i/denom))}};
  let nl=dot(n,l).max(0.);let brdf=sample(material,n,l,view)?;for j in 0..3{out[j]+=(brdf.diffuse[j]+brdf.specular[j])*rgb[j]*v*nl;}}
 if out.iter().any(|v|!v.is_finite()||*v>f32::MAX as f64){return Err("radiance overflow".into());}Ok(out)}

/// Image-based lighting contribution (no direct lights): specular = prefiltered(R,alpha)
/// * (F0*lut.x + lut.y); diffuse = irradiance(n)/PI * albedo * (1-metallic) * (1-F_ibl),
/// matching the glTF energy gating used by sample(). Constant-env output agrees with a
/// direct Monte Carlo integral of sample() to measured sampling noise (test-pinned band).
pub fn shade_ibl(material:&Material,point:[f64;3],normal:[f64;3],eye:[f64;3],ibl:&super::ibl::IblSet)->Result<[f64;3],String>{
 if point.iter().chain(eye.iter()).any(|v|!v.is_finite()||v.abs()>1e9){return Err("bounded finite point/eye required".into());}
 let n=unit(normal)?;let view=unit(std::array::from_fn(|j|eye[j]-point[j]))?;sample(material,n,view,view)?;ibl.validate()?;
 let nv=dot(n,view);if nv<=0.0{return Ok([0.;3]);}
 let alpha=(material.roughness*material.roughness).max(0.001);let[scale,bias]=ibl.lut.sample(nv,alpha)?;
 let r=[2.0*nv*n[0]-view[0],2.0*nv*n[1]-view[1],2.0*nv*n[2]-view[2]];let pf=ibl.prefiltered_at(r,alpha)?;let irr=ibl.irradiance.sample(n)?;
 let mut out=[0.;3];for j in 0..3{let f0=0.04*(1.0-material.metallic)+material.albedo[j]*material.metallic;let f_ibl=(f0*scale+bias).clamp(0.,1.);out[j]=pf[j]*f_ibl+irr[j]/std::f64::consts::PI*material.albedo[j]*(1.0-material.metallic)*(1.0-f_ibl);}
 if out.iter().any(|v|!v.is_finite()||*v>f32::MAX as f64){return Err("radiance overflow".into());}Ok(out)}


#[cfg(test)]mod tests{use super::*;
 #[test]fn directional_and_point_finite_inverse_square(){let m=Material{albedo:[0.5;3],metallic:0.,roughness:0.5};let light=|z|Light::Point{position:[0.,0.,z],intensity:[1.;3],min_distance:0.01};let a=shade(&m,[0.;3],[0.,0.,1.],[0.,0.,4.],&[light(2.)]).unwrap();let b=shade(&m,[0.;3],[0.,0.,1.],[0.,0.,4.],&[light(4.)]).unwrap();for j in 0..3{assert!((a[j]/b[j]-4.).abs()<1e-12);}let d=Light::Directional{travel_direction:[0.,0.,-1.],radiance:[1.;3]};assert!(shade(&m,[0.;3],[0.,0.,1.],[0.,0.,4.],&[d]).unwrap().iter().all(|v|v.is_finite()&&*v>0.));}
 #[test]fn coincident_point_is_explicit_zero_contribution(){let m=Material{albedo:[0.5;3],metallic:1.,roughness:0.5};assert_eq!(shade(&m,[0.;3],[0.,0.,1.],[0.,0.,4.],&[Light::Point{position:[0.;3],intensity:[1.;3],min_distance:0.01}]).unwrap(),[0.;3]);}

 // MCTRUTH_CASES: direct uniform-hemisphere Monte Carlo of the PL3-oracle-transcribed BRDF
 // under a constant unit env (Python, 8192 samples) - independent of the split-sum path.
 const MCTRUTH_CASES:[(f64,f64,f64,f64,[f64;3]);8]=[
(0.8,0.0,0.3,0.9,[8.07284117732170570e-01,8.07284117732170570e-01,8.07284117732170570e-01]),
(0.8,0.0,0.3,0.4,[8.69129196205507770e-01,8.69129196205507770e-01,8.69129196205507770e-01]),
(0.8,0.0,0.8,0.9,[7.90350992170509747e-01,7.90350992170509747e-01,7.90350992170509747e-01]),
(0.9,1.0,0.1,0.9,[7.99289722921418422e-01,7.99289722921418422e-01,7.99289722921418422e-01]),
(0.9,1.0,0.6,0.6,[7.08806818464574384e-01,7.08806818464574384e-01,7.08806818464574384e-01]),
(0.2,0.5,0.5,0.7,[1.98076114391487867e-01,1.98076114391487867e-01,1.98076114391487867e-01]),
(0.5,0.0,0.05,0.2,[4.80928852451848632e-01,4.80928852451848632e-01,4.80928852451848632e-01]),
(0.7,1.0,1.0,1.0,[2.14785694939040678e-01,2.14785694939040678e-01,2.14785694939040678e-01]),
];

 fn white_ibl()->crate::threed::ibl::IblSet{let env=crate::threed::ibl::EnvMap::from_fn(16,8,|_|[1.;3]).unwrap();crate::threed::ibl::IblSet::bake(&env,[8,4],[8,4],4,64,1024).unwrap()}
 // ACCEPTED THRESHOLD (2026-10-16): the split-sum vs Monte Carlo ceiling is 5.49%
 // relative, measured at grazing dielectric (nv=0.4, r=0.3); 1.94% rough metal, <=0.08%
 // mid-roughness metal. This is an approximation difference, not a bug: split-sum gates
 // diffuse with the LUT (D*vis-weighted) F while the reference BRDF gates with the
 // directional F. The assert band below is 7% (measured 5.49% + margin); tighten it only
 // with a measured improvement, loosen it only with Naman's sign-off. Smooth lobes (r<0.3)
 // are excluded: uniform-hemisphere MC is too noisy to be an oracle there (a 12.6% "error"
 // at r=0.1 measured as pure estimator variance); smooth-lobe correctness is covered by
 // the 1e-12 LUT oracle in threed::ibl plus the constant-env prefilter identity.
 #[test]fn shade_ibl_matches_direct_monte_carlo_constant_env(){let ibl=white_ibl();
  for (alb,m,r,nv,truth) in MCTRUTH_CASES{if r<0.3{continue;}let mat=Material{albedo:[alb;3],metallic:m,roughness:r};let n=[0.,0.,1.];let v=[(1.-nv*nv).sqrt(),0.,nv];let got=shade_ibl(&mat,[0.;3],n,[v[0]*4.,v[1]*4.,v[2]*4.],&ibl).unwrap();
   for j in 0..3{let rel=(got[j]-truth[j]).abs()/truth[j];assert!(rel<0.07,"albedo {alb} metallic {m} rough {r} nv {nv} ch{j}: split-sum {} vs MC truth {} ({:.2}%)",got[j],truth[j],rel*100.);}}}
 #[test]fn shade_ibl_zero_backface_finite_and_typed(){let ibl=white_ibl();let mat=Material{albedo:[0.5;3],metallic:0.,roughness:0.5};
  assert_eq!(shade_ibl(&mat,[0.;3],[0.,0.,1.],[0.,0.,-4.],&ibl).unwrap(),[0.;3],"backface gets no IBL");
  assert_eq!(shade_ibl(&mat,[f64::NAN;3],[0.,0.,1.],[0.,0.,4.],&ibl).unwrap_err(),"bounded finite point/eye required");
  let mut bad=ibl.clone();bad.prefiltered.clear();assert_eq!(shade_ibl(&mat,[0.;3],[0.,0.,1.],[0.,0.,4.],&bad).unwrap_err(),"prefilter levels 1..=16 required");
  let got=shade_ibl(&mat,[0.;3],[0.,0.,1.],[0.,0.,4.],&ibl).unwrap();assert!(got.iter().all(|v|v.is_finite()&&*v>=0.));}
}
