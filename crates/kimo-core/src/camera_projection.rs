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

#[cfg(test)]
mod tests{
 use super::*;
 // Reference vectors below are an independent Python (stdlib math) transcription of the
 // documented kimo camera convention: inverse of R_y(pan) R_x(tilt) R_z(roll) applied to
 // world-eye, pinhole pixel projection f = height*focal/sensor_height, z>=near clip with
 // lerped attributes. Generated constants pinned at 1e-12 (rel, floor 1 abs).
 pub const VIEW_POSES:[([f64;3],f64,f64,f64);6]=[
 ([0.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00],0.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00),
 ([1.00000000000000000e+00,2.00000000000000000e+00,3.00000000000000000e+00],3.00000000000000000e+01,-4.50000000000000000e+01,1.50000000000000000e+01),
 ([2.39999999999999991e+00,1.89999999999999991e+00,-3.39999999999999991e+00],3.24790000000000020e+02,2.45500000000000007e+01,0.00000000000000000e+00),
 ([-5.00000000000000000e+00,5.00000000000000000e-01,1.00000000000000000e+01],7.20000000000000000e+02,8.99000000000000057e+01,1.80000000000000000e+02),
 ([0.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00],-9.00000000000000000e+01,-3.00000000000000000e+01,-2.70000000000000000e+02),
 ([1.00000000000000000e+02,-5.00000000000000000e+01,2.50000000000000000e-01],1.79990000000000009e+02,1.00000000000000002e-02,3.59500000000000000e+02),
 ];
 pub const VIEW_POINTS:[[f64;3];4]=[
 [0.00000000000000000e+00,0.00000000000000000e+00,5.00000000000000000e+00],
 [1.00000000000000000e+00,-2.00000000000000000e+00,3.00000000000000000e+00],
 [1.00000000000000002e-03,1.00000000000000000e+03,-7.50000000000000000e+00],
 [-4.20000000000000018e+00,6.99999999999999956e-01,1.11099999999999994e+01],
 ];
 pub const VIEW_EXPECTED:[[[f64;3];4];6]=[
 [[0.00000000000000000e+00,0.00000000000000000e+00,5.00000000000000000e+00],[1.00000000000000000e+00,-2.00000000000000000e+00,3.00000000000000000e+00],[1.00000000000000002e-03,1.00000000000000000e+03,-7.50000000000000000e+00],[-4.20000000000000018e+00,6.99999999999999956e-01,1.11099999999999994e+01]],
 [[-2.39394848097298585e+00,-1.72456884158601409e+00,-5.43022081574779492e-01],[-7.32050807568877304e-01,-2.73205080756887764e+00,-2.82842712474618985e+00],[1.88637705448679441e+02,6.87063777755349975e+02,6.98909457212165762e+02],[-9.31418098640593861e+00,-1.69414065200387309e+00,2.20862400686535842e+00]],
 [[2.88232310833135585e+00,1.69826452982593490e+00,8.29084501639695226e+00],[2.54621753564500519e+00,-1.03943587053241648e+00,7.11100199214037598e+00],[-4.32404642337568035e+00,9.07053668530885147e+02,-4.16486638715087395e+02],[2.97360993289330011e+00,5.41530407977248007e+00,1.47435486125490787e+01]],
 [[-4.99999999999999822e+00,5.00086504874938864e+00,4.91272596627152558e-01],[-5.99999999999999734e+00,7.00435265930776119e+00,2.48777889372193117e+00],[-5.00099999999999323e+00,1.57555176442672273e+01,-9.99529020921234292e+02],[-8.00000000000000266e-01,-1.11034737504692815e+00,-1.98062380896510432e-01]],
 [[-1.07156594925393393e-15,-5.00000000000000000e+00,2.65143809681226741e-16],[-1.23205080756887808e+00,-2.99999999999999956e+00,-1.86602540378443837e+00],[8.66025903784438697e+02,7.49999999999984102e+00,4.99999133974596134e+02],[-1.49378221735089500e+00,-1.11099999999999994e+01,3.98730669589464348e+00]],
 [[9.95590422764457657e+01,5.08699096477991830e+01,-4.77617979368791001e+00],[9.85768794454987898e+01,4.88616114259140204e+01,-2.77565625584111109e+00],[9.08336694892129941e+01,1.05083400937197143e+03,7.54928707575990465e+00],[1.03751716666090147e+02,5.16054586423931667e+01,-1.08870348188846418e+01]],
 ];
 pub const PROJECT_CASES:[(usize,usize,u32,u32);6]=[
 (1, 0, 640, 360),
 (2, 2, 1920, 1080),
 (3, 3, 128, 72),
 (0, 1, 100, 50),
 (5, 0, 640, 360),
 (4, 3, 333, 777),
 ];
 pub const PROJECT_EXPECTED:[Option<[f64;3]>;6]=[
 None,
 None,
 None,
 Some([7.43055555555555429e+01,-2.36111111111111001e+01,3.00000000000000000e+00]),
 None,
 Some([-2.58007594757756635e+02,-2.76877374645189275e+03,3.98730669589464348e+00]),
 ];
 pub const FOV_CASES:[(f64,f64,f64);5]=[
 (2.40000000000000000e+01,3.50000000000000000e+01,3.78492888321024665e+01),
 (3.60000000000000000e+01,5.00000000000000000e+01,3.95977527090498640e+01),
 (2.40000000000000000e+01,2.40000000000000000e+01,5.31301023541559800e+01),
 (8.00000000000000000e+00,8.50000000000000000e+01,5.38856857971220116e+00),
 (1.33330000000000002e+01,1.70000000000000000e+01,4.28249652428151109e+01),
 ];
 // clip straddle: one vertex (idx0 z=1.0) inside near=0.1, two outside -> 3-gon -> 1 triangle
 pub const CLIP_STRADDLE_TRI:[([f64;3],[f64;2]);3]=[([0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000000e+00],[1.00000000000000000e+01,1.00000000000000000e+02]),([0.00000000000000000e+00,0.00000000000000000e+00,-1.00000000000000000e+00],[2.00000000000000000e+01,2.00000000000000000e+02]),([0.00000000000000000e+00,0.00000000000000000e+00,5.00000000000000028e-02],[3.00000000000000000e+01,3.00000000000000000e+02])];
 pub const CLIP_STRADDLE_EXPECTED:[([f64;3],[f64;2]);3]=[([0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000000e+00],[1.00000000000000000e+01,1.00000000000000000e+02]),([0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000006e-01],[1.45000000000000000e+01,1.45000000000000000e+02]),([0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000006e-01],[2.89473684210526301e+01,2.89473684210526301e+02])];
 fn close(a:f64,b:f64)->bool{(a-b).abs()<=1e-12*b.abs().max(1.0)}
 fn close3(a:[f64;3],b:[f64;3])->bool{(0..3).all(|i|close(a[i],b[i]))}
 fn cam(pose:([f64;3],f64,f64,f64))->Camera{Camera{eye:pose.0,pan:pose.1,tilt:pose.2,roll:pose.3,sensor_height:24.,focal_length:35.,near:0.1}}
 fn v(p:[f64;3],a:&[f64])->Vertex{Vertex{position:p,attributes:a.to_vec()}}
 fn tri_area(a:[f64;3],b:[f64;3],c:[f64;3])->f64{let e=[b[0]-a[0],b[1]-a[1],b[2]-a[2]];let f=[c[0]-a[0],c[1]-a[1],c[2]-a[2]];let x=[e[1]*f[2]-e[2]*f[1],e[2]*f[0]-e[0]*f[2],e[0]*f[1]-e[1]*f[0]];0.5*(x[0]*x[0]+x[1]*x[1]+x[2]*x[2]).sqrt()}

