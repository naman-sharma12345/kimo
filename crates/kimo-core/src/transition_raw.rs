//! Software transition primitives adapted from gl-transitions, pinned in docs/transitions/WIP.md.
//! MIT authors: gre (luma), pschroen (directionalwarp), rectalogic/gre
//! (CrossZoom, lineage glfx.js), Travis Fischer (displacement, Robin Delaporte lineage).
//! See GL_TRANSITIONS_LICENSE.txt. Sampler and validation are original Rust.
use serde::{Deserialize,Serialize};
use crate::effects::Frame;
#[derive(Clone,Copy,Debug,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Edge {Clamp,Repeat,Transparent}
/// Raw controls, not bundled creative presets or automatic easing.
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
pub enum Operator {
 Crossfade,
 Slide {from_offset:[f64;2],to_offset:[f64;2]},
 Luma {smoothing:f64},
 DirectionalWarp {direction:[f64;2],smoothing:f64,distortion:f64},
 CrossZoom {center:[f64;2],strength:f64,samples:u16},
 Displacement {strength:[f64;2],neutral:f64},
}
fn valid(f:&Frame)->Result<(),String>{
 let n=f.w.checked_mul(f.h).and_then(|n|n.checked_mul(4)).ok_or("frame size overflow")?;
 if f.w==0||f.h==0||n!=f.d.len()||n>4*4096*4096{return Err("nonempty RGBA frame up to 4096x4096 required".into());}
 if f.d.chunks_exact(4).any(|p|p.iter().any(|v|!v.is_finite())||!(0.0..=1.0).contains(&p[3])){return Err("finite RGB and alpha 0..1 required".into());}Ok(())
}
fn smooth(a:f64,b:f64,x:f64)->f64{if a==b{return if x<a{0.0}else{1.0};}let t=((x-a)/(b-a)).clamp(0.0,1.0);t*t*(3.0-2.0*t)}
/// Pixel-center UV convention, bilinear sampling in premultiplied space.
/// Returned values stay premultiplied until final conversion; hidden RGB at alpha0 is ignored.
fn sample(f:&Frame,uv:[f64;2],edge:Edge)->Result<[f64;4],String>{
 if uv.iter().any(|v|!v.is_finite()){return Err("UV arithmetic overflow".into());}
 let q=uv.map(|v|match edge{Edge::Repeat=>v.rem_euclid(1.0),Edge::Clamp=>v.clamp(0.0,1.0),Edge::Transparent=>v});
 if matches!(edge,Edge::Transparent)&&(q[0]< -1.0||q[0]>2.0||q[1]< -1.0||q[1]>2.0){return Ok([0.0;4]);}
 let x=q[0]*f.w as f64-0.5;let y=q[1]*f.h as f64-0.5;let ix=x.floor() as i64;let iy=y.floor() as i64;let dx=x-x.floor();let dy=y-y.floor();let mut out=[0.0;4];
 for (oy,wy) in [(0,1.0-dy),(1,dy)]{for (ox,wx) in [(0,1.0-dx),(1,dx)]{
  let (mut px,mut py)=(ix+ox,iy+oy);
  match edge{Edge::Clamp=>{px=px.clamp(0,f.w as i64-1);py=py.clamp(0,f.h as i64-1);},Edge::Repeat=>{px=px.rem_euclid(f.w as i64);py=py.rem_euclid(f.h as i64);},Edge::Transparent=>{if px<0||py<0||px>=f.w as i64||py>=f.h as i64{continue;}}}
  let i=(py as usize*f.w+px as usize)*4;let a=f.d[i+3] as f64;let w=wx*wy;for c in 0..3{out[c]+=f.d[i+c] as f64*a*w;}out[3]+=a*w;
 }}Ok(out)
}
fn mix(a:[f64;4],b:[f64;4],t:f64)->[f64;4]{std::array::from_fn(|i|a[i]*(1.0-t)+b[i]*t)}
/// A/B straight RGBA, optional same-size raw scalar map in the red channel.
/// No colorspace inference. Caller supplies matching color domains.
pub fn render(a:&Frame,b:&Frame,map:Option<&Frame>,t:f64,op:&Operator,edge:Edge)->Result<Frame,String>{
 valid(a)?;valid(b)?;if (a.w,a.h)!=(b.w,b.h){return Err("input sizes differ".into());}
 if !t.is_finite()||!(0.0..=1.0).contains(&t){return Err("progress 0..1 required".into());}
 let scalar=matches!(op,Operator::Luma{..}|Operator::Displacement{..});
 if scalar{let f=map.ok_or("scalar map required")?;valid(f)?;if (f.w,f.h)!=(a.w,a.h){return Err("scalar map size differs".into());}}
 match op{
 Operator::Crossfade=>{},
 Operator::Slide{from_offset,to_offset}=>{if from_offset.iter().chain(to_offset.iter()).any(|v|!v.is_finite()){return Err("finite slide UV offsets required".into());}},
 Operator::Luma{smoothing}=>{if !smoothing.is_finite()||!(0.0..=1.0).contains(smoothing){return Err("smoothing 0..1 required".into());}if map.unwrap().d.chunks_exact(4).any(|p|!(0.0..=1.0).contains(&p[0])){return Err("luma map red 0..1 required".into());}},
 Operator::DirectionalWarp{direction,smoothing,distortion}=>{if direction.iter().any(|v|!v.is_finite())||direction.iter().all(|v|*v==0.0)||!smoothing.is_finite()||!(0.0..=1.0).contains(smoothing)||!distortion.is_finite(){return Err("finite nonzero direction, smoothing0..1 and finite distortion required".into());}},
 Operator::CrossZoom{center,strength,samples}=>{if center.iter().any(|v|!v.is_finite())||!strength.is_finite()||!(1..=128).contains(samples){return Err("finite center/strength, samples1..128 required".into());}},
 Operator::Displacement{strength,neutral}=>{if strength.iter().any(|v|!v.is_finite())||!neutral.is_finite(){return Err("finite displacement controls required".into());}},
 }
 // Endpoints bypass filtering, preserving exact original bytes, including hidden RGB.
 if t==0.0||t==1.0{let f=if t==0.0{a}else{b};return Ok(Frame{w:f.w,h:f.h,d:f.d.clone()});}
 let mut out=Frame::try_new(a.w,a.h)?;
 for y in 0..a.h{crate::guard::check();for x in 0..a.w{
 let uv=[(x as f64+0.5)/a.w as f64,(y as f64+0.5)/a.h as f64];let i=(y*a.w+x)*4;
 let c=match op{
 Operator::Crossfade=>{let premult=|f:&Frame|{let alpha=f.d[i+3] as f64;[f.d[i] as f64*alpha,f.d[i+1] as f64*alpha,f.d[i+2] as f64*alpha,alpha]};mix(premult(a),premult(b),t)},
 Operator::Slide{from_offset,to_offset}=>{let u=std::array::from_fn(|j|uv[j]+t*from_offset[j]);let v=std::array::from_fn(|j|uv[j]+(1.0-t)*to_offset[j]);mix(sample(a,u,edge)?,sample(b,v,edge)?,t)},
 Operator::Luma{smoothing}=>{let l=map.unwrap().d[i] as f64;let w=1.0-smooth(t-smoothing*0.5,t+smoothing*0.5,l);mix(sample(a,uv,edge)?,sample(b,uv,edge)?,w)},
 Operator::Displacement{strength,neutral}=>{let d=map.unwrap().d[i] as f64-neutral;let u=std::array::from_fn(|j|uv[j]+t*d*strength[j]);let v=std::array::from_fn(|j|uv[j]-(1.0-t)*d*strength[j]);mix(sample(a,u,edge)?,sample(b,v,edge)?,t)},
 Operator::DirectionalWarp{direction,smoothing,distortion}=>{
 let scale=direction[0].abs().max(direction[1].abs());let q=direction.map(|v|v/scale);let sum=q[0].abs()+q[1].abs();let v=q.map(|z|z/sum);
 let d=0.5*(v[0]+v[1]);let m=1.0-smooth(-*smoothing,0.0,v[0]*uv[0]+v[1]*uv[1]-(d-0.5+t*(1.0+smoothing)));
 let u=uv.map(|z|(z-0.5)*(1.0-m*distortion)+0.5);let v=uv.map(|z|(z-0.5)*(1.0-(1.0-m)*distortion)+0.5);mix(sample(a,u,edge)?,sample(b,v,edge)?,m)},
 Operator::CrossZoom{center,strength,samples}=>{
 let mut acc=[0.0;4];let mut total=0.0;
 // Deterministic midpoint quadrature, no random jitter or implicit strength envelope.
 for k in 0..*samples{let p=(k as f64+0.5)/ *samples as f64;let w=4.0*p*(1.0-p);let uv=std::array::from_fn(|j|uv[j]+(center[j]-uv[j])*p*strength);let c=mix(sample(a,uv,edge)?,sample(b,uv,edge)?,t);for j in 0..4{acc[j]+=c[j]*w;}total+=w;}acc.map(|v|v/total)},
 };
 let alpha=c[3].clamp(0.0,1.0);for j in 0..3{let v=if alpha>0.0{c[j]/alpha}else{0.0};if !v.is_finite()||v.abs()>f32::MAX as f64{return Err("transition RGB overflow".into());}out.d[i+j]=v as f32;}out.d[i+3]=alpha as f32;
 }}Ok(out)
}
