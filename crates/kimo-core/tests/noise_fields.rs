//! Numeric gates for separate raw operators. No premium claim.
use kimo_core::noise::{Kind,sample};
use kimo_core::noise_fields::{worley,Fbm,Warp,curl,parse};
use kimo_core::commands::{Session,dispatch};
use serde_json::json;
#[test]fn fbm_one_octave_is_exact_identity_and_normalization_explicit(){for k in [Kind::Perlin,Kind::Simplex]{
    let f=Fbm{kind:k,seed:7,octaves:1,lacunarity:2.0,gain:0.5,normalize:false};assert_eq!(f.sample(0.13,-0.72).unwrap(),sample(k,7,0.13,-0.72).unwrap());
    let f=Fbm{kind:k,seed:7,octaves:4,lacunarity:2.0,gain:-0.5,normalize:false};let a=f.sample(0.13,-0.72).unwrap();let b=Fbm{normalize:true,..f}.sample(0.13,-0.72).unwrap();let norm=1.0+0.5+0.25+0.125;
    assert!((a.value/norm-b.value).abs()<1e-12);for j in 0..2{assert!((a.gradient[j]/norm-b.gradient[j]).abs()<1e-12);}
}}
#[test]fn warp_zero_is_exact_identity(){for k in [Kind::Perlin,Kind::Simplex]{let w=Warp{source:k,source_seed:7,warp:Kind::Perlin,warp_seed:99,frequency:2.0,amplitude:0.0};assert_eq!(w.sample(0.13,-0.72).unwrap(),sample(k,7,0.13,-0.72).unwrap());}}
#[test]fn fbm_and_warp_chain_rule_finite_difference(){for k in [Kind::Perlin,Kind::Simplex]{
    let f=Fbm{kind:k,seed:7,octaves:4,lacunarity:1.73,gain:0.45,normalize:true};let w=Warp{source:k,source_seed:8,warp:k,warp_seed:10,frequency:1.4,amplitude:0.7};
    for n in 0..100{let p=[n as f64*0.13+0.17,n as f64*0.19-4.8];for op in 0..2{let eval=|x,y|if op==0{f.sample(x,y)}else{w.sample(x,y)};let s=eval(p[0],p[1]).unwrap();
        for j in 0..2{let(mut a,mut b)=(p,p);let e=1e-6;a[j]+=e;b[j]-=e;let fd=(eval(a[0],a[1]).unwrap().value-eval(b[0],b[1]).unwrap().value)/(2.0*e);assert!((fd-s.gradient[j]).abs()<1e-5,"{k:?} {op} {p:?}");}
    }}
}}
#[test]fn cellular_bounds_gradients_and_order_independent(){for seed in [0,1,u64::MAX]{for n in 0..99{let(x,y)=(n as f64*0.17-8.2,n as f64*0.29-13.8);let w=worley(seed,x,y).unwrap();assert!(w.f1>=0.0&&w.f1<=w.f2&&w.f2<=5.0f64.sqrt()+1e-12);
    assert_ne!(w.cell1,w.cell2);assert!((w.gradient1[0].hypot(w.gradient1[1])-1.0).abs()<1e-10);
    for j in 0..2{let mut a=[x,y];let mut b=a;let e=1e-6;a[j]+=e;b[j]-=e;let wa=worley(seed,a[0],a[1]).unwrap();let wb=worley(seed,b[0],b[1]).unwrap();if wa.cell1==w.cell1&&wb.cell1==w.cell1{assert!(((wa.f1-wb.f1)/(2.0*e)-w.gradient1[j]).abs()<1e-6);}}
    assert_eq!(w,worley(seed,x,y).unwrap());
}}}
#[test]fn curl_is_divergence_free_and_frequency_chain_rule(){for k in [Kind::Perlin,Kind::Simplex]{for n in 0..100{let(x,y)=(n as f64*0.17+0.18,n as f64*0.23-4.7);let e=1e-5;
    let dx=(curl(k,8,x+e,y,1.7,0.4).unwrap()[0]-curl(k,8,x-e,y,1.7,0.4).unwrap()[0])/(2.0*e);
    let dy=(curl(k,8,x,y+e,1.7,0.4).unwrap()[1]-curl(k,8,x,y-e,1.7,0.4).unwrap()[1])/(2.0*e);assert!((dx+dy).abs()<1e-5,"{k:?}: {}",dx+dy);
    let g=sample(k,8,x*1.7,y*1.7).unwrap().gradient;let v=curl(k,8,x,y,1.7,0.4).unwrap();assert_eq!(v,[(1.7*0.4)*g[1],-(1.7*0.4)*g[0]]);
    assert_eq!(curl(k,8,x,y,1.0,0.0).unwrap(),[0.0,-0.0]);
}}}
#[test]fn config_rejects_invalid_and_overflow(){
    for v in [json!({"op":"unknown"}),json!({"op":"fbm","octaves":0}),json!({"op":"fbm","octaves":17}),json!({"op":"fbm","normalize":1}),json!({"op":"warp","seed":-1}),json!({"op":"curl","channel":2})]{assert!(parse(&v).is_err());}
    let f=Fbm{kind:Kind::Perlin,seed:0,octaves:16,lacunarity:1e100,gain:2.0,normalize:false};assert!(f.sample(2.0,3.0).is_err());
}
#[test]fn mcp_operator_each_is_independently_callable(){let mut ss=Session::new();assert_eq!(kimo_core::toolgroups::group_of("sample_field"),"effects");
    for op in ["worley","fbm","warp","curl"]{let r=dispatch(&mut ss,"sample_field",&json!({"op":op,"seed":7,"points":[[0.17,-0.31]],"octaves":3,"normalize":true})).unwrap();assert_eq!(r["samples"].as_array().unwrap().len(),1);}
}
#[test]fn selected_field_effect_is_transactional_and_alpha_preserving(){use kimo_core::{effects::{Frame,Params},keyframe::Prop};use std::collections::BTreeMap;
    for op in ["worley","fbm","warp","curl"]{let mut f=Frame::new(32,18);for px in f.d.chunks_exact_mut(4){px.copy_from_slice(&[0.2,0.3,0.4,0.37]);}
        let mut strings=BTreeMap::new();strings.insert("field".into(),json!({"op":op,"seed":7,"octaves":2}).to_string());
        kimo_core::effects::apply("field_operator",&Params::new(),&strings,&mut f,0.25,0).unwrap();assert!(f.d.chunks_exact(4).all(|p|p[3]==0.37));
        let before=f.d.clone();let mut p=Params::new();p.insert("frequency".into(),Prop::new(1e30));assert!(kimo_core::effects::apply("field_operator",&p,&strings,&mut f,0.25,0).is_err());assert_eq!(before,f.d);
    }
}
