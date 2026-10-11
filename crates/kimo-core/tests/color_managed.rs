use kimo_core::color::managed::*;
use kimo_core::{effects::{Frame, Params}, keyframe::Prop};
#[test] fn neutral_adaptation_and_inverse() {
    for a in 0..8 { for b in 0..8 {
        let n=mv(conversion(a,b),[1.;3]);for x in n{assert!((x-1.).abs()<2e-12)}
        for c in [[0.18;3],[0.9,0.2,0.03],[-0.2,4.,0.8]] {
            let d=mv(conversion(b,a),mv(conversion(a,b),c));
            for j in 0..3 {assert!((d[j]-c[j]).abs()<2e-12)}
        }
    }}
}
#[test] fn pq_reference_and_roundtrip() {
    assert!((decode(6,0.508078421517399,100.)-1.).abs()<1e-10);
    assert!((decode(6,1.,100.)-100.).abs()<1e-10);
    for i in 1..1000 {let x=i as f64/1000.;assert!((encode(6,decode(6,x,100.),100.)-x).abs()<1e-12)}
}
#[test] fn hlg_reference_and_roundtrip() {
    assert!((decode(7,0.5,100.)-1./12.).abs()<1e-12);
    for i in 0..1000 {let x=i as f64/1000.;assert!((encode(7,decode(7,x,100.),100.)-x).abs()<3e-8)}
}
#[test] fn negative_scene_values_preserved() {
    for t in [0,1,2,3,4,5] {let x=-0.05;assert!((decode(t,encode(t,x,100.),100.)-x).abs()<2e-8)}
}
#[test] fn gamut_compression_preserves_luma() {
    for c in [[1.2,0.3,-0.1],[-0.3,0.8,1.2]] {let d=gamut_compress(c);assert!(d.iter().all(|v|*v>=0.&&*v<=1.));let l=|v:[f64;3]|0.2126*v[0]+0.7152*v[1]+0.0722*v[2];assert!((l(c)-l(d)).abs()<1e-12)}
}
#[test] fn bounded_hostile_params() {
    for k in ["from_gamut","to_gamut","from_transfer","to_transfer","reference_white","exposure"] {
        for v in [f64::NAN,f64::INFINITY,-1e30,1e30,0.,0.5] {
            let mut pr=Params::new();pr.insert(k.into(),Prop::new(v));
            let mut f=Frame{w:1,h:1,d:vec![0.18,0.3,0.8,0.4]};
            let _=apply(&pr,&mut f,0.);assert!(f.d.iter().all(|v|v.is_finite()));assert_eq!(f.d[3],0.4);
        }
    }
}
#[test]fn camera_log_scene_roundtrip(){for(g,t)in[(5,4),(6,5),(7,5)]{for c in [[0.18;3],[0.02,0.1,0.5],[2.,1.,0.1]]{let mut f=Frame{w:1,h:1,d:vec![c[0],c[1],c[2],1.]};let mut p=Params::new();for(k,v)in[("from_gamut",3.),("to_gamut",g as f64),("from_transfer",0.),("to_transfer",t as f64)]{p.insert(k.into(),Prop::new(v));}apply(&p,&mut f,0.).unwrap();p.insert("from_gamut".into(),Prop::new(g as f64));p.insert("to_gamut".into(),Prop::new(3.));p.insert("from_transfer".into(),Prop::new(t as f64));p.insert("to_transfer".into(),Prop::new(0.));apply(&p,&mut f,0.).unwrap();for j in 0..3{assert!((f.d[j]-c[j]).abs()<3e-6)}}}}
