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

/// A built cascade set: the fits plus one shadow map per cascade. `blend_band` is the
/// fraction of each cascade's depth range (measured from its far end) over which the
/// receiver blends into the next cascade - C0 continuity across the boundary, so no
/// seam popping. 0 = hard selection (documented: a visible seam is then possible).
#[derive(Clone,Debug)]
pub struct CascadedShadowMaps{
 pub cascades:Vec<Cascade>,
 pub maps:Vec<super::shadow::DirectionalShadowMap>,
 pub blend_band:f64,
}

/// Allocate one map per cascade from existing fits.
pub fn build_cascade_maps(cascades:Vec<Cascade>,map_size:usize,blend_band:f64)->Result<CascadedShadowMaps,String>{
 if cascades.is_empty(){return Err("at least one cascade required".into());}
 if map_size==0||map_size>4096{return Err("cascade map 1..4096 per axis".into());}
 if !blend_band.is_finite()||!(0.0..0.5).contains(&blend_band){return Err("blend band in [0,0.5) required".into());}
 let maps=cascades.iter().map(|c|super::shadow::DirectionalShadowMap{view:c.view,bounds:c.bounds,width:map_size,height:map_size,depth:vec![f64::INFINITY;map_size*map_size]}).collect();
 Ok(CascadedShadowMaps{cascades,maps,blend_band})
}

/// Rasterize casters into EVERY cascade map (nearest wins per map; a caster outside a
/// cascade's x/y fit simply covers nothing there).
pub fn raster_cascades(positions:&[[f64;3]],indices:&[[u32;3]],csm:&mut CascadedShadowMaps)->Result<(),String>{
 for m in csm.maps.iter_mut(){super::shadow::raster_depth(positions,indices,m)?;}
 Ok(())
}

/// Cascade for a receiver at camera-space depth `d`, plus the blend factor toward the
/// next cascade (0 outside the blend band). Depths at/below the first cascade's near
/// clamp to cascade 0; at/above the last far clamp to the last cascade with factor 0.
pub fn select_cascade(cascades:&[Cascade],blend_band:f64,d:f64)->Result<(usize,f64),String>{
 if cascades.is_empty(){return Err("at least one cascade required".into());}
 if !blend_band.is_finite()||!(0.0..0.5).contains(&blend_band){return Err("blend band in [0,0.5) required".into());}
 if !d.is_finite()||d<=0.{return Err("positive finite camera depth required".into());}
 let n=cascades.len();
 for i in 0..n{
  let c=&cascades[i];
  if d>c.far&&i+1<n{continue;}
  let i=i.min(n-1);let c=&cascades[i];
  let start=c.far-blend_band*(c.far-c.near);
  let width=c.far-start;
  let t=if i+1<n&&width>0.{((d-start)/width).clamp(0.,1.)}else{0.};
  return Ok((i,t));
 }
 Ok((n-1,0.))
}

