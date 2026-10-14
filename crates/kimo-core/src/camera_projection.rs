//! Independent explicit-pose pinhole camera and attributed triangle clipping math.
//! Original implementation. Does not reinterpret legacy Camera or saved scenes.
use serde::{Deserialize,Serialize};
#[derive(Clone,Copy,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Camera {pub eye:[f64;3],pub pan:f64,pub tilt:f64,pub roll:f64,pub sensor_height:f64,pub focal_length:f64,pub near:f64}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Vertex {pub position:[f64;3],pub attributes:Vec<f64>}
impl Camera {
 pub fn validate(&self)->Result<(),String>{if self.eye.iter().chain([self.pan,self.tilt,self.roll,self.sensor_height,self.focal_length,self.near].iter()).any(|v|!v.is_finite())||self.sensor_height<=0.0||self.focal_length<=0.0||self.near<=0.0{return Err("finite camera values, positive sensor_height/focal_length/near required".into());}let focal=self.focal_length/self.sensor_height;if !focal.is_finite()||focal==0.0{return Err("camera focal/sensor ratio overflow/underflow".into());}Ok(())}
 pub fn vertical_fov(&self)->Result<f64,String>{self.validate()?;Ok(2.0*(self.sensor_height/(2.0*self.focal_length)).atan().to_degrees())}
 pub fn view(&self,world:[f64;3])->Result<[f64;3],String>{self.validate()?;if world.iter().any(|v|!v.is_finite()){return Err("finite world coordinates required".into());}
    // Inverse of R_y(pan) R_x(tilt) R_z(roll), retaining legacy orientation order.
    let mut q=[world[0]-self.eye[0],world[1]-self.eye[1],world[2]-self.eye[2]];
    let(s,c)=self.pan.to_radians().sin_cos();q=[c*q[0]-s*q[2],q[1],s*q[0]+c*q[2]];
    let(s,c)=self.tilt.to_radians().sin_cos();q=[q[0],c*q[1]+s*q[2],-s*q[1]+c*q[2]];
    let(s,c)=self.roll.to_radians().sin_cos();q=[c*q[0]+s*q[1],-s*q[0]+c*q[1],q[2]];
    if q.iter().any(|v|!v.is_finite()){return Err("camera view transform overflow".into());}Ok(q)
 }
 pub fn project(&self,world:[f64;3],width:u32,height:u32)->Result<Option<[f64;3]>,String>{if width==0||height==0{return Err("nonempty viewport required".into());}let q=self.view(world)?;if q[2]<self.near{return Ok(None);}let f=height as f64*(self.focal_length/self.sensor_height);let o=[width as f64*0.5+f*(q[0]/q[2]),height as f64*0.5+f*(q[1]/q[2]),q[2]];if o.iter().any(|v|!v.is_finite()){return Err("camera projection overflow".into());}Ok(Some(o))}
}
/// Camera-space clipping against z>=near. Affine attributes interpolated before
/// raster perspective correction. Output fan has zero, one or two triangles.
pub fn clip(tri:[Vertex;3],near:f64)->Result<Vec<[Vertex;3]>,String>{
 if !near.is_finite()||near<=0.0{return Err("near must be positive finite".into());}
 let n=tri[0].attributes.len();if n>32||tri.iter().any(|v|v.attributes.len()!=n||v.position.iter().chain(v.attributes.iter()).any(|x|!x.is_finite())){return Err("finite vertices and matching <=32 attribute channels required".into());}
 let mut polygon=Vec::with_capacity(4);
 for i in 0..3{let a=&tri[i];let b=&tri[(i+1)%3];let ai=a.position[2]>=near;let bi=b.position[2]>=near;
  if ai {polygon.push(a.clone());}
  if ai!=bi{let den=b.position[2]-a.position[2];let t=(near-a.position[2])/den;if !t.is_finite(){return Err("clipping intersection overflow".into());}
   let lerp=|a:f64,b:f64|a*(1.0-t)+b*t;
   let v=Vertex{position:[lerp(a.position[0],b.position[0]),lerp(a.position[1],b.position[1]),near],attributes:a.attributes.iter().zip(&b.attributes).map(|(&x,&y)|lerp(x,y)).collect()};
   if v.position.iter().chain(&v.attributes).any(|x|!x.is_finite()){return Err("clipping attribute overflow".into());}polygon.push(v);
  }
 }
 // On-plane endpoints can duplicate intersections; retain polygon order, remove exact adjacent copies.
 polygon.dedup_by(|a,b|a.position==b.position&&a.attributes==b.attributes);
 if polygon.len()>1&&polygon.first()==polygon.last(){polygon.pop();}
 let mut out=Vec::new();for i in 1..polygon.len().saturating_sub(1){out.push([polygon[0].clone(),polygon[i].clone(),polygon[i+1].clone()]);}Ok(out)
}
