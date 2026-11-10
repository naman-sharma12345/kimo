//! Keyframe animation bridge for scene descriptors (scene composition CP4). Original code, zero new deps.
//! Raw composable primitives: per-target keyframe lists - linear interpolation for scalars and
//! vec3s, slerp (shorter arc) for unit quaternion rotations [w,x,y,z]. No easing curves, no
//! presets: easing is upstream composition of sample times. sample() produces a STATIC Scene
//! at time t, so animated frames route through the unchanged CP2/CP3 executors
//! (render_cpu / render_wgpu): the bridge owns interpolation, the executors own pixels.
//! An animated node's world matrix is REPLACED by T(t)*R(t)*S(t) composed from its tracks
//! (defaults: translation 0, rotation identity, scale 1) - node.world is not composed in;
//! author the full pose in tracks when animating a node. Times clamp to the first/last key.
//! Four target branches: node pose, camera pose, directional light params, point light params.
//! Light tracks that do not match the light's variant are typed errors, never silently dropped.
use serde::{Serialize,Deserialize};
use super::scene::Scene;
use super::lighting_stage::Light;
use crate::camera_projection::Camera;

#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]
pub struct Keys<T>{pub times:Vec<f64>,pub values:Vec<T>}

#[derive(Clone,Debug,Default,Serialize,Deserialize)]#[serde(deny_unknown_fields)]
pub struct NodeAnim{pub node:usize,#[serde(default)]pub translation:Option<Keys<[f64;3]>>,#[serde(default)]pub rotation:Option<Keys<[f64;4]>>,#[serde(default)]pub scale:Option<Keys<[f64;3]>>}

#[derive(Clone,Debug,Default,Serialize,Deserialize)]#[serde(deny_unknown_fields)]
pub struct CameraAnim{#[serde(default)]pub eye:Option<Keys<[f64;3]>>,#[serde(default)]pub pan:Option<Keys<f64>>,#[serde(default)]pub tilt:Option<Keys<f64>>,#[serde(default)]pub roll:Option<Keys<f64>>}

#[derive(Clone,Debug,Default,Serialize,Deserialize)]#[serde(deny_unknown_fields)]
pub struct LightAnim{pub light:usize,#[serde(default)]pub travel_direction:Option<Keys<[f64;3]>>,#[serde(default)]pub radiance:Option<Keys<[f64;3]>>,#[serde(default)]pub position:Option<Keys<[f64;3]>>,#[serde(default)]pub intensity:Option<Keys<[f64;3]>>,#[serde(default)]pub min_distance:Option<Keys<f64>>}

#[derive(Clone,Debug,Default,Serialize,Deserialize)]#[serde(deny_unknown_fields)]
pub struct Animation{#[serde(default)]pub nodes:Vec<NodeAnim>,#[serde(default)]pub camera:Option<CameraAnim>,#[serde(default)]pub lights:Vec<LightAnim>}

fn check<T>(k:&Option<Keys<T>>,what:&str,finite:impl Fn(&T)->bool)->Result<(),String>{
 let Some(k)=k else{return Ok(())};
 if k.times.is_empty()||k.times.len()>4096{return Err(format!("{what}: 1..4096 keys required"));}
 if k.times.len()!=k.values.len(){return Err(format!("{what}: times/values length mismatch ({} vs {})",k.times.len(),k.values.len()));}
 if k.times.iter().any(|t|!t.is_finite()){return Err(format!("{what}: finite key times required"));}
 if k.times.windows(2).any(|w|w[1]<=w[0]){return Err(format!("{what}: key times strictly increasing required"));}
 if k.values.iter().any(|v|!finite(v)){return Err(format!("{what}: finite key values required"));}
 Ok(())}
fn f3(v:&[f64;3])->bool{v.iter().all(|x|x.is_finite())}
fn f4(v:&[f64;4])->bool{v.iter().all(|x|x.is_finite())}
fn fs(v:&f64)->bool{v.is_finite()}

/// Interpolation parameter with end clamping: exact on keys, clamped outside the range.
fn span(times:&[f64],t:f64)->(usize,f64){
 if t<=times[0]{return (0,0.);}
 let n=times.len();if t>=times[n-1]{return (n-1,0.);}
 let i=times.partition_point(|x|*x<=t)-1;
 (i,(t-times[i])/(times[i+1]-times[i]))}

pub fn sample_vec3(k:&Keys<[f64;3]>,t:f64)->[f64;3]{let(i,u)=span(&k.times,t);let a=k.values[i];if u==0.{return a;}let b=k.values[i+1];std::array::from_fn(|j|a[j]+(b[j]-a[j])*u)}
pub fn sample_scalar(k:&Keys<f64>,t:f64)->f64{let(i,u)=span(&k.times,t);let a=k.values[i];if u==0.{return a;}a+(k.values[i+1]-a)*u}

fn qnorm(q:[f64;4])->Result<[f64;4],String>{let n=(q.iter().map(|x|x*x).sum::<f64>()).sqrt();if !n.is_finite()||n<1e-12{return Err("rotation key: unit quaternion required (near-zero norm)".into());}Ok(q.map(|x|x/n))}
/// Spherical linear interpolation of unit quaternions [w,x,y,z] along the shorter arc.
pub fn quat_slerp(a:[f64;4],b:[f64;4],u:f64)->[f64;4]{let mut b=b;let mut d: f64=(0..4).map(|i|a[i]*b[i]).sum();if d<0.{d=-d;for x in b.iter_mut(){*x=-*x;}}
 if d>0.9995{let r=std::array::from_fn(|i|a[i]+(b[i]-a[i])*u);let n=(r.iter().map(|x|x*x).sum::<f64>()).sqrt();return r.map(|x|x/n);}
 let th=d.clamp(-1.,1.).acos();let s=th.sin();let(a0,b0)=(((1.-u)*th).sin()/s,(u*th).sin()/s);std::array::from_fn(|i|a0*a[i]+b0*b[i])}
pub fn sample_rotation(k:&Keys<[f64;4]>,t:f64)->Result<[f64;4],String>{let(i,u)=span(&k.times,t);let a=qnorm(k.values[i])?;if u==0.{return Ok(a);}let b=qnorm(k.values[i+1])?;Ok(quat_slerp(a,b,u))}
/// Quaternion [w,x,y,z] to a row-major 3x3 rotation matrix.
pub fn quat_mat3(q:[f64;4])->[[f64;3];3]{let(w,x,y,z)=(q[0],q[1],q[2],q[3]);
 [[1.-2.*(y*y+z*z),2.*(x*y-w*z),2.*(x*z+w*y)],[2.*(x*y+w*z),1.-2.*(x*x+z*z),2.*(y*z-w*x)],[2.*(x*z-w*y),2.*(y*z+w*x),1.-2.*(x*x+y*y)]]}

/// Animated node pose: T(t)*R(t)*S(t), missing tracks default to translation 0 / identity / scale 1.
pub fn node_world(a:&NodeAnim,t:f64)->Result<super::mesh::Matrix,String>{
 let tr=a.translation.as_ref().map(|k|sample_vec3(k,t)).unwrap_or([0.;3]);
 let rot=match &a.rotation{Some(k)=>sample_rotation(k,t)?,None=>[1.,0.,0.,0.]};
 let sc=a.scale.as_ref().map(|k|sample_vec3(k,t)).unwrap_or([1.;3]);
 let r=quat_mat3(rot);
 let mut m=[[0.;4];4];for i in 0..3{for j in 0..3{m[i][j]=r[i][j]*sc[j];}m[i][3]=tr[i];}m[3]=[0.,0.,0.,1.];Ok(m)}

/// Validate an animation against a scene: track shapes, index ranges, duplicates, variant match.
pub fn validate(s:&Scene,a:&Animation)->Result<(),String>{
 if a.nodes.len()>64||a.lights.len()>64{return Err("max64 animated nodes/lights".into());}
 let mut seen=std::collections::BTreeSet::new();
 for n in &a.nodes{if !seen.insert(n.node){return Err(format!("node {} animated twice",n.node));}
  if n.node>=s.nodes.len(){return Err(format!("animates node {} of {}",n.node,s.nodes.len()));}
  check(&n.translation,"node translation",f3)?;check(&n.rotation,"node rotation",f4)?;check(&n.scale,"node scale",f3)?;}
 if let Some(c)=&a.camera{check(&c.eye,"camera eye",f3)?;check(&c.pan,"camera pan",fs)?;check(&c.tilt,"camera tilt",fs)?;check(&c.roll,"camera roll",fs)?;}
 let mut seen=std::collections::BTreeSet::new();
 for l in &a.lights{if !seen.insert(l.light){return Err(format!("light {} animated twice",l.light));}
  let Some(light)=s.lights.get(l.light)else{return Err(format!("animates light {} of {}",l.light,s.lights.len()));};
  check(&l.travel_direction,"light travel_direction",f3)?;check(&l.radiance,"light radiance",f3)?;check(&l.position,"light position",f3)?;check(&l.intensity,"light intensity",f3)?;check(&l.min_distance,"light min_distance",fs)?;
  match light{
   Light::Directional{..}=>{if l.position.is_some()||l.intensity.is_some()||l.min_distance.is_some(){return Err(format!("light {l} is directional: position/intensity/min_distance tracks do not apply",l=l.light));}}
   Light::Point{..}=>{if l.travel_direction.is_some()||l.radiance.is_some(){return Err(format!("light {l} is point: travel_direction/radiance tracks do not apply",l=l.light));}}}}
 Ok(())}

/// Sample the animation at time t into a fresh static Scene (sources/materials shared by clone).
pub fn sample(s:&Scene,a:&Animation,t:f64)->Result<Scene,String>{
 if !t.is_finite(){return Err("finite sample time required".into());}
 validate(s,a)?;
 let mut out=s.clone();
 for n in &a.nodes{out.nodes[n.node].world=node_world(n,t)?;}
 if let Some(c)=&a.camera{let cam:&mut Camera=&mut out.camera;
  if let Some(k)=&c.eye{cam.eye=sample_vec3(k,t);}
  if let Some(k)=&c.pan{cam.pan=sample_scalar(k,t);}
  if let Some(k)=&c.tilt{cam.tilt=sample_scalar(k,t);}
  if let Some(k)=&c.roll{cam.roll=sample_scalar(k,t);}}
 for l in &a.lights{match &mut out.lights[l.light]{
  Light::Directional{travel_direction,radiance}=>{if let Some(k)=&l.travel_direction{*travel_direction=sample_vec3(k,t);}if let Some(k)=&l.radiance{*radiance=sample_vec3(k,t);}}
  Light::Point{position,intensity,min_distance}=>{if let Some(k)=&l.position{*position=sample_vec3(k,t);}if let Some(k)=&l.intensity{*intensity=sample_vec3(k,t);}if let Some(k)=&l.min_distance{*min_distance=sample_scalar(k,t);}}}}
 Ok(out)}

#[cfg(test)]mod tests{use super::*;
 fn keys(t:Vec<f64>,v:Vec<[f64;3]>)->Keys<[f64;3]>{Keys{times:t,values:v}}
 #[test]fn vec3_linear_midpoint_and_end_clamping(){let k=keys(vec![0.,2.],vec![[0.,0.,0.],[4.,-2.,1.]]);
  assert_eq!(sample_vec3(&k,1.),[2.,-1.,0.5],"linear midpoint");
  assert_eq!(sample_vec3(&k,-5.),[0.,0.,0.],"clamp before first key");
  assert_eq!(sample_vec3(&k,9.),[4.,-2.,1.],"clamp after last key");
  let one=keys(vec![3.],vec![[7.,8.,9.]]);assert_eq!(sample_vec3(&one,0.),[7.,8.,9.],"single key is constant");}
 #[test]fn scalar_linear_and_clamp(){let k=Keys{times:vec![0.,4.],values:vec![10.,-10.]};assert_eq!(sample_scalar(&k,3.),-5.);assert_eq!(sample_scalar(&k,-1.),10.);assert_eq!(sample_scalar(&k,5.),-10.);}
 #[test]fn slerp_90deg_z_midpoint_is_45deg_with_unit_norm(){
  let half=(45f64.to_radians()/2.).sin();let q90=[(90f64.to_radians()/2.).cos(),0.,0.,(90f64.to_radians()/2.).sin()];
  let m=sample_rotation(&Keys{times:vec![0.,1.],values:vec![[1.,0.,0.,0.],q90]},0.5).unwrap();
  let n=(m.iter().map(|x|x*x).sum::<f64>()).sqrt();assert!((n-1.).abs()<1e-12,"unit norm {n}");
  assert!((m[0]-(45f64.to_radians()/2.).cos()).abs()<1e-9&&(m[3]-half).abs()<1e-9,"45deg about z: {m:?}");
  // shorter arc: q and -q are the same rotation - slerp to either reaches the same place
  let m2=sample_rotation(&Keys{times:vec![0.,1.],values:vec![[1.,0.,0.,0.],[-q90[0],-q90[1],-q90[2],-q90[3]]]},0.5).unwrap();
  assert!((m2[0]-m[0]).abs()<1e-9&&(m2[3]-m[3]).abs()<1e-9,"shorter arc sign flip");
  assert!(sample_rotation(&Keys{times:vec![0.],values:vec![[0.,0.,0.,0.]]},0.).is_err(),"zero-norm quaternion key is a typed error");}
 #[test]fn malformed_tracks_are_typed_errors(){
  let node=crate::threed::scene::Node{source:0,world:[[1.,0.,0.,0.],[0.,1.,0.,0.],[0.,0.,1.,0.],[0.,0.,0.,1.]],material:0};
  let s=Scene{sources:vec![],nodes:vec![node],materials:vec![],lights:vec![],camera:Camera{eye:[0.;3],pan:0.,tilt:0.,roll:0.,sensor_height:24.,focal_length:35.,near:0.1}};
  let a=Animation{nodes:vec![NodeAnim{node:0,translation:Some(keys(vec![0.,0.],vec![[0.;3],[1.;3]])),..Default::default()}],..Default::default()};
  assert!(validate(&s,&a).unwrap_err().contains("strictly increasing"),"non-increasing times");
  let a=Animation{nodes:vec![NodeAnim{node:0,translation:Some(keys(vec![0.],vec![[0.;3],[1.;3]])),..Default::default()}],..Default::default()};
  assert!(validate(&s,&a).unwrap_err().contains("length mismatch"),"times/values mismatch");
  let a=Animation{nodes:vec![NodeAnim{node:0,..Default::default()},NodeAnim{node:0,..Default::default()}],..Default::default()};
  assert!(validate(&s,&a).unwrap_err().contains("animated twice"),"duplicate node");
  let a=Animation{lights:vec![LightAnim{light:0,..Default::default()}],..Default::default()};
  assert!(validate(&s,&a).unwrap_err().contains("animates light 0 of 0"),"dangling light index");}
 #[test]fn node_world_composes_trs_exactly(){
  let half=90f64.to_radians()/2.;let q90=[half.cos(),0.,0.,half.sin()];
  let a=NodeAnim{node:0,translation:Some(keys(vec![0.],vec![[5.,6.,7.]])),rotation:Some(Keys{times:vec![0.],values:vec![q90]}),scale:Some(keys(vec![0.],vec![[2.,3.,4.]]))};
  let m=node_world(&a,0.).unwrap();
  // Rz(90): x->y, y->-x; columns scaled by [2,3,4]
  let exp=[[0.,-3.,0.,5.],[2.,0.,0.,6.],[0.,0.,4.,7.],[0.,0.,0.,1.]];
  for i in 0..4{for j in 0..4{assert!((m[i][j]-exp[i][j]).abs()<1e-12,"[{i}][{j}] {} vs {}",m[i][j],exp[i][j]);}}
  let bare=NodeAnim{node:0,..Default::default()};assert_eq!(node_world(&bare,0.).unwrap(),[[1.,0.,0.,0.],[0.,1.,0.,0.],[0.,0.,1.,0.],[0.,0.,0.,1.]],"no tracks: identity pose");}
 #[test]fn sample_applies_camera_and_both_light_variants(){
  let mut s=Scene{sources:vec![],nodes:vec![],materials:vec![],lights:vec![Light::Directional{travel_direction:[0.,0.,1.],radiance:[1.;3]},Light::Point{position:[0.;3],intensity:[1.;3],min_distance:0.01}],camera:Camera{eye:[0.;3],pan:0.,tilt:0.,roll:0.,sensor_height:24.,focal_length:35.,near:0.1}};
  let a=Animation{camera:Some(CameraAnim{eye:Some(keys(vec![0.,2.],vec![[0.,0.,0.],[0.,0.,4.]])),pan:Some(Keys{times:vec![0.],values:vec![0.5]}),..Default::default()}),lights:vec![LightAnim{light:0,radiance:Some(keys(vec![0.],vec![[3.,3.,3.]])),..Default::default()},LightAnim{light:1,position:Some(keys(vec![0.],vec![[1.,2.,3.]])),min_distance:Some(Keys{times:vec![0.],values:vec![0.25]}),..Default::default()}],..Default::default()};
  let out=sample(&s,&a,1.).unwrap();
  assert_eq!(out.camera.eye,[0.,0.,2.]);assert_eq!(out.camera.pan,0.5);
  match &out.lights[0]{Light::Directional{radiance,..}=>assert_eq!(*radiance,[3.;3]),_=>panic!()}
  match &out.lights[1]{Light::Point{position,min_distance,..}=>{assert_eq!(*position,[1.,2.,3.]);assert_eq!(*min_distance,0.25);},_=>panic!()}
  // wrong-variant tracks are typed errors, never silently dropped
  let bad=Animation{lights:vec![LightAnim{light:0,position:Some(keys(vec![0.],vec![[0.;3]])),..Default::default()}],..Default::default()};
  assert!(sample(&s,&bad,0.).unwrap_err().contains("directional: position/intensity/min_distance"));
  let bad=Animation{lights:vec![LightAnim{light:1,radiance:Some(keys(vec![0.],vec![[1.;3]])),..Default::default()}],..Default::default()};
  assert!(sample(&s,&bad,0.).unwrap_err().contains("point: travel_direction/radiance"));
  s.nodes.clear();assert!(sample(&s,&Animation::default(),f64::NAN).is_err(),"NaN sample time is a typed error");}
}
