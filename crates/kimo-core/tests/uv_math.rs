use kimo_core::uv_math::{push,radial};
#[test]fn push_endpoints_identity_and_zero_direction(){for uv in [[0.,0.],[0.5,0.5],[1.,1.]]{let a=push(uv,0.,[1.,0.]).unwrap();let b=push(uv,1.,[1.,0.]).unwrap();assert_eq!(a.from_uv,uv);assert_eq!(a.from_weight,1.);assert_eq!(b.to_uv,uv);assert_eq!(b.to_weight,1.);let z=push(uv,0.5,[0.,0.]).unwrap();assert_eq!(z.from_uv,uv);}}
#[test]fn push_magnitude_not_sign_quantized(){let a=push([0.2,0.3],0.5,[0.4,0.8]).unwrap();assert_eq!(a.from_uv,[0.4,0.7]);assert_eq!(a.to_uv,a.from_uv);assert_eq!(a.from_weight+a.to_weight,1.);}
#[test]fn radial_endpoints_and_independent_strength(){assert_eq!(radial([0.2,0.3],[0.5,0.5],0.,1.).unwrap(),[0.2,0.3]);assert_eq!(radial([0.2,0.3],[0.5,0.5],1.,1.).unwrap(),[0.5,0.5]);assert!(radial([0.,0.],[1.,1.],2.,1.).is_err());assert!(push([0.,0.],f64::NAN,[1.,0.]).is_err());}

#[test]fn mappings_axes_poles_seams_and_unbounded_planar(){use kimo_core::uv_math::{polar,spherical,cylindrical,planar};
 assert_eq!(polar([1.,0.]).unwrap().uv,[0.5,1.]);assert!(polar([0.,0.]).unwrap().singular);
 let p=spherical([0.,1.,0.]).unwrap();assert_eq!(p.uv,[0.5,0.]);assert!(p.singular);
 assert_eq!(spherical([0.,-1.,0.]).unwrap().uv,[0.5,1.]);assert!(spherical([0.;3]).unwrap().singular);
 assert_eq!(cylindrical([1.,7.,0.]).unwrap().uv,[0.5,7.]);
 assert_eq!(planar([12.,-9.,7.],2,0).unwrap().uv,[7.,12.]);assert!(planar([0.;3],1,1).is_err());
 let a=polar([-1.,1e-9]).unwrap().uv[0];let b=polar([-1.,-1e-9]).unwrap().uv[0];assert!((a-b).abs()>0.99);
}
#[test]fn spherical_scale_invariant_and_finite_large_coordinates(){use kimo_core::uv_math::spherical;
 let a=spherical([1.,2.,3.]).unwrap();let b=spherical([1e100,2e100,3e100]).unwrap();for j in 0..2{assert!((a.uv[j]-b.uv[j]).abs()<1e-14);}
}

