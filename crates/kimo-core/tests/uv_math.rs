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