/// CSM visibility: the receiver's cascade by camera depth, disk-PCF sampled, blended
/// with the next cascade across the band. Bias and normal offset come from
/// shadow::bias PER CASCADE MAP, so they scale with that cascade's texel size
/// (no acne near, no peter-panning far - pinned in tests). n_dot_l<=0 -> 0.
pub fn csm_visibility(csm:&CascadedShadowMaps,cam:&Camera,world:[f64;3],normal:[f64;3],n_dot_l:f64,radius_texels:f64,px:usize,py:usize)->f64{
 if n_dot_l<=0.{return 0.;}
 let Ok(q)=cam.view(world)else{return 1.}; // degenerate receiver: lit (matches shadow.rs pinning)
 let Ok((i,t))=select_cascade(&csm.cascades,csm.blend_band,q[2])else{return 1.;};
 let v0=super::shadow::visibility_disk(&csm.maps[i],world,normal,n_dot_l,radius_texels,px,py);
 if t<=0.{return v0;}
 let v1=super::shadow::visibility_disk(&csm.maps[i+1],world,normal,n_dot_l,radius_texels,px,py);
 v0*(1.-t)+v1*t
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
 #[test]fn select_cascade_hard_and_blend_band(){
  let c=cam();let dir=[0.3,-0.5,1.];
  let fits=cascaded_light_space(dir,&c,16./9.,1.,9.,2,0.,aabb(),256).unwrap(); // uniform split: [1,5],[5,9]
  // band 0: hard selection
  assert_eq!(select_cascade(&fits,0.,1.0).unwrap(),(0,0.));
  assert_eq!(select_cascade(&fits,0.,4.999).unwrap(),(0,0.));
  assert_eq!(select_cascade(&fits,0.,5.0).unwrap(),(0,0.),"the lower cascade owns the split boundary (hard cut, band=0)");
  assert_eq!(select_cascade(&fits,0.,1000.).unwrap(),(1,0.),"past the last far clamps to the last cascade");
  assert!(select_cascade(&fits,0.,0.).is_err(),"non-positive depth typed");
  assert!(select_cascade(&fits,0.5,3.).is_err(),"band 0.5 typed out");
  // band 0.25: blends run only on non-last cascades (the last has no next map to blend toward)
  let fits3=cascaded_light_space(dir,&c,16./9.,1.,9.,3,0.,aabb(),256).unwrap();
  let c1=&fits3[1];let start1=c1.far-0.25*(c1.far-c1.near);let mid1=start1+0.5*(c1.far-start1);
  assert_eq!(select_cascade(&fits3,0.25,start1-1e-3).unwrap(),(1,0.),"before the band: no blend");
  let(i,t)=select_cascade(&fits3,0.25,mid1).unwrap();assert_eq!(i,1);assert!((t-0.5).abs()<1e-12,"mid-band t=0.5: {t}");
  let(i,t)=select_cascade(&fits3,0.25,c1.far).unwrap();assert_eq!(i,1);assert_eq!(t,1.0,"at far the next cascade fully owns it");
  let c0=&fits3[0];let start0=c0.far-0.25*(c0.far-c0.near);let mid0=start0+0.5*(c0.far-start0);
  let(i,t)=select_cascade(&fits3,0.25,mid0).unwrap();assert_eq!(i,0);assert!((t-0.5).abs()<1e-12);
  let(i,t)=select_cascade(&fits3,0.25,8.5).unwrap();assert_eq!(i,2);assert_eq!(t,0.,"the last cascade has no next map: never blends");
 }
 #[test]fn bias_scales_with_per_cascade_texel(){
  let c=cam();let dir=[0.3,-0.5,1.];
  let fits=cascaded_light_space(dir,&c,16./9.,0.5,64.,3,0.7,aabb(),256).unwrap();
  let set=build_cascade_maps(fits,256,0.25).unwrap();
  assert!(set.maps[0].bounds[1][0]-set.maps[0].bounds[0][0]<set.maps[2].bounds[1][0]-set.maps[2].bounds[0][0],"later cascades cover more");
  let(b0,_)=crate::threed::shadow::bias(&set.maps[0],0.7);let(b2,_)=crate::threed::shadow::bias(&set.maps[2],0.7);
  assert!(b2>b0*2.,"normal offset scales with the cascade texel: near {b0} far {b2}");
  assert!(set.blend_band==0.25);
  assert!(build_cascade_maps(vec![],256,0.25).is_err(),"empty typed");
  assert!(select_cascade(&set.cascades,0.6,3.).is_err(),"band typed");}
 fn csm_scene()->(Vec<[f64;3]>,Vec<[u32;3]>){
  // ground quad y=0 spanning x,z in [-40,40]; occluder quad at y=2 over x,z in [-1,1]
  let mut p=vec![[-40.,0.,-40.],[40.,0.,-40.],[40.,0.,40.],[-40.,0.,40.]];
  let mut i=vec![[0u32,1,2],[0,2,3]];
  let b=p.len() as u32;p.extend([[-1.,2.,-1.],[1.,2.,-1.],[1.,2.,1.],[-1.,2.,1.]]);i.extend([[b,b+1,b+2],[b,b+2,b+3]]);
  (p,i)}
 fn csm_set(cam:&Camera,near:f64,far:f64,n:usize)->CascadedShadowMaps{
  let fits=cascaded_light_space([0.2,-1.,0.15],cam,16./9.,near,far,n,0.5,[[-40.,-1.,-40.],[40.,3.,40.]],256).unwrap();
  let mut set=build_cascade_maps(fits,256,0.25).unwrap();
  let(p,i)=csm_scene();raster_cascades(&p,&i,&mut set).unwrap();set}
 #[test]fn csm_visibility_blends_c0_at_the_split(){
  // camera high above the ground looking down: ground receivers sweep cascade depths
  let c=Camera{eye:[0.,10.,-12.],pan:0.,tilt:40.,roll:0.,sensor_height:24.,focal_length:35.,near:0.1};
  let set=csm_set(&c,1.,24.,3);
  let split=set.cascades[0].far;
  // place a receiver straight ahead of the camera at exactly the split depth
  let w=world_from_camera(&c,[0.,0.,split]).expect("invertible pose");
  let d=c.view(w).expect("in front")[2];
  assert!((d-split).abs()<1e-9,"constructed receiver sits at the split: {d}");
  let n=[0.,1.,0.];let nl={let ld=crate::threed::vertex_stream::unit([0.2,-1.,0.15].map(|v|-v)).unwrap();(0..3).map(|j|n[j]*ld[j]).sum::<f64>().max(0.)};
  let(i,t)=select_cascade(&set.cascades,set.blend_band,split).unwrap();
  assert_eq!(t,1.0,"at the split the blend fully selects the next cascade");
  let blended=csm_visibility(&set,&c,w,n,nl,4.0,7,11);
  let next_only=crate::threed::shadow::visibility_disk(&set.maps[i+1],w,n,nl,4.0,7,11);
  assert!((blended-next_only).abs()<1e-9,"C0 at the split: blend matches the next cascade: {blended} vs {next_only}");
 }
 #[test]fn csm_no_acne_and_far_umbra_holds(){
  let c=Camera{eye:[0.,10.,-12.],pan:0.,tilt:40.,roll:0.,sensor_height:24.,focal_length:35.,near:0.1};
  let set=csm_set(&c,1.,24.,3);
  let n=[0.,1.,0.];let nl={let ld=crate::threed::vertex_stream::unit([0.2,-1.,0.15].map(|v|-v)).unwrap();(0..3).map(|j|n[j]*ld[j]).sum::<f64>().max(0.)};
  // no acne: open ground receivers are fully lit under every cascade (own depth is ground depth)
  for z in [-30.,-10.,0.,10.,30.]{let v=csm_visibility(&set,&c,[15.,0.,z],n,nl,4.0,3,5);assert_eq!(v,1.0,"open ground lit at z={z}: {v}");}
  // umbra: the occluder (x,z in [-1,1] at y=2) throws its footprint 2*(0.2,0.15)=(0.4,0.3)
  // off axis along the light tilt, so receivers with (x+0.4, z+0.3) inside [-1,1] are shadowed
  let mut shadowed=0;for z in [-1.,0.,0.5]{let v=csm_visibility(&set,&c,[0.,0.,z],n,nl,4.0,3,5);if v<0.5{shadowed+=1;}}
  assert!(shadowed>=2,"receivers inside the footprint shadowed: {shadowed}");
  // directly under the occluder center must be fully shadowed regardless of band
  let v=csm_visibility(&set,&c,[-0.2,0.,-0.15],n,nl,4.0,3,5);
  assert!(v<0.2,"umbra core dark: {v}");}
 #[test]fn refit_under_sub_texel_motion_is_identical_or_one_texel(){
  let c=Camera{eye:[0.,10.,-12.],pan:0.,tilt:40.,roll:0.,sensor_height:24.,focal_length:35.,near:0.1};
  let a=csm_set(&c,1.,24.,3);
  let r=[a.cascades[0].view[0][0],a.cascades[0].view[0][1],a.cascades[0].view[0][2]];
  let mut c2=c.clone();for j in 0..3{c2.eye[j]+=0.37*a.cascades[0].texel_world*r[j];}
  let b=csm_set(&c2,1.,24.,3);
  for k in 0..3{
   if a.cascades[k].bounds==b.cascades[k].bounds{
    assert_eq!(a.maps[k].depth,b.maps[k].depth,"cascade {k}: identical bounds -> bit-identical map");
   }else{
    // one whole texel shift in x: the map content shifts with it (no fractional resample)
    let dx=((b.cascades[k].bounds[0][0]-a.cascades[k].bounds[0][0])/a.cascades[k].texel_world).round();
    assert!((dx.abs()-1.).abs()<1e-9,"cascade {k}: exactly one texel of shift: {dx}");
    assert_eq!(a.cascades[k].bounds[0][1],b.cascades[k].bounds[0][1],"cascade {k}: y stable under x motion");}}
 }
 #[test]#[ignore="1080p CSM shadow frames for visual review"]fn dump_csm_frames(){
  use std::io::Write;let dir=std::env::var("KIMO_CSM_FRAMES").expect("KIMO_CSM_FRAMES directory");std::fs::create_dir_all(&dir).unwrap();
  let ground=crate::threed::primitives::generate(&crate::threed::primitives::Primitive::Cube{size:[80.,0.1,80.],subdivisions:1}).unwrap();
  let moved=|g:&crate::threed::primitives::Geometry,d:[f64;3]|{let mut v=g.vertices.clone();for q in &mut v{for j in 0..3{q.position[j]+=d[j] as f32;}}crate::threed::vertex_stream::Stream{vertices:v,indices:g.indices.clone()}};
  let b1=crate::threed::primitives::generate(&crate::threed::primitives::Primitive::Cube{size:[2.,2.,2.],subdivisions:1}).unwrap();
  let b2=crate::threed::primitives::generate(&crate::threed::primitives::Primitive::Cube{size:[2.,4.,2.],subdivisions:1}).unwrap();
  let streams=[moved(&ground,[0.,-0.06,10.]),moved(&b1,[-5.,1.,2.]),moved(&b2,[2.,2.,14.]),moved(&b1,[-2.,1.,30.]),moved(&b2,[6.,2.,44.])];
  let mats=[crate::threed::pbr::Material{albedo:[0.75,0.72,0.68],metallic:0.,roughness:0.8},crate::threed::pbr::Material{albedo:[0.7,0.3,0.2],metallic:0.,roughness:0.5},crate::threed::pbr::Material{albedo:[0.2,0.4,0.75],metallic:0.1,roughness:0.35},crate::threed::pbr::Material{albedo:[0.3,0.65,0.3],metallic:0.,roughness:0.6},crate::threed::pbr::Material{albedo:[0.65,0.55,0.25],metallic:0.3,roughness:0.4}];
  let draws:Vec<(&crate::threed::vertex_stream::Stream,&crate::threed::pbr::Material)>=streams.iter().zip(mats.iter()).map(|(s,m)|(s,m)).collect();
  let cam=Camera{eye:[0.,9.,-14.],pan:0.,tilt:26.,roll:0.,sensor_height:24.,focal_length:35.,near:0.1};
  let lights=[crate::threed::lighting_stage::Light::Directional{travel_direction:[0.35,-1.,0.3],radiance:[3.2,3.0,2.7]}];
  let fits=cascaded_light_space([0.35,-1.,0.3],&cam,16./9.,0.5,80.,4,0.6,[[-40.,-1.,-30.],[40.,6.,50.]],512).unwrap();
  eprintln!("CSM splits/texels:");for (i,c) in fits.iter().enumerate(){eprintln!("  cascade {i}: [{:.2},{:.2}] texel {:.4}",c.near,c.far,c.texel_world);}
  let mut set=build_cascade_maps(fits,512,0.25).unwrap();
  let(pos,idx): (Vec<[f64;3]>,Vec<[u32;3]>)={let mut p=vec![];let mut ii=vec![];for s in &streams{let base=p.len() as u32;for v in &s.vertices{p.push(v.position.map(|x|x as f64));}for t in &s.indices{ii.push([t[0]+base,t[1]+base,t[2]+base]);}}(p,ii)};
  raster_cascades(&pos,&idx,&mut set).unwrap();
  let dump=|name:&str,o:&crate::threed::lit_raster::Output|{let path=format!("{dir}/{name}.png");let mut c=std::process::Command::new("ffmpeg").args(["-v","error","-y","-f","rawvideo","-pix_fmt","rgba","-s","1920x1080","-i","-","-frames:v","1",&path]).stdin(std::process::Stdio::piped()).spawn().unwrap();c.stdin.take().unwrap().write_all(&crate::render::to_rgba8(&o.frame)).unwrap();assert!(c.wait().unwrap().success());eprintln!("wrote {path}");};
  dump("csm-1080p-shadowed",&crate::threed::lit_raster::render_shadowed(&draws,&cam,1920,1080,&lights,&[Some(crate::threed::shadow::ShadowMapRef::Cascaded(&set))]).unwrap());
  dump("csm-1080p-unshadowed",&crate::threed::lit_raster::render_multi(&draws,&cam,1920,1080,&lights).unwrap());
  // camera-motion stability pair: sub-texel shift along the light r axis
  let r=[set.cascades[0].view[0][0],set.cascades[0].view[0][1],set.cascades[0].view[0][2]];
  let mut cam2=cam.clone();for j in 0..3{cam2.eye[j]+=0.4*set.cascades[0].texel_world*r[j];}
  let fits2=cascaded_light_space([0.35,-1.,0.3],&cam2,16./9.,0.5,80.,4,0.6,[[-40.,-1.,-30.],[40.,6.,50.]],512).unwrap();
  let mut set2=build_cascade_maps(fits2,512,0.25).unwrap();raster_cascades(&pos,&idx,&mut set2).unwrap();
  for k in 0..4{let same=set.cascades[k].bounds==set2.cascades[k].bounds;eprintln!("  cascade {k} refit under 0.4-texel move: {}",if same{"identical"}else{"shifted whole texels"});}
  dump("csm-1080p-shadowed-moved",&crate::threed::lit_raster::render_shadowed(&draws,&cam2,1920,1080,&lights,&[Some(crate::threed::shadow::ShadowMapRef::Cascaded(&set2))]).unwrap());
 }
 #[test]#[ignore="diagnostic probe"]fn dump_csm_probe(){
  let ground=crate::threed::primitives::generate(&crate::threed::primitives::Primitive::Cube{size:[80.,0.1,80.],subdivisions:1}).unwrap();
  let moved=|g:&crate::threed::primitives::Geometry,d:[f64;3]|{let mut v=g.vertices.clone();for q in &mut v{for j in 0..3{q.position[j]+=d[j] as f32;}}crate::threed::vertex_stream::Stream{vertices:v,indices:g.indices.clone()}};
  let b2=crate::threed::primitives::generate(&crate::threed::primitives::Primitive::Cube{size:[2.,4.,2.],subdivisions:1}).unwrap();
  let streams=[moved(&ground,[0.,-0.06,10.]),moved(&b2,[2.,2.,14.])];
  let cam=Camera{eye:[0.,9.,-14.],pan:0.,tilt:26.,roll:0.,sensor_height:24.,focal_length:35.,near:0.1};
  let fits=cascaded_light_space([0.35,-1.,0.3],&cam,16./9.,0.5,80.,4,0.6,[[-40.,-1.,-30.],[40.,6.,50.]],512).unwrap();
  let mut set=build_cascade_maps(fits,512,0.25).unwrap();
  let(pos,idx):(Vec<[f64;3]>,Vec<[u32;3]>)={let mut p=vec![];let mut ii=vec![];for s in &streams{let base=p.len() as u32;for v in &s.vertices{p.push(v.position.map(|x|x as f64));}for t in &s.indices{ii.push([t[0]+base,t[1]+base,t[2]+base]);}}(p,ii)};
  raster_cascades(&pos,&idx,&mut set).unwrap();
  let n=[0.,1.,0.];let ld=crate::threed::vertex_stream::unit([0.35,-1.,0.3].map(|v|-v)).unwrap();let nl=(0..3).map(|j|n[j]*ld[j]).sum::<f64>().max(0.);
  for x in [3.05,3.2,3.5,4.,4.5,5.,5.4,5.6,6.,7.,8.]{
   let w=[x,0.,14.];let d=cam.view(w).unwrap()[2];
   let(i,tt)=select_cascade(&set.cascades,set.blend_band,d).unwrap();
   let v=csm_visibility(&set,&cam,w,n,nl,2.0,0,0);
   eprintln!("x={x:5.2} depth={d:6.2} cascade={i} t={tt:5.2} v={v:6.3}");
  }
 }
 #[test]fn cascaded_set_covers_range_and_grows(){
  let c=cam();let dir=[0.3,-0.5,1.];
  let set=cascaded_light_space(dir,&c,16./9.,0.1,100.,4,0.5,aabb(),512).unwrap();
  assert_eq!(set.len(),4);
  assert!((set[0].near-0.1).abs()<1e-15&&(set[3].far-100.).abs()<1e-9,"endpoints");
  for w in set.windows(2){assert!((w[0].far-w[1].near).abs()<1e-15,"adjacent cascades share the split");}
  for (i,cs) in set.iter().enumerate(){assert!(cs.bounds[1][0]>cs.bounds[0][0]&&cs.bounds[1][2]>cs.bounds[0][2],"cascade {i} non-empty");}
 }
}
