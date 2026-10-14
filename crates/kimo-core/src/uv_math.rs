//! Original raw coordinate primitives. No shader code copied; no preset stack.
use serde::{Serialize,Deserialize};
#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq)]pub struct Sample{pub from_uv:[f64;2],pub to_uv:[f64;2],pub from_weight:f64,pub to_weight:f64}
fn validate(uv:[f64;2],progress:f64,direction:[f64;2])->Result<(),String>{if uv.iter().chain(direction.iter()).chain([progress].iter()).any(|x|!x.is_finite())||!(0.0..=1.0).contains(&progress){return Err("finite UV/direction and progress 0..1 required".into());}Ok(())}
/// Continuous direction magnitude, no component sign snapping. Each image slides
/// by the specified vector; callers own sampling/edges. Exact endpoints defined.
pub fn push(uv:[f64;2],progress:f64,direction:[f64;2])->Result<Sample,String>{validate(uv,progress,direction)?;
 let from=[uv[0]+progress*direction[0],uv[1]+progress*direction[1]];
 let to=[uv[0]-(1.0-progress)*direction[0],uv[1]-(1.0-progress)*direction[1]];
 if from.iter().chain(to.iter()).any(|x|!x.is_finite()){return Err("UV push overflow".into());}
 let inside=|q:[f64;2]|(0.0..=1.0).contains(&q[0])&&(0.0..=1.0).contains(&q[1]);
 let (a,b)=if progress==0.0{(1.0,0.0)}else if progress==1.0{(0.0,1.0)}else if inside(from){(1.0,0.0)}else if inside(to){(0.0,1.0)}else{(0.0,0.0)};
 Ok(Sample{from_uv:from,to_uv:to,from_weight:a,to_weight:b})}
/// A single radial sample along UV-to-center. Caller supplies sample fraction and
/// blend progress independently. This is not a bundled cross-zoom effect.
pub fn radial(uv:[f64;2],center:[f64;2],fraction:f64,strength:f64)->Result<[f64;2],String>{if uv.iter().chain(center.iter()).chain([fraction,strength].iter()).any(|x|!x.is_finite())||!(0.0..=1.0).contains(&fraction){return Err("finite UV/center/strength; fraction 0..1 required".into());}let p=[uv[0]+(center[0]-uv[0])*fraction*strength,uv[1]+(center[1]-uv[1])*fraction*strength];if p.iter().any(|x|!x.is_finite()){return Err("radial UV overflow".into());}Ok(p)}

/// Raw mappings use caller coordinates and explicit radius/axis conventions.
/// Seam policy: atan2 angle maps to u in [0,1], seam at negative X (polar) or -X/+Z azimuth.
#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq)]
pub struct Mapping {pub uv:[f64;2],pub singular:bool}
pub fn planar(point:[f64;3],axis_u:u8,axis_v:u8)->Result<Mapping,String>{
 if point.iter().any(|v|!v.is_finite())||axis_u>2||axis_v>2||axis_u==axis_v{return Err("planar requires finite point and two distinct axes 0..2".into());}
 Ok(Mapping{uv:[point[axis_u as usize],point[axis_v as usize]],singular:false})
}
pub fn polar(point:[f64;2])->Result<Mapping,String>{
 if point.iter().any(|v|!v.is_finite()){return Err("polar point must be finite".into());}
 let r=point[0].hypot(point[1]);if !r.is_finite(){return Err("polar radius overflow".into());}
 let singular=r==0.0;let u=if singular{0.5}else{point[1].atan2(point[0])/std::f64::consts::TAU+0.5};
 Ok(Mapping{uv:[u,r],singular})
}
pub fn cylindrical(point:[f64;3])->Result<Mapping,String>{
 if point.iter().any(|v|!v.is_finite()){return Err("cylindrical point must be finite".into());}
 let p=polar([point[0],point[2]])?;Ok(Mapping{uv:[p.uv[0],point[1]],singular:p.singular})
}
pub fn spherical(point:[f64;3])->Result<Mapping,String>{
 if point.iter().any(|v|!v.is_finite()){return Err("spherical point must be finite".into());}
 let r=point[0].hypot(point[1]).hypot(point[2]);if !r.is_finite(){return Err("spherical radius overflow".into());}
 if r==0.0{return Ok(Mapping{uv:[0.5,0.5],singular:true});}
 let axis=point[0]==0.0&&point[2]==0.0;
 let u=if axis{0.5}else{point[2].atan2(point[0])/std::f64::consts::TAU+0.5};
 let v=(point[1]/r).clamp(-1.0,1.0).acos()/std::f64::consts::PI;
 Ok(Mapping{uv:[u,v],singular:axis})
}
