//! Cascaded shadow maps for directional lights (CSM-1): practical split scheme, tight
//! per-cascade frustum-slice fitting, and texel snapping against shimmer. Shares the
//! light-frame basis convention with shadow::directional_light_space (r = f x seed,
//! u = r x f, z along the travel direction) so cascade maps and whole-scene maps use
//! the same world -> light transform family and the same bias contract (shadow::bias).
//!
//! Fit contract: cascade i covers the camera frustum slice [c_i, c_{i+1}]. The slice's
//! bounding SPHERE (translation-covariant, rotation-invariant) sets the map extent:
//! texel size = sphere diameter / map_size, and the sphere center is snapped to that
//! texel grid in light space. Snapping to ABSOLUTE grid multiples means sub-texel
//! camera motion either leaves the snapped bounds identical or shifts them by a whole
//! texel - never a fractional resample - which is the anti-shimmer property. Depth
//! (z) bounds cover the scene AABB in light space (casters outside the frustum slice
//! still cast into it), with the same pad convention as directional_light_space.
use super::mesh::Matrix;
use super::vertex_stream::unit;
use crate::camera_projection::Camera;

pub const MAX_CASCADES:usize=8;

fn sub(a:[f64;3],b:[f64;3])->[f64;3]{[a[0]-b[0],a[1]-b[1],a[2]-b[2]]}
fn cross(a:[f64;3],b:[f64;3])->[f64;3]{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
fn mv(m:&Matrix,v:[f64;4])->[f64;4]{std::array::from_fn(|i|m[i][0]*v[0]+m[i][1]*v[1]+m[i][2]*v[2]+m[i][3]*v[3])}

/// Practical split scheme (log/uniform blend): split i in 0..=n at
/// c_i = lambda * near*(far/near)^(i/n) + (1-lambda) * (near + (far-near)*i/n).
/// c_0 = near, c_n = far. Reference: MSDN "Cascaded Shadow Maps" common practices
/// (algorithm only). lambda = 1 is fully logarithmic, 0 fully uniform.
pub fn practical_split(near:f64,far:f64,cascades:usize,lambda:f64)->Result<Vec<f64>,String>{
 if !near.is_finite()||!far.is_finite()||near<=0.||far<=near{return Err("finite 0 < near < far required".into());}
 if cascades==0||cascades>MAX_CASCADES{return Err("cascade count 1..=8 required".into());}
 if !lambda.is_finite()||!(0.0..=1.0).contains(&lambda){return Err("split lambda in [0,1] required".into());}
 let n=cascades as f64;
 let out=(0..=cascades).map(|i|{let t=i as f64/n;
  let log=near*(far/near).powf(t);
  let uni=near+(far-near)*t;
  lambda*log+(1.-lambda)*uni}).collect::<Vec<_>>();
 if out.iter().any(|v|!v.is_finite()){return Err("split distances overflow".into());}
 Ok(out)
}

/// World position from camera-space coordinates; the exact inverse of Camera::view
/// (forward R_y(pan) R_x(tilt) R_z(roll), then translate by the eye).
pub fn world_from_camera(cam:&Camera,q:[f64;3])->Result<[f64;3],String>{
 cam.validate()?;if q.iter().any(|v|!v.is_finite()){return Err("finite camera-space coordinates required".into());}
 let mut p=q;
 let(s,c)=cam.roll.to_radians().sin_cos();p=[c*p[0]-s*p[1],s*p[0]+c*p[1],p[2]];
 let(s,c)=cam.tilt.to_radians().sin_cos();p=[p[0],c*p[1]-s*p[2],s*p[1]+c*p[2]];
 let(s,c)=cam.pan.to_radians().sin_cos();p=[c*p[0]+s*p[2],p[1],-s*p[0]+c*p[2]];
 let w=[p[0]+cam.eye[0],p[1]+cam.eye[1],p[2]+cam.eye[2]];
 if w.iter().any(|v|!v.is_finite()){return Err("camera inverse transform overflow".into());}
 Ok(w)
}

/// The 8 world-space corners of the camera frustum slice [d0, d1] (camera-space depths,
/// d0 < d1). aspect = viewport width/height; the horizontal half-extent follows
/// project()'s square-pixel convention (f = height * focal/sensor for both axes).
pub fn frustum_slice_corners(cam:&Camera,aspect:f64,d0:f64,d1:f64)->Result<[[f64;3];8],String>{
 cam.validate()?;
 if !aspect.is_finite()||aspect<=0.{return Err("positive finite aspect required".into());}
 if !d0.is_finite()||!d1.is_finite()||d0<=0.||d1<=d0{return Err("finite 0 < d0 < d1 required".into());}
 let hy=cam.sensor_height/(2.*cam.focal_length); // tan of the vertical half-angle
 let hx=aspect*hy;
 let mut out=[[0.;3];8];
 for k in 0..8{
  let d=if k&4==0{d0}else{d1};
  let q=[if k&1==0{-hx*d}else{hx*d},if k&2==0{-hy*d}else{hy*d},d];
  out[k]=world_from_camera(cam,q)?;
 }
 Ok(out)
}

/// One fitted cascade: the slice it covers, the world -> light view, the snapped ortho
/// bounds in light space, and the world size of one map texel (diameter/map_size).
#[derive(Clone,Debug)]
pub struct Cascade{
 pub near:f64,pub far:f64,
 pub view:Matrix,
 pub bounds:[[f64;3];2],
 pub texel_world:f64,
 pub slice_center:[f64;3],
 pub slice_radius:f64,
}

/// World -> light frame basis shared with shadow::directional_light_space:
/// r = unit(f x seed), u = r x f, z along the travel direction, origin at the world
/// origin (the bounds carry the offset; a fixed origin keeps the frame
/// camera-independent so the texel grid is stable under camera motion).
pub fn light_frame(travel_direction:[f64;3])->Result<Matrix,String>{
 let f=unit(travel_direction)?;
 let seed=if f[1].abs()<0.99{[0.,1.,0.]}else{[1.,0.,0.]};
 let r=unit(cross(f,seed))?;let u=cross(r,f);
 Ok([[r[0],r[1],r[2],0.],[u[0],u[1],u[2],0.],[f[0],f[1],f[2],0.],[0.,0.,0.,1.]])
}

/// Exact bounding sphere of the 8 slice corners: center = corner average (the frustum
/// slice is symmetric about its center), radius = max corner distance.
pub fn slice_sphere(corners:&[[f64;3];8])->([f64;3],f64){
 let c=std::array::from_fn(|j|corners.iter().map(|p|p[j]).sum::<f64>()/8.);
 let r=corners.iter().map(|p|{let d=sub(*p,c);(d[0]*d[0]+d[1]*d[1]+d[2]*d[2]).sqrt()}).fold(0f64,f64::max);
 (c,r)
}

/// Fit one cascade over the slice [near, far]: extent from the slice sphere, center
/// snapped OUTWARD-safe to the texel grid in light space (round to nearest multiple;
/// the radius then grows by at most texel/2 per axis and coverage is re-widened by
/// rounding the half-extent up to a whole number of texels).
pub fn fit_cascade(travel_direction:[f64;3],cam:&Camera,aspect:f64,near:f64,far:f64,scene_aabb:[[f64;3];2],map_size:usize)->Result<Cascade,String>{
 if scene_aabb.iter().flatten().any(|v|!v.is_finite()){return Err("finite scene bounds required".into());}
 if map_size==0||map_size>4096{return Err("cascade map 1..4096 per axis".into());}
 let corners=frustum_slice_corners(cam,aspect,near,far)?;
 let(center,radius)=slice_sphere(&corners);
 if !radius.is_finite()||radius<=0.{return Err("degenerate frustum slice".into());}
 let view=light_frame(travel_direction)?;
 let texel=(2.*radius)/map_size as f64;
 if !texel.is_finite()||texel<=0.{return Err("cascade texel size underflow".into());}
 // snap the slice center to the absolute texel grid in light space
 let cl=mv(&view,[center[0],center[1],center[2],1.]);
 let snapped=[(cl[0]/texel).round()*texel,(cl[1]/texel).round()*texel];
 // half-extent in whole texels, covering center offset + radius on both sides
 let half=(((radius+(cl[0]-snapped[0]).abs().max((cl[1]-snapped[1]).abs()))/texel).ceil()*texel).max(texel);
 let bounds=[[snapped[0]-half,snapped[1]-half,0.],[snapped[0]+half,snapped[1]+half,0.]];
 // depth from the scene AABB (casters outside the slice cast into it)
 let(mut lo,mut hi)=(f64::INFINITY,f64::NEG_INFINITY);
 for k in 0..8{let p=[if k&1==0{scene_aabb[0][0]}else{scene_aabb[1][0]},if k&2==0{scene_aabb[0][1]}else{scene_aabb[1][1]},if k&4==0{scene_aabb[0][2]}else{scene_aabb[1][2]}];
  let q=mv(&view,[p[0],p[1],p[2],1.]);lo=lo.min(q[2]);hi=hi.max(q[2]);}
 let rad=(0..3).map(|j|(scene_aabb[1][j]-scene_aabb[0][j])/2.).fold(0.,|s,d|s+d*d).sqrt();
 let pad=1e-3*(1.+rad);
 Ok(Cascade{near,far,view,bounds:[[bounds[0][0],bounds[0][1],lo-pad],[bounds[1][0],bounds[1][1],hi+pad]],texel_world:texel,slice_center:center,slice_radius:radius})
}

/// Full cascade set: practical split over [near, far], one fit per cascade.
pub fn cascaded_light_space(travel_direction:[f64;3],cam:&Camera,aspect:f64,near:f64,far:f64,cascades:usize,lambda:f64,scene_aabb:[[f64;3];2],map_size:usize)->Result<Vec<Cascade>,String>{
 let splits=practical_split(near,far,cascades,lambda)?;
 (0..cascades).map(|i|fit_cascade(travel_direction,cam,aspect,splits[i],splits[i+1],scene_aabb,map_size)).collect()
}

#[cfg(test)]mod tests{use super::*;
 fn cam()->Camera{Camera{eye:[2.,1.5,-3.],pan:324.79,tilt:18.,roll:0.,sensor_height:24.,focal_length:35.,near:0.1}}
 fn aabb()->[[f64;3];2]{[[-4.;3],[4.;3]]}
 // Oracle: independent Python transcription of the blend formula (math.pow semantics).
 const SPLIT_CASES:&[(f64,f64,usize,f64,&[f64])]=&[
  (0.1,100.,4,0.5,&[0.1,12.818670662595176,26.60613883008419,46.403897050194615,100.]),
  (0.5,50.,3,0.75,&[0.5,5.990595812604791,16.45413008761956,50.]),
  (1.,16.,2,0.,&[1.,8.5,16.]),
  (1.,16.,2,1.,&[1.,4.,16.]),
 ];
 #[test]fn practical_split_matches_oracle_and_endpoints(){
  for (near,far,n,lambda,want) in SPLIT_CASES{
   let got=practical_split(*near,*far,*n,*lambda).unwrap();
   assert_eq!(got.len(),n+1);
   for (g,w) in got.iter().zip(want.iter()){assert!((g-w).abs()<1e-12,"near={near} far={far} n={n} l={lambda}: got {got:?} want {want:?}");}
   assert!((got[0]-near).abs()<1e-15&&(got[*n]-far).abs()<1e-12,"endpoints pinned");
   assert!(got.windows(2).all(|w|w[1]>w[0]),"strictly increasing");}
 }
 #[test]fn practical_split_typed_errors(){
  assert_eq!(practical_split(0.,1.,4,0.5).err().unwrap(),"finite 0 < near < far required");
  assert_eq!(practical_split(2.,1.,4,0.5).err().unwrap(),"finite 0 < near < far required");
  assert_eq!(practical_split(f64::NAN,10.,4,0.5).err().unwrap(),"finite 0 < near < far required");
  assert_eq!(practical_split(0.1,10.,0,0.5).err().unwrap(),"cascade count 1..=8 required");
  assert_eq!(practical_split(0.1,10.,9,0.5).err().unwrap(),"cascade count 1..=8 required");
  assert_eq!(practical_split(0.1,10.,4,1.5).err().unwrap(),"split lambda in [0,1] required");
  assert_eq!(practical_split(0.1,10.,4,f64::NAN).err().unwrap(),"split lambda in [0,1] required");}
 #[test]fn world_from_camera_inverts_view(){
  let c=cam();
  for q in [[0.3,-0.2,1.5],[-1.,2.,7.],[0.,0.,0.1]]{
   let w=world_from_camera(&c,q).unwrap();
   let back=c.view(w).unwrap();
   for j in 0..3{assert!((back[j]-q[j]).abs()<1e-12,"roundtrip {q:?} -> {w:?} -> {back:?}");}}
 }
 #[test]fn slice_corners_straddle_and_scale(){
  let c=cam();
  let co=frustum_slice_corners(&c,16./9.,1.,2.).unwrap();
  for (k,p) in co.iter().enumerate(){
   let q=c.view(*p).unwrap();
   assert!((q[2]-if k&4==0{1.}else{2.}).abs()<1e-12,"corner {k} at its slice depth: {q:?}");}
  assert!(frustum_slice_corners(&c,16./9.,2.,1.).is_err(),"d1<=d0 typed");
  assert!(frustum_slice_corners(&c,0.,1.,2.).is_err(),"aspect typed");}
 #[test]fn light_frame_matches_directional_light_space_basis(){
  // same direction -> same rotation rows as shadow::directional_light_space (fixed origin here)
  let dir=[0.3,-0.5,1.];
  let(v,_)=crate::threed::shadow::directional_light_space(dir,aabb()).unwrap();
  let f=light_frame(dir).unwrap();
  for i in 0..3{for j in 0..3{assert!((v[i][j]-f[i][j]).abs()<1e-15,"basis row {i} col {j}");}}
 }
 #[test]fn fit_covers_slice_and_snaps_to_grid(){
  let c=cam();let dir=[0.3,-0.5,1.];
  let cs=fit_cascade(dir,&c,16./9.,0.5,8.,aabb(),512).unwrap();
  let corners=frustum_slice_corners(&c,16./9.,0.5,8.).unwrap();
  for p in corners{let q=mv(&cs.view,[p[0],p[1],p[2],1.]);
   assert!(q[0]>=cs.bounds[0][0]-1e-9&&q[0]<=cs.bounds[1][0]+1e-9,"x covered: {q:?} vs {:?}",cs.bounds);
   assert!(q[1]>=cs.bounds[0][1]-1e-9&&q[1]<=cs.bounds[1][1]+1e-9,"y covered: {q:?} vs {:?}",cs.bounds);}
  // snapped edges are absolute multiples of the texel size; extent is a whole texel count
  for j in 0..2{
   assert!((cs.bounds[0][j]/cs.texel_world).fract().abs()<1e-9,"lo edge {j} on the grid");
   assert!((cs.bounds[1][j]/cs.texel_world).fract().abs()<1e-9,"hi edge {j} on the grid");}
  let nx=(cs.bounds[1][0]-cs.bounds[0][0])/cs.texel_world;
  assert!((nx.round()-nx).abs()<1e-9&&nx>=512.,"extent a whole texel count >= map: {nx}");
  // depth covers every scene corner
  for k in 0..8{let p=[if k&1==0{-4.}else{4.},if k&2==0{-4.}else{4.},if k&4==0{-4.}else{4.}];
   let q=mv(&cs.view,[p[0],p[1],p[2],1.]);
   assert!(q[2]>=cs.bounds[0][2]&&q[2]<=cs.bounds[1][2],"scene corner {k} inside depth");}
 }
 #[test]fn anti_shimmer_integer_texel_motion_shifts_whole_texels(){
  let c=cam();let dir=[0.3,-0.5,1.];
  let a=fit_cascade(dir,&c,16./9.,0.5,8.,aabb(),512).unwrap();
  // move the eye exactly 3 texels along the light frame's r axis (world-computable)
  let r=[a.view[0][0],a.view[0][1],a.view[0][2]];
  let mut c2=cam();for j in 0..3{c2.eye[j]+=3.*a.texel_world*r[j];}
  let b=fit_cascade(dir,&c2,16./9.,0.5,8.,aabb(),512).unwrap();
  // the sphere radius accumulates 1-ULP summation differences under translation; the
  // anti-shimmer contract is the grid snap below, not bitwise radius equality
  assert!((a.texel_world-b.texel_world).abs()<1e-12*a.texel_world,"texel size is translation-invariant: {} vs {}",a.texel_world,b.texel_world);
  for j in 0..2{
   let shift=(b.bounds[0][j]-a.bounds[0][j])/a.texel_world;
   assert!((shift.round()-shift).abs()<1e-9,"axis {j} shifted a whole texel count: {shift}");}
 }
 #[test]fn sub_texel_motion_never_fractionally_resamples(){
  let c=cam();let dir=[0.3,-0.5,1.];
  let a=fit_cascade(dir,&c,16./9.,0.5,8.,aabb(),512).unwrap();
  let r=[a.view[0][0],a.view[0][1],a.view[0][2]];
  for frac in [0.1,0.37,0.5,0.9]{
   let mut c2=cam();for j in 0..3{c2.eye[j]+=frac*a.texel_world*r[j];}
   let b=fit_cascade(dir,&c2,16./9.,0.5,8.,aabb(),512).unwrap();
   let shift=(b.bounds[0][0]-a.bounds[0][0])/a.texel_world;
   assert!((shift.round()-shift).abs()<1e-9,"frac {frac}: whole-texel shift or none: {shift}");
   assert!(shift.abs()<=1.,"frac {frac}: at most one texel: {shift}");}
 }
 #[test]fn bounds_stable_under_camera_roll(){
  // the slice-sphere fit is invariant to rotation about the view axis: rolling the
  // camera rotates the 8 corners around that axis, leaving their average (center) and
  // max distance (radius) unchanged, so texel size, snapped center, and bounds are
  // identical - no edge swimming under camera rotation.
  let dir=[0.3,-0.5,1.];
  let a=fit_cascade(dir,&cam(),16./9.,0.5,8.,aabb(),512).unwrap();
  let mut c2=cam();c2.roll=37.;
  let b=fit_cascade(dir,&c2,16./9.,0.5,8.,aabb(),512).unwrap();
  for j in 0..3{assert!((a.slice_center[j]-b.slice_center[j]).abs()<1e-9,"center stable under roll: {:?} vs {:?}",a.slice_center,b.slice_center);}
  assert!((a.slice_radius-b.slice_radius).abs()<1e-9,"radius stable under roll");
  assert_eq!(a.bounds,b.bounds,"snapped bounds identical under roll");
  assert_eq!(a.texel_world,b.texel_world,"texel size identical under roll");}
 #[test]fn cascaded_set_covers_range_and_grows(){
  let c=cam();let dir=[0.3,-0.5,1.];
  let set=cascaded_light_space(dir,&c,16./9.,0.1,100.,4,0.5,aabb(),512).unwrap();
  assert_eq!(set.len(),4);
  assert!((set[0].near-0.1).abs()<1e-15&&(set[3].far-100.).abs()<1e-9,"endpoints");
  for w in set.windows(2){assert!((w[0].far-w[1].near).abs()<1e-15,"adjacent cascades share the split");}
  for (i,cs) in set.iter().enumerate(){assert!(cs.bounds[1][0]>cs.bounds[0][0]&&cs.bounds[1][2]>cs.bounds[0][2],"cascade {i} non-empty");}
 }
}
