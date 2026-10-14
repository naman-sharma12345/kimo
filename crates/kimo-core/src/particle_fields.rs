//! Stateless fixed-step point/sphere trajectory with raw forces and swept planes.
//! Original bounded primitive, not a particle preset or general rigid-body solver.
use serde::{Deserialize,Serialize};
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]
pub struct State {pub position:[f64;3],pub velocity:[f64;3],pub mass:f64,pub radius:f64}
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
pub enum Force {Acceleration{value:[f64;3]},LinearDrag{coefficient:f64},Attractor{position:[f64;3],strength:f64,softening:f64},VectorField{value:[f64;3]}}
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]
pub struct Plane {pub normal:[f64;3],pub offset:f64,pub restitution:f64}
fn dot(a:[f64;3],b:[f64;3])->f64{a.iter().zip(b).map(|(x,y)|x*y).sum()}
fn finite(v:&[f64])->bool{v.iter().all(|x|x.is_finite()&&x.abs()<=1e12)}
fn validate(s:&State,forces:&[Force],planes:&[Plane],dt:f64,steps:u32)->Result<(),String>{
 if !finite(&s.position)||!finite(&s.velocity)||!s.mass.is_finite()||s.mass<=0.0||!s.radius.is_finite()||s.radius<0.0||!dt.is_finite()||dt<=0.0||dt>1.0||steps>100000||forces.len()>64||planes.len()>16{return Err("invalid state/dt or limits: steps<=100000 forces<=64 planes<=16".into());}
 for f in forces{match f{Force::Acceleration{value}|Force::VectorField{value}=>if !finite(value){return Err("finite force vector required".into());},Force::LinearDrag{coefficient}=>if !coefficient.is_finite()||*coefficient<0.0{return Err("drag nonnegative finite required".into());},Force::Attractor{position,strength,softening}=>if !finite(position)||!strength.is_finite()||!softening.is_finite()||*softening<=0.0{return Err("finite attractor and positive softening required".into());}}}
 for p in planes{let n=dot(p.normal,p.normal);if !finite(&p.normal)||!p.offset.is_finite()||!p.restitution.is_finite()||!(0.0..=1.0).contains(&p.restitution)||(n-1.0).abs()>1e-8{return Err("plane normal must be unit, offset finite, restitution 0..1".into());}}
 Ok(())
}
fn acceleration(s:&State,forces:&[Force])->[f64;3]{let mut a=[0.0;3];for f in forces{match f{
 Force::Acceleration{value}=>for j in 0..3{a[j]+=value[j];},
 Force::VectorField{value}=>for j in 0..3{a[j]+=value[j]/s.mass;},
 Force::LinearDrag{coefficient}=>for j in 0..3{a[j]-=coefficient*s.velocity[j]/s.mass;},
 Force::Attractor{position,strength,softening}=>{let d=[position[0]-s.position[0],position[1]-s.position[1],position[2]-s.position[2]];let den=(dot(d,d)+softening*softening).powf(1.5);for j in 0..3{a[j]+=strength*d[j]/den;}}
 }}a}
/// Outputs initial state plus each exact fixed-step sample. No age-dependent dt.
/// Within each step, up to eight earliest sphere/plane events; explicit error if
/// unresolved rather than silently tunneling. No sphere/sphere contact yet.
pub fn simulate(mut s:State,forces:&[Force],planes:&[Plane],dt:f64,steps:u32)->Result<Vec<State>,String>{
 validate(&s,forces,planes,dt,steps)?;
 let count=(steps as usize).checked_add(1).ok_or("trajectory size overflow")?;
 crate::resource_limits::reserve(count.checked_mul(std::mem::size_of::<State>()).ok_or("trajectory allocation overflow")?)?;
 let mut out=Vec::new();out.try_reserve_exact(count).map_err(|e|e.to_string())?;out.push(s.clone());
 for _ in 0..steps{crate::guard::check();let a=acceleration(&s,forces);for j in 0..3{s.velocity[j]+=a[j]*dt;}
  // Resolve initial penetration separately, then swept motion under held step velocity.
  for p in planes{let d=dot(p.normal,s.position)-p.offset-s.radius;if d<0.0{for j in 0..3{s.position[j]-=p.normal[j]*d;}}}
  let mut remaining=dt;
  for _ in 0..8{if remaining<=dt*1e-12{remaining=0.0;break;}
   let mut best=remaining;let mut hit=None;
   for (i,p) in planes.iter().enumerate(){let speed=dot(p.normal,s.velocity);if speed>=0.0{continue;}let distance=(dot(p.normal,s.position)-p.offset-s.radius).max(0.0);let t=distance/(-speed);if t<=best{best=t;hit=Some(i);}}
   for j in 0..3{s.position[j]+=s.velocity[j]*best;}remaining-=best;
   if let Some(i)=hit{let p=&planes[i];let vn=dot(p.normal,s.velocity);for j in 0..3{s.velocity[j]-=(1.0+p.restitution)*vn*p.normal[j];}}
   else{remaining=0.0;break;}
  }
  if remaining>dt*1e-12{return Err("swept collision event budget exhausted".into());}
  if !finite(&s.position)||!finite(&s.velocity){return Err("particle state overflow or +/-1e12 limit".into());}out.push(s.clone());
 }Ok(out)
}
