//! Direct Rust API bypasses JSON sanitization, so validate each declared control.
use kimo_core::{color,effects::{Frame,Params},keyframe::Prop};
#[test]fn every_declared_managed_control_rejects_nonfinite_before_write(){
    type Effect=fn(&Params,&mut Frame,f64)->Result<(),String>;
    let cases:[(Effect,Vec<color::Entry>);8]=[
        (color::managed::apply,color::managed::catalog()),
        (color::linear::apply,color::linear::catalog()),
        (color::adaptation::apply,color::adaptation::catalog()),
        (color::gamut_ray::apply,color::gamut_ray::catalog()),
        (color::shoulder::apply,color::shoulder::catalog()),
        (color::curves::apply,color::curves::catalog()),
        (color::cdl::apply,color::cdl::catalog()),
        (color::matching::apply,color::matching::catalog()),
    ];
    let mut checked=0;
    for(apply,catalog)in cases{for(_,_,controls)in catalog{for(key,_,_)in controls{for value in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY]{
        let mut params=Params::new();params.insert((*key).into(),Prop::new(value));
        let mut f=Frame{w:2,h:1,d:vec![0.1,0.2,0.3,1.,0.3,0.2,0.1,0.5]};let before=f.d.clone();
        assert!(apply(&params,&mut f,0.).is_err(),"accepted {key}={value}");assert_eq!(f.d,before,"mutated {key}={value}");checked+=1;
    }}}}
    assert!(checked>=138,"matrix unexpectedly shrank: {checked}");
}