#[test]fn mapping_singular_outputs_are_finite_including_signed_zero(){
 use kimo_core::uv_math::{polar,spherical,cylindrical};
 for x in [0.0,-0.0]{for y in [0.0,-0.0]{
  let p=polar([x,y]).unwrap();assert!(p.singular);assert_eq!(p.uv,[0.5,0.0]);
  for z in [0.0,-0.0]{let s=spherical([x,y,z]).unwrap();assert!(s.singular);assert_eq!(s.uv,[0.5,0.5]);}
  for height in [-f64::MAX,-1.0,0.0,1.0,f64::MAX]{
   let c=cylindrical([x,height,y]).unwrap();assert!(c.singular);assert!(c.uv.iter().all(|v|v.is_finite()));assert_eq!(c.uv,[0.5,height]);
  }
 }}
 for y in [-f64::MAX,-1.0,-f64::from_bits(1),f64::from_bits(1),1.0,f64::MAX]{
  let s=spherical([0.0,y,-0.0]).unwrap();assert!(s.singular);assert_eq!(s.uv,[0.5,if y>0.0{0.0}else{1.0}]);
 }
}
#[test]fn angular_wrap_is_canonical_and_seam_sides_are_modulo_close(){
 use kimo_core::uv_math::{polar,spherical,cylindrical};
 for z in [0.0,-0.0]{
  assert_eq!(polar([-1.0,z]).unwrap().uv[0],0.0);
  assert_eq!(spherical([-1.0,0.0,z]).unwrap().uv[0],0.0);
  assert_eq!(cylindrical([-1.0,3.0,z]).unwrap().uv[0],0.0);
 }
 for epsilon in [1e-3,1e-9,1e-15]{
  let a=polar([-1.0,epsilon]).unwrap().uv[0];let b=polar([-1.0,-epsilon]).unwrap().uv[0];
  for u in [a,b]{assert!(u.is_finite()&&(0.0..1.0).contains(&u));}
  let d=(a-b).abs();assert!(d.min(1.0-d)<epsilon);
 }
}
#[test]fn mappings_extreme_finite_inputs_and_nonfinite_rejection(){
 use kimo_core::uv_math::{polar,spherical,cylindrical,planar};
 let tiny=f64::from_bits(1);
 for point in [[tiny,tiny,tiny],[f64::MAX,f64::MAX,f64::MAX],[tiny,0.0,-tiny],[-f64::MAX,tiny,0.0]]{
  let s=spherical(point).unwrap();assert!(s.uv.iter().all(|v|v.is_finite()));assert!((0.0..1.0).contains(&s.uv[0]));assert!((0.0..=1.0).contains(&s.uv[1]));assert!(!s.singular);
 }
 let small=spherical([tiny,tiny,tiny]).unwrap();let unit=spherical([1.0,1.0,1.0]).unwrap();assert_eq!(small,unit);
 assert!(polar([f64::MAX,f64::MAX]).is_err());assert!(cylindrical([f64::MAX,0.0,f64::MAX]).is_err());
 for bad in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY]{
  for axis in 0..3{let mut p=[1.0;3];p[axis]=bad;assert!(spherical(p).is_err());assert!(cylindrical(p).is_err());assert!(planar(p,0,1).is_err());}
  assert!(polar([bad,0.0]).is_err());assert!(polar([0.0,bad]).is_err());
 }
}

#[test]fn push_has_exactly_one_owner_over_time_and_pixel_grid(){
 for d in [[1.0,0.0],[-1.0,0.0],[0.0,1.0],[0.0,-1.0],[1.0,1.0],[-1.0,1.0],[0.4,0.8],[3.0,-2.0],[0.0,0.0]]{
  for step in 0..=100{let t=step as f64/100.0;for y in 0..33{for x in 0..65{
   let uv=[x as f64/64.0,y as f64/32.0];let p=push(uv,t,d).unwrap();
   assert_eq!(p.from_weight+p.to_weight,1.0);assert!(p.from_weight==0.0||p.to_weight==0.0);
   let q=if p.from_weight==1.0{p.from_uv}else{p.to_uv};assert!(q.iter().all(|v|v.is_finite()&&(0.0..=1.0).contains(v)));
   if t==0.0{assert_eq!(p.from_uv,uv);assert_eq!(p.from_weight,1.0);}if t==1.0{assert_eq!(p.to_uv,uv);assert_eq!(p.to_weight,1.0);}
  }}}
 }
}
#[test]fn cardinal_push_abuts_at_boundary_without_gutter(){
 for t in [0.1,0.25,0.5,0.75,0.9]{
  let boundary=1.0-t;let left=push([boundary-1e-10,0.5],t,[1.0,0.0]).unwrap();let right=push([boundary+1e-10,0.5],t,[1.0,0.0]).unwrap();
  assert_eq!(left.from_weight,1.0);assert_eq!(right.to_weight,1.0);assert!((left.from_uv[0]-1.0).abs()<2e-10);assert!(right.to_uv[0].abs()<2e-10);
 }
 let corner=push([0.9,0.1],0.5,[1.0,1.0]).unwrap();assert_eq!(corner.to_weight,1.0);assert!(corner.to_uv.iter().all(|v|(0.0..1.0).contains(v)));
}
#[test]fn azimuth_continuity_away_from_seam_and_circular_continuity_at_seam(){
 use kimo_core::uv_math::{polar,cylindrical,spherical};
 let mut previous=None;
 for i in 0..=10000{let theta=-std::f64::consts::PI+1e-6+(std::f64::consts::TAU-2e-6)*i as f64/10000.0;let (x,z)=(theta.cos(),theta.sin());
  let u=polar([x,z]).unwrap().uv[0];assert!(u.is_finite()&&(0.0..1.0).contains(&u));
  if let Some(p)=previous{assert!(u>=p);assert!(u-p<0.000101);}previous=Some(u);
  assert_eq!(cylindrical([x,0.0,z]).unwrap().uv[0],u);assert_eq!(spherical([x,0.0,z]).unwrap().uv[0],u);
 }
 for epsilon in [f64::from_bits(1),1e-300,1e-16,1e-12,1e-9]{
  let a=polar([-1.0,epsilon]).unwrap().uv[0];let b=polar([-1.0,-epsilon]).unwrap().uv[0];
  assert!((0.0..1.0).contains(&a)&&(0.0..1.0).contains(&b));let d=(a-b).abs();assert!(d.min(1.0-d)<=epsilon+4.0*f64::EPSILON);
 }
}