 #[test]fn view_matches_independent_python_oracle(){for (pi,pose) in VIEW_POSES.iter().enumerate(){let c=cam(*pose);for (qi,p) in VIEW_POINTS.iter().enumerate(){let got=c.view(*p).unwrap();assert!(close3(got,VIEW_EXPECTED[pi][qi]),"pose {pi} point {qi}: got {got:?} want {:?}",VIEW_EXPECTED[pi][qi]);}}}
 #[test]fn view_is_rigid_distance_preserving_and_eye_maps_to_origin(){let d=|a:[f64;3],b:[f64;3]|((a[0]-b[0]).powi(2)+(a[1]-b[1]).powi(2)+(a[2]-b[2]).powi(2)).sqrt();for pose in VIEW_POSES{let c=cam(pose);assert_eq!(c.view(pose.0).unwrap(),[0.,0.,0.],"eye maps to origin");for a in VIEW_POINTS{for b in VIEW_POINTS{let wa=c.view(a).unwrap();let wb=c.view(b).unwrap();assert!((d(wa,wb)-d(a,b)).abs()<1e-9*d(a,b).max(1.),"rigid transform preserves pairwise distances");}}}}
 #[test]fn project_matches_oracle_and_near_boundary(){for (ci,(pi,qi,w,h)) in PROJECT_CASES.iter().enumerate(){let c=cam(VIEW_POSES[*pi]);let got=c.project(VIEW_POINTS[*qi],*w,*h).unwrap();match(got,PROJECT_EXPECTED[ci]){(None,None)=>{},(Some(a),Some(b))=>assert!(close3(a,b),"case {ci}: got {a:?} want {b:?}"),_=>panic!("case {ci}: got {got:?} want {:?}",PROJECT_EXPECTED[ci])}}
  let c=cam(VIEW_POSES[0]);assert!(c.project([0.,0.,0.1],640,360).unwrap().is_some(),"z == near projects");assert_eq!(c.project([0.,0.,0.099],640,360).unwrap(),None,"z < near is clipped");assert_eq!(c.project([0.,0.,5.],0,360).unwrap_err(),"nonempty viewport required");assert_eq!(c.project([0.,0.,5.],640,0).unwrap_err(),"nonempty viewport required");}
 #[test]fn vertical_fov_pins(){for (s,f,want) in FOV_CASES{let c=Camera{eye:[0.;3],pan:0.,tilt:0.,roll:0.,sensor_height:s,focal_length:f,near:0.1};let got=c.vertical_fov().unwrap();assert!(close(got,want),"sensor {s} focal {f}: got {got} want {want}");}}
 #[test]fn validate_and_view_typed_errors(){let base=||Camera{eye:[0.;3],pan:0.,tilt:0.,roll:0.,sensor_height:24.,focal_length:35.,near:0.1};
  let bad=|m:Camera|m.validate().unwrap_err();
  assert_eq!(bad(Camera{eye:[f64::NAN,0.,0.],..base()}),"finite camera values, positive sensor_height/focal_length/near required");
  assert_eq!(bad(Camera{sensor_height:0.,..base()}),"finite camera values, positive sensor_height/focal_length/near required");
  assert_eq!(bad(Camera{focal_length:-1.,..base()}),"finite camera values, positive sensor_height/focal_length/near required");
  assert_eq!(bad(Camera{near:0.,..base()}),"finite camera values, positive sensor_height/focal_length/near required");
  assert_eq!(bad(Camera{focal_length:1e-308,sensor_height:1e308,..base()}),"camera focal/sensor ratio overflow/underflow");
  assert_eq!(base().view([f64::INFINITY,0.,0.]).unwrap_err(),"finite world coordinates required");}
 #[test]fn clip_fully_inside_outside_and_typed_errors(){let t=[v([0.,0.,1.],&[1.]),v([1.,0.,2.],&[2.]),v([0.,1.,3.],&[3.])];
  let inside=clip(t.clone(),0.1).unwrap();assert_eq!(inside.len(),1);assert_eq!(inside[0],t,"fully-inside triangle returned unchanged");
  let out=[v([0.,0.,-1.],&[1.]),v([1.,0.,0.05],&[2.]),v([0.,1.,0.09],&[3.])];assert!(clip(out,0.1).unwrap().is_empty(),"fully-outside triangle clips to nothing");
  assert_eq!(clip(t.clone(),0.0).unwrap_err(),"near must be positive finite");
  assert_eq!(clip(t.clone(),f64::NAN).unwrap_err(),"near must be positive finite");
  let mismatched=[v([0.,0.,1.],&[1.,2.]),v([1.,0.,2.],&[2.]),v([0.,1.,3.],&[3.])];assert_eq!(clip(mismatched,0.1).unwrap_err(),"finite vertices and matching <=32 attribute channels required");
  let wide=[v([0.,0.,1.],&[0.;33]),v([1.,0.,2.],&[0.;33]),v([0.,1.,3.],&[0.;33])];assert_eq!(clip(wide,0.1).unwrap_err(),"finite vertices and matching <=32 attribute channels required");
  let nan=[v([0.,0.,f64::NAN],&[1.]),v([1.,0.,2.],&[2.]),v([0.,1.,3.],&[3.])];assert_eq!(clip(nan,0.1).unwrap_err(),"finite vertices and matching <=32 attribute channels required");}
 #[test]fn clip_straddle_matches_oracle_with_attribute_lerp(){let t:[Vertex;3]=CLIP_STRADDLE_TRI.map(|(p,a)|v(p,&a));let out=clip(t,0.1).unwrap();
  assert_eq!(out.len(),1,"one-inside straddle yields a single triangle");for i in 0..3{let(p,a)=CLIP_STRADDLE_EXPECTED[i];assert!(close3(out[0][i].position,p)&&out[0][i].position[2]>=0.1,"vertex {i} position {:?} want {p:?}",out[0][i].position);assert!(a.iter().zip(&out[0][i].attributes).all(|(w,g)|close(*w,*g)),"vertex {i} attributes lerped");}}
 #[test]fn clip_area_conservation_and_plane_vertex_dedup(){let one=[v([0.,0.,1.],&[]),v([2.,0.,-1.],&[]),v([0.,2.,-1.],&[])];let out=clip(one,0.5).unwrap();
  let area:f64=out.iter().map(|t|tri_area(t[0].position,t[1].position,t[2].position)).sum();assert!(close(area,0.21650635094610965),"one-inside clipped area {area}");
  let two=[v([0.,0.,1.],&[]),v([2.,0.,-1.],&[]),v([0.,2.,2.],&[])];let out=clip(two,0.5).unwrap();assert_eq!(out.len(),2,"two-inside straddle yields a 4-gon fan of 2 triangles");
  let area:f64=out.iter().map(|t|tri_area(t[0].position,t[1].position,t[2].position)).sum();assert!(close(area,1.875),"two-inside clipped area {area}");assert!(out.iter().flatten().all(|x|x.position[2]>=0.5),"every emitted vertex satisfies z>=near");
  let on_plane=[v([0.,0.,0.5],&[7.]),v([2.,0.,-1.],&[8.]),v([0.,2.,2.],&[9.])];let out=clip(on_plane,0.5).unwrap();assert_eq!(out.len(),1,"on-plane endpoint intersection dedups to one triangle");
  for t in &out{for i in 0..3{assert_ne!(t[i].position,t[(i+1)%3].position,"no duplicate adjacent polygon vertices");}}}
}
