use kimo_core::uv_math::{push,radial};
#[test]fn push_endpoints_identity_and_zero_direction(){for uv in [[0.,0.],[0.5,0.5],[1.,1.]]{let a=push(uv,0.,[1.,0.]).unwrap();let b=push(uv,1.,[1.,0.]).unwrap();assert_eq!(a.from_uv,uv);assert_eq!(a.from_weight,1.);assert_eq!(b.to_uv,uv);assert_eq!(b.to_weight,1.);let z=push(uv,0.5,[0.,0.]).unwrap();assert_eq!(z.from_uv,uv);}}
#[test]fn push_magnitude_not_sign_quantized(){let a=push([0.2,0.3],0.5,[0.4,0.8]).unwrap();assert_eq!(a.from_uv,[0.4,0.7]);assert!((a.to_uv[1]+0.1).abs()<1e-15);assert!(a.from_weight+a.to_weight<=1.);}
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