/// 1080p render harness v2: bilinear grid-texture sampling (repeat wrap), full-coverage
/// push contract. Caller-side harness code only: uv_math supplies coordinates.
/// Not a golden update and never a substitute for visual inspection.
#[test]#[ignore="1080p render gate after compile and functional tests"]
fn render_1080p_uv_sources(){
 use kimo_core::uv_math::{push,radial,polar,cylindrical,spherical,planar};
 use std::{io::Write,path::PathBuf};
 let dir=PathBuf::from(std::env::var("KIMO_UV_FRAMES").expect("set output directory"));std::fs::create_dir_all(&dir).unwrap();
 let (w,h)=(1920usize,1080usize);
 let (tw,th)=(192usize,108usize);
 let grid=|swap:bool|->Vec<[f32;4]>{let mut g=vec![[0.;4];tw*th];
  for ty in 0..th{for tx in 0..tw{let c=if (tx/12+ty/12)%2==0{0.85f32}else{0.15};
   let detail=((tx*7+ty*13)%17)as f32/17.0*0.2;
   g[ty*tw+tx]=if swap{[(ty as f32/th as f32)*0.5,(c*0.9+detail).min(1.0),(1.0-tx as f32/tw as f32)*0.7+0.2,1.0]}
    else{[(c*0.9+detail).min(1.0),(tx as f32/tw as f32),(ty as f32/th as f32)*0.6+0.2,1.0]};}}
  g};
 let ga=grid(false);let gb=grid(true);
 let bili=|g:&[[f32;4]],u:f64,v:f64|->[f32;4]{
  let x=u.rem_euclid(1.0)*tw as f64-0.5;let y=v.rem_euclid(1.0)*th as f64-0.5;
  let ix=x.floor() as i64;let iy=y.floor() as i64;let dx=(x-x.floor())as f32;let dy=(y-y.floor())as f32;
  let mut out=[0f32;4];
  for (oy,wy) in [(0,1.0-dy),(1,dy)]{for (ox,wx) in [(0,1.0-dx),(1,dx)]{
   let px=(ix+ox).rem_euclid(tw as i64)as usize;let py=(iy+oy).rem_euclid(th as i64)as usize;
   let t=g[py*tw+px];for c in 0..4{out[c]+=t[c]*wx*wy;}}}
  out};
 let px=|x:usize,y:usize|[(x as f64+0.5)/w as f64,(y as f64+0.5)/h as f64];
 let write=|name:&str,f:&[[f32;4]]|{let mut file=std::fs::File::create(dir.join(format!("{name}.ppm"))).unwrap();
  write!(file,"P6\n{w} {h}\n255\n").unwrap();let mut rgb=Vec::with_capacity(w*h*3);
  for p in f{for v in p.iter().take(3){rgb.push((v.clamp(0.,1.)*255.+0.5)as u8);}}
  file.write_all(&rgb).unwrap();};
 let mut frames:Vec<(String,Vec<[f32;4]>)>=Vec::new();
 let mut f=vec![[0.;4];w*h];
 for y in 0..h{for x in 0..w{let [u,v]=px(x,y);let p=polar([(u-0.5)*4.0,(v-0.5)*4.0]).unwrap();
  f[y*w+x]=if p.singular{[1.,0.,1.,1.]}else{bili(&ga,p.uv[0],p.uv[1]*0.25)};}}
 frames.push(("polar".into(),f));
 let mut f2=vec![[0.;4];w*h];
 for y in 0..h{for x in 0..w{let [u,v]=px(x,y);let p=cylindrical([(u-0.5)*4.0,(v-0.5)*2.25,1.5]).unwrap();
  f2[y*w+x]=if p.singular{[1.,0.,1.,1.]}else{bili(&ga,p.uv[0],p.uv[1])};}}
 frames.push(("cylindrical".into(),f2));
 let mut f3=vec![[0.;4];w*h];
 for y in 0..h{for x in 0..w{let [u,v]=px(x,y);let p=spherical([(u-0.5)*2.0,(0.5-v)*2.0,0.6]).unwrap();
  f3[y*w+x]=if p.singular{[1.,0.,1.,1.]}else{bili(&ga,p.uv[0],p.uv[1]*2.0)};}}
 frames.push(("spherical".into(),f3));
 let mut f4=vec![[0.;4];w*h];
 for y in 0..h{for x in 0..w{let [u,v]=px(x,y);let p=planar([u*3.0,v*3.0,(u*u+v*v)*0.8],2,0).unwrap();
  f4[y*w+x]=bili(&ga,p.uv[0]*0.5,p.uv[1]*0.5);}}
 frames.push(("planar".into(),f4));
 for (name,t,dir) in [("push-00",0.0,[0.35,0.12]),("push-01",0.25,[0.35,0.12]),("push-02",0.5,[0.35,0.12]),("push-03",0.75,[0.35,0.12]),("push-04",1.0,[0.35,0.12]),("pushd-00",0.0,[0.5,-0.3]),("pushd-01",0.25,[0.5,-0.3]),("pushd-02",0.5,[0.5,-0.3]),("pushd-03",0.75,[0.5,-0.3]),("pushd-04",1.0,[0.5,-0.3])]{
  let mut f5=vec![[0.;4];w*h];
  for y in 0..h{for x in 0..w{let uv=px(x,y);let s=push(uv,t,dir).unwrap();
   let a=bili(&ga,s.from_uv[0],s.from_uv[1]);let b=bili(&gb,s.to_uv[0],s.to_uv[1]);
   f5[y*w+x]=std::array::from_fn(|i|a[i]*s.from_weight as f32+b[i]*s.to_weight as f32);}}
  frames.push((name.into(),f5));}
 let mut f6=vec![[0.;4];w*h];
 for y in 0..h{for x in 0..w{let uv=px(x,y);let mut acc=[0f64;4];
  for k in 0..24{let frac=(k as f64+0.5)/24.0;let q=radial(uv,[0.5,0.5],frac,0.55).unwrap();let c=bili(&ga,q[0],q[1]);for j in 0..4{acc[j]+=c[j]as f64;}}
  f6[y*w+x]=std::array::from_fn(|i|(acc[i]/24.0)as f32);}}
 frames.push(("radial".into(),f6));
 for (name,f) in &frames{write(name,f);}
}

// PROBE NOTES (approved, 1080p bilinear harness renders):
// - radial zoom-blur frame: faint vertical line at the center column is the zoom singularity
//   (pull direction flips; +-8px magnification band, peak column delta 5.08 vs 0.00 median,
//   smooth monotone ramp across - no single-column jump). The operator has no atan2 and no
//   wrap path, so no branch-cut or wrap-filtering fix applies.
// - push: ownership boundary pixel-exact (A through x=1679, B from x=1680; B through y=80,
//   A from y=81 for dir (0.5,-0.3) t=0.25); tiled B strips match across frames at fraction 1.0.
//   The top-right "jump" is the translated frame edge, not an off-by-one clip.
// - spherical: soft pole patch measured 10x smoother than surround (30px block gradient 0.08 vs
//   0.82 quadrant median) with valid content (mean [62,232,138]) - pole undersampling softening,
//   acceptable for the correctness pass.
