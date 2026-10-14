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
