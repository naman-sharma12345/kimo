//! Original raw coordinate primitives. No shader code copied; no preset stack.
use serde::{Serialize,Deserialize};
#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq)]pub struct Sample{pub from_uv:[f64;2],pub to_uv:[f64;2],pub from_weight:f64,pub to_weight:f64}
fn validate(uv:[f64;2],progress:f64,direction:[f64;2])->Result<(),String>{if uv.iter().chain(direction.iter()).chain([progress].iter()).any(|x|!x.is_finite())||!(0.0..=1.0).contains(&progress){return Err("finite UV/direction and progress 0..1 required".into());}Ok(())}
/// Outgoing coordinates offset by t*direction. Incoming B tiles the exposed
/// region using fractional outgoing coordinates. Exactly one owner for every
/// finite UV; no transparent gutters. Unit cardinal vectors give an ordinary
/// abutting push. Diagonal/non-unit vectors require tiled incoming corners.
/// Endpoints bypass wrapping and return the original selected coordinates.
pub fn push(uv:[f64;2],progress:f64,direction:[f64;2])->Result<Sample,String>{validate(uv,progress,direction)?;
 if progress==0.0{return Ok(Sample{from_uv:uv,to_uv:uv,from_weight:1.0,to_weight:0.0});}
 if progress==1.0{return Ok(Sample{from_uv:uv,to_uv:uv,from_weight:0.0,to_weight:1.0});}
 let from=[uv[0]+progress*direction[0],uv[1]+progress*direction[1]];
 if from.iter().any(|x|!x.is_finite()){return Err("UV push overflow".into());}
 let inside=from.iter().all(|q|(0.0..1.0).contains(q));
 let to=from.map(|q|{let w=q.rem_euclid(1.0);if w>=1.0{0.0}else{w}});
 let (a,b)=if inside{(1.0,0.0)}else{(0.0,1.0)};
 Ok(Sample{from_uv:from,to_uv:to,from_weight:a,to_weight:b})}
/// A single radial sample along UV-to-center. Caller supplies sample fraction and
/// blend progress independently. This is not a bundled cross-zoom effect.
pub fn radial(uv:[f64;2],center:[f64;2],fraction:f64,strength:f64)->Result<[f64;2],String>{if uv.iter().chain(center.iter()).chain([fraction,strength].iter()).any(|x|!x.is_finite())||!(0.0..=1.0).contains(&fraction){return Err("finite UV/center/strength; fraction 0..1 required".into());}let p=[uv[0]+(center[0]-uv[0])*fraction*strength,uv[1]+(center[1]-uv[1])*fraction*strength];if p.iter().any(|x|!x.is_finite()){return Err("radial UV overflow".into());}Ok(p)}

/// Raw mappings use caller coordinates and explicit radius/axis conventions.
/// Seam policy: atan2 angle maps to u in [0,1), seam at negative X.
/// Both signed-zero sides of the seam map to u=0; callers own texture wrapping.
#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq)]
pub struct Mapping {pub uv:[f64;2],pub singular:bool}
pub fn planar(point:[f64;3],axis_u:u8,axis_v:u8)->Result<Mapping,String>{
 if point.iter().any(|v|!v.is_finite())||axis_u>2||axis_v>2||axis_u==axis_v{return Err("planar requires finite point and two distinct axes 0..2".into());}
 Ok(Mapping{uv:[point[axis_u as usize],point[axis_v as usize]],singular:false})
}
fn azimuth(x:f64,y:f64)->f64{
 let u=(y.atan2(x)/std::f64::consts::TAU+0.5).rem_euclid(1.0);
 // Guard rem_euclid rounding to the upper bound. Seam-near values can
 // quantize to the seam in f64; never return a gap or u=1.
 if u>=1.0{0.0}else{u}
}
pub fn polar(point:[f64;2])->Result<Mapping,String>{
 if point.iter().any(|v|!v.is_finite()){return Err("polar point must be finite".into());}
 let r=point[0].hypot(point[1]);if !r.is_finite(){return Err("polar radius overflow".into());}
 let singular=r==0.0;let u=if singular{0.5}else{azimuth(point[0],point[1])};
 Ok(Mapping{uv:[u,r],singular})
}
pub fn cylindrical(point:[f64;3])->Result<Mapping,String>{
 if point.iter().any(|v|!v.is_finite()){return Err("cylindrical point must be finite".into());}
 let p=polar([point[0],point[2]])?;Ok(Mapping{uv:[p.uv[0],point[1]],singular:p.singular})
}
pub fn spherical(point:[f64;3])->Result<Mapping,String>{
 if point.iter().any(|v|!v.is_finite()){return Err("spherical point must be finite".into());}
 // Normalize before computing the radius: avoid overflow and preserve tiny
 // nonzero directions. Division is reached only after the zero-scale guard.
 let scale=point.iter().fold(0.0_f64,|m,v|m.max(v.abs()));
 if scale==0.0{return Ok(Mapping{uv:[0.5,0.5],singular:true});}
 let q=[point[0]/scale,point[1]/scale,point[2]/scale];
 let r=q[0].hypot(q[1]).hypot(q[2]);
 let axis=point[0]==0.0&&point[2]==0.0;
 let u=if axis{0.5}else{azimuth(point[0],point[2])};
 let v=(q[1]/r).clamp(-1.0,1.0).acos()/std::f64::consts::PI;
 Ok(Mapping{uv:[u,v],singular:axis})
}
