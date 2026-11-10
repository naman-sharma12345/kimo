use serde::{Serialize,Deserialize};
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
pub enum Field {Sphere{center:[f64;3],radius:f64},Box{center:[f64;3],half_size:[f64;3]},Union{a:Box<Field>,b:Box<Field>},Intersect{a:Box<Field>,b:Box<Field>},Subtract{a:Box<Field>,b:Box<Field>},SmoothUnion{a:Box<Field>,b:Box<Field>,k:f64}}
fn bounded(v:f64)->bool{v.is_finite()&&v.abs()<=1e9}
impl Field {
 pub fn sample(&self,p:[f64;3])->Result<f64,String>{if p.iter().any(|v|!bounded(*v)){return Err("SDF finite point abs<=1e9 required".into());}self.eval(p,0)}
 fn eval(&self,p:[f64;3],depth:usize)->Result<f64,String>{if depth>32{return Err("SDF depth max32".into());}match self{
 Self::Sphere{center,radius}=>{if center.iter().any(|v|!bounded(*v))||!bounded(*radius)||*radius<=0.0{return Err("valid sphere center/radius required".into());}Ok((p[0]-center[0]).hypot(p[1]-center[1]).hypot(p[2]-center[2])-radius)},
 Self::Box{center,half_size}=>{if center.iter().any(|v|!bounded(*v))||half_size.iter().any(|v|!bounded(*v)||*v<=0.0){return Err("valid box center/positive half sizes required".into());}let q:[f64;3]=std::array::from_fn(|i|(p[i]-center[i]).abs()-half_size[i]);Ok(q[0].max(0.0).hypot(q[1].max(0.0)).hypot(q[2].max(0.0))+q[0].max(q[1]).max(q[2]).min(0.0))},
 Self::Union{a,b}=>Ok(a.eval(p,depth+1)?.min(b.eval(p,depth+1)?)),Self::Intersect{a,b}=>Ok(a.eval(p,depth+1)?.max(b.eval(p,depth+1)?)),Self::Subtract{a,b}=>Ok(a.eval(p,depth+1)?.max(-b.eval(p,depth+1)?)),
 Self::SmoothUnion{a,b,k}=>{if !bounded(*k)||*k<=0.0{return Err("smooth union k>0 finite required".into());}let (a,b)=(a.eval(p,depth+1)?,b.eval(p,depth+1)?);let h=(0.5+0.5*(b-a)/k).clamp(0.0,1.0);Ok(b*(1.0-h)+a*h-k*h*(1.0-h))}
 }}
}
