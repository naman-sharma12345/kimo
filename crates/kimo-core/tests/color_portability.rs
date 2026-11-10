use kimo_core::commands::{dispatch,Session};
use serde_json::json;
#[test]fn baking_rejects_spatial_and_effect_mask(){let mut s=Session::new();let tr=dispatch(&mut s,"add_track",&json!({})).unwrap()["track_id"].as_str().unwrap().to_string();let c=dispatch(&mut s,"add_clip",&json!({"track_id":tr,"source_type":"solid","start":0,"duration":1})).unwrap()["clip_id"].as_str().unwrap().to_string();let e=dispatch(&mut s,"add_effect",&json!({"clip_id":c,"kind":"exposure","strings":{"mask_layer":"any"}})).unwrap()["effect_id"].as_str().unwrap().to_string();assert!(dispatch(&mut s,"export_grade_lut",&json!({"clip_id":c,"time":0,"path":"/tmp/no-masked.cube"})).is_err());dispatch(&mut s,"remove_effect",&json!({"clip_id":c,"effect_id":e})).unwrap();dispatch(&mut s,"add_effect",&json!({"clip_id":c,"kind":"halation"})).unwrap();assert!(dispatch(&mut s,"export_grade_lut",&json!({"clip_id":c,"time":0,"path":"/tmp/no-spatial.cube"})).is_err());}
#[test]fn serialized_managed_grade_roundtrip(){let mut s=Session::new();let tr=dispatch(&mut s,"add_track",&json!({})).unwrap()["track_id"].as_str().unwrap().to_string();let c=dispatch(&mut s,"add_clip",&json!({"track_id":tr,"source_type":"solid","start":0,"duration":1})).unwrap()["clip_id"].as_str().unwrap().to_string();dispatch(&mut s,"set_color_curve",&json!({"clip_id":c,"channel":"m","points":[[0,0],[0.2,0.3],[1,1]]})).unwrap();let serialized=serde_json::to_string(&s.project).unwrap();let restored:kimo_core::model::Project=serde_json::from_str(&serialized).unwrap();let orig=s.project.find_clip(&c).unwrap();let back=restored.find_clip(&c).unwrap();assert_eq!(orig.effects[0].kind,back.effects[0].kind);assert_eq!(serde_json::to_value(&orig.effects).unwrap(),serde_json::to_value(&back.effects).unwrap());}
#[test]fn serialized_pipeline_wrapper_curve_render_is_exact(){
    use kimo_core::{render,color::pipeline};
    let mut s=Session::new();dispatch(&mut s,"project_set",&json!({"width":32,"height":16,"fps":25})).unwrap();
    let tr=dispatch(&mut s,"add_track",&json!({})).unwrap()["track_id"].clone();
    let c=dispatch(&mut s,"add_clip",&json!({"track_id":tr,"source_type":"gradient","color":[0.05,0.1,0.2],"color2":[0.9,0.4,0.2],"start":0,"duration":1})).unwrap()["clip_id"].clone();
    dispatch(&mut s,"set_color_pipeline",&json!({"settings":{"enabled":true,"working_gamut":3,"output_gamut":1,"output_transfer":6,"reference_white":203}})).unwrap();
    dispatch(&mut s,"add_managed_linear_effect",&json!({"clip_id":c,"kind":"linear_grade","params":{"exposure":0.25,"saturation":1.2}})).unwrap();
    dispatch(&mut s,"set_color_curve",&json!({"clip_id":c,"channel":"m","points":[[0,0],[0.2,0.3],[1,1.1]]})).unwrap();
    let e=dispatch(&mut s,"add_effect",&json!({"clip_id":c,"kind":"cdl","params":{"slope_r":1.05,"saturation":0.8}})).unwrap()["effect_id"].clone();
    dispatch(&mut s,"set_effect_encoding",&json!({"clip_id":c,"effect_id":e,"gamut":0,"transfer":1})).unwrap();
    let serialized=serde_json::to_string(&s.project).unwrap();let restored:kimo_core::model::Project=serde_json::from_str(&serialized).unwrap();
    assert_eq!(pipeline::inspect(&s.project).unwrap(),pipeline::inspect(&restored).unwrap());
    for t in [0.,0.2,0.76]{let mut a=render::render_frame(&s.project,t,&s.cache).unwrap();let mut b=render::render_frame(&restored,t,&render::Cache::new()).unwrap();assert_eq!(a.d,b.d);pipeline::output(&s.project,&mut a).unwrap();pipeline::output(&restored,&mut b).unwrap();assert_eq!(pipeline::rgba16(&a).unwrap(),pipeline::rgba16(&b).unwrap());}
}
