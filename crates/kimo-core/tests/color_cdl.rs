use kimo_core::{color::cdl::apply,effects::{Frame,Params},keyframe::Prop};
#[test]fn sop_known_answer(){let mut p=Params::new();for (k,v) in [("slope_r",2.),("offset_r",0.1),("power_r",2.),("saturation",1.)]{p.insert(k.into(),Prop::new(v));}let mut f=Frame{w:1,h:1,d:vec![0.2,0.3,0.4,0.6]};apply(&p,&mut f,0.).unwrap();assert!((f.d[0]-0.25).abs()<1e-6);assert_eq!(f.d[3],0.6)}
#[test]fn hostile_sop(){for(k,v)in [("power_r",0.),("slope_r",-1.),("saturation",-1.),("power_b",f64::NAN)]{let mut p=Params::new();p.insert(k.into(),Prop::new(v));let mut f=Frame{w:1,h:1,d:vec![0.5;4]};assert!(apply(&p,&mut f,0.).is_err())}}

#[test]fn hostile_frame_rejected_before_mutation(){
    let params=Params::new();
    for n in [0,1,3,5,9]{let mut f=Frame{w:1,h:1,d:vec![0.25;n]};let before=f.d.clone();assert!(apply(&params,&mut f,0.).is_err());assert_eq!(f.d,before);}
    for bad in [f32::NAN,f32::INFINITY,f32::NEG_INFINITY]{for channel in 0..4{let mut f=Frame{w:2,h:1,d:vec![0.25,0.5,0.75,1.,0.25,0.5,0.75,1.]};f.d[4+channel]=bad;let before:Vec<_>=f.d.iter().map(|v|v.to_bits()).collect();assert!(apply(&params,&mut f,0.).is_err());assert_eq!(f.d.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),before);}}
    for alpha in [-0.1,1.01]{let mut f=Frame{w:1,h:1,d:vec![0.25,0.5,0.75,alpha]};assert!(apply(&params,&mut f,0.).is_err());}
    let mut f=Frame{w:1,h:1,d:vec![-0.25,4.,0.5,0.25]};apply(&params,&mut f,0.).unwrap();assert_eq!(f.d,vec![0.,1.,0.5,0.25]);
}
