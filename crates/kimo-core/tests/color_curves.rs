use kimo_core::color::curves::Curve;
#[test]fn nonuniform_identity_extended(){let c=Curve::new(&[[0.,0.],[0.01,0.01],[0.6,0.6],[1.,1.]]).unwrap();for i in -100..1000{let x=i as f32/200.;assert!((c.at(x)-x).abs()<1e-6)}}
#[test]fn no_overshoot_monotone(){let c=Curve::new(&[[0.,0.],[0.1,0.5],[0.9,0.6],[1.,1.]]).unwrap();let mut prev=0.;for i in 0..10001{let v=c.at(i as f32/10000.);assert!(v>=prev-1e-7&&v<=1.00001);prev=v}}
#[test]fn hostile_points(){for p in [vec![],vec![[0.,0.]],vec![[0.,0.],[0.,1.]],vec![[1.,0.],[0.,1.]],vec![[0.,f32::NAN],[1.,1.]],vec![[0.,0.];33]]{assert!(Curve::new(&p).is_err())}}
#[test]fn nonfinite_lookup_no_panic(){let c=Curve::new(&[[0.,0.],[1.,1.]]).unwrap();for v in [f32::NAN,f32::INFINITY,f32::NEG_INFINITY]{assert!(c.at(v).is_nan());}}
#[test]fn effect_shape_and_alpha_preflight(){use kimo_core::{effects::{Params,Frame},color::curves::apply};for f in [Frame{w:2,h:1,d:vec![0.;4]},Frame{w:1,h:1,d:vec![0.1,0.2,0.3,1.01]}]{let mut f=f;assert!(apply(&Params::new(),&mut f,0.).is_err());}}
