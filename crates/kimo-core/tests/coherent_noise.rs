//! Numeric gates plus actual renderer assertions. Not a premium visual sign-off.
use kimo_core::{commands::{dispatch,registry_ref,Session},effects::{Frame,Params},keyframe::Prop,noise::{sample,Kind,COORD_LIMIT}};
use serde_json::json;
use std::collections::BTreeMap;
fn points()->Vec<[f64;2]> {(0..501).map(|i| [i as f64*0.137-30.0,i as f64*0.293-65.0]).collect()}
#[test] fn deterministic_seeds_threads_order_and_bounds() {
    for k in [Kind::Perlin,Kind::Simplex] {let p=points();let expected:Vec<_>=p.iter().map(|q|sample(k,7,q[0],q[1]).unwrap()).collect();
        let threaded=std::thread::spawn(move||p.iter().rev().map(|q|sample(k,7,q[0],q[1]).unwrap()).collect::<Vec<_>>()).join().unwrap();
        assert_eq!(expected,threaded.into_iter().rev().collect::<Vec<_>>());
        let mut changed=0;
        for (q,v) in points().iter().zip(&expected) {assert!(v.value.abs()<=k.bound()+1e-12);assert!(v.gradient.iter().all(|x|x.is_finite()&&x.abs()<=32.0));if sample(k,8,q[0],q[1]).unwrap()!=*v {changed+=1;}}
        assert!(changed>490);
    }
}
#[test] fn analytic_gradient_matches_independent_central_difference() {
    for k in [Kind::Perlin,Kind::Simplex] {for seed in [0,1,u64::MAX] {for q in points().into_iter().step_by(3) {
        let s=sample(k,seed,q[0],q[1]).unwrap();
        for axis in 0..2 {let mut a=q;let mut b=q;let e=1e-5;a[axis]+=e;b[axis]-=e;
            let fd=(sample(k,seed,a[0],a[1]).unwrap().value-sample(k,seed,b[0],b[1]).unwrap().value)/(2.0*e);
            assert!((fd-s.gradient[axis]).abs()<2e-6,"{k:?} {q:?} axis {axis}: {fd} vs {}",s.gradient[axis]);}
    }}}
}
#[test] fn lattice_and_simplex_diagonal_boundaries_are_continuous() {
    for k in [Kind::Perlin,Kind::Simplex] {for i in -20..20 {for q in [[i as f64,0.371],[0.371,i as f64],[i as f64*0.13,i as f64*0.13]] {
        let e=1e-7;let a=sample(k,11,q[0]-e,q[1]).unwrap();let b=sample(k,11,q[0]+e,q[1]).unwrap();
        assert!((a.value-b.value).abs()<64.0*e);for j in 0..2 {assert!((a.gradient[j]-b.gradient[j]).abs()<2e-4,"{k:?} {q:?}");}
    }}}
}
#[test] fn perlin_lattice_is_exact_zero_and_rejects_bad_coordinates() {
    for i in -8..8 {for j in -8..8 {assert_eq!(sample(Kind::Perlin,0,i as f64,j as f64).unwrap().value,0.0);}}
    for k in [Kind::Perlin,Kind::Simplex] {for x in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY,COORD_LIMIT+1.0] {assert!(sample(k,0,x,0.0).is_err());assert!(sample(k,0,0.0,x).is_err());}}
}
#[test] fn mcp_samples_match_raw_and_tool_is_discoverable() {
    let mut ss=Session::new();let q=[[0.125,-1.75],[1.25,4.5]];
    assert!(registry_ref().iter().any(|c|c.name=="sample_noise"));assert_eq!(kimo_core::toolgroups::group_of("sample_noise"),"effects");
    for (name,k) in [("perlin",Kind::Perlin),("simplex",Kind::Simplex)] {
        let r=dispatch(&mut ss,"sample_noise",&json!({"kind":name,"seed":u64::MAX,"points":q})).unwrap();
        for (i,p) in q.iter().enumerate(){let v=sample(k,u64::MAX,p[0],p[1]).unwrap();assert_eq!(r["samples"][i]["value"].as_f64().unwrap(),v.value);}
    }
    for a in [json!({"kind":"wrong","seed":1,"points":[]}),json!({"kind":"perlin","seed":-1,"points":[]}),json!({"kind":"perlin","seed":1.5,"points":[]}),json!({"kind":"perlin","seed":1,"points":[[0]]})] {assert!(dispatch(&mut ss,"sample_noise",&a).is_err());}
}
#[test] fn effect_preserves_alpha_signed_hdr_and_identity() {
    let mut p=Params::new();p.insert("gain".into(),Prop::new(8.0));p.insert("bias".into(),Prop::new(-1.0));
    let strings=BTreeMap::new();let mut f=Frame::new(32,18);for (i,px) in f.d.chunks_exact_mut(4).enumerate(){px.copy_from_slice(&[2.0,-0.5,4.0,i as f32/576.0]);}
    let before=f.d.clone();p.insert("amount".into(),Prop::new(0.0));kimo_core::effects::apply("coherent_noise",&p,&strings,&mut f,0.0,0).unwrap();assert_eq!(f.d,before);
    p.insert("amount".into(),Prop::new(1.0));kimo_core::effects::apply("coherent_noise",&p,&strings,&mut f,0.0,0).unwrap();
    for (a,b) in f.d.chunks_exact(4).zip(before.chunks_exact(4)){assert_eq!(a[3],b[3]);assert_eq!(a[0],a[1]);}
    assert!(f.d.chunks_exact(4).any(|p|p[0]<0.0));assert!(f.d.chunks_exact(4).any(|p|p[0]>1.0));
}
#[test] fn actual_renderer_agrees_with_sample_at_pixel_centers() {
    for (name,kind,gain) in [("perlin",Kind::Perlin,0.3),("simplex",Kind::Simplex,4.0)] {
        let mut ss=Session::new();dispatch(&mut ss,"project_set",&json!({"width":320,"height":180,"fps":25})).unwrap();
        let tr=dispatch(&mut ss,"add_track",&json!({})).unwrap()["track_id"].clone();
        let clip=dispatch(&mut ss,"add_clip",&json!({"track_id":tr,"source_type":"solid","color":[0,0,0],"start":0,"duration":2})).unwrap()["clip_id"].clone();
        let fx=dispatch(&mut ss,"add_effect",&json!({"clip_id":clip,"kind":"coherent_noise","strings":{"kind":name,"seed":"19"}})).unwrap()["effect_id"].clone();
        for (p,v) in [("frequency",8.0),("gain",gain),("bias",0.5),("rate_x",0.25)] {dispatch(&mut ss,"set_effect_param",&json!({"clip_id":clip,"effect_id":fx,"param":p,"value":v})).unwrap();}
        let frame=kimo_core::render::render_frame(&ss.project,0.5,&kimo_core::render::Cache::new()).unwrap();
        for (x,y) in [(10,10),(160,90),(310,170)] {let expected=sample(kind,19,(x as f64+0.5)/180.0*8.0+0.125,(y as f64+0.5)/180.0*8.0).unwrap().value*gain+0.5;
            assert!((frame.d[(y*320+x)*4] as f64-expected).abs()<2e-6,"{name}");}
    }
}
#[test] fn invalid_effect_and_resource_errors_do_not_mutate_pixels() {
    let mut f=Frame::new(8,8);f.d.fill(0.25);let before=f.d.clone();
    let mut p=Params::new();p.insert("frequency".into(),Prop::new(f64::INFINITY));
    assert!(kimo_core::noise::apply(&p,&BTreeMap::new(),&mut f,0.0).is_err());assert_eq!(f.d,before);
    p.clear();let mut lim=kimo_core::resource_limits::current();lim.max_pixels=10;
    let _scope=kimo_core::resource_limits::install(lim);
    assert!(kimo_core::noise::apply(&p,&BTreeMap::new(),&mut f,0.0).is_err());assert_eq!(f.d,before);
}
#[test] fn signal_is_nonconstant_without_accidental_axis_only_seed_pattern() {
    for k in [Kind::Perlin,Kind::Simplex] {let mut sum=0.0;let mut sum2=0.0;let mut count=0.0;
        for y in 0..80 {for x in 0..80 {let v=sample(k,291,x as f64*0.083+0.17,y as f64*0.091+0.31).unwrap().value;sum+=v;sum2+=v*v;count+=1.0;}}
        assert!(sum2/count-(sum/count).powi(2)>1e-6);
        assert_ne!(sample(k,291,0.17,0.31).unwrap(),sample(k,291,0.31,0.17).unwrap());
    }
}
