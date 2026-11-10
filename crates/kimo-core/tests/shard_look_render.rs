use kimo_core::{effects::Frame, shards::{self, ShardBurst}, three_d::Camera};
use serde_json::{json, Value};
fn cfg(extra: Value) -> ShardBurst {
    let mut c = json!({"count":180,"size_min":3,"size_max":38,"origin_spread":140,"speed":240,"gravity":45,"spin":220,"dof":0,"shutter_deg":0,"seed":21,"brightness":1.0,"tint":[0.82,0.90,1.0],"thickness":[0.02,0.05]});
    for (k,v) in extra.as_object().unwrap() { c[k] = v.clone(); }
    serde_json::from_value(c).unwrap()
}
fn draw(c: &ShardBurst, t: f64) -> Frame {
    let mut f = Frame::new(640,360); shards::render(c,&mut f,t,&Camera::default(),30.0,2.0); f
}
#[test] fn zero_amount_frame_identity_and_alpha_preservation() {
    for material in ["legacy","glass","concrete","metal"] {
        let a = draw(&cfg(json!({"material":material})),0.55);
        let b = draw(&cfg(json!({"material":material,"env_reflection":0.0,"env_roughness":1.0})),0.55);
        assert!(a.d==b.d,"frames differ");
        let b = draw(&cfg(json!({"material":material,"env_reflection":0.8})),0.55);
        let mut changed = 0;
        for (aa,bb) in a.d.chunks_exact(4).zip(b.d.chunks_exact(4)) {
            assert_eq!(aa[3],bb[3]);
            if aa[3]==0.0 { assert_eq!(aa,bb); }
            if aa[..3]!=bb[..3] { changed+=1; }
        }
        assert!(changed>100,"reflection must affect visible shards: {material}");
    }
}
#[test] fn deterministic_analytic_and_physics() {
    for physics in [false,true] {
        let c = cfg(json!({"material":"glass","env_reflection":0.8,"size_bias":2.8,"size_scale":0.6,"physics":physics,"collide":false}));
        let a=draw(&c,0.55); let b=draw(&c,0.55);
        assert!(a.d==b.d,"frames differ"); assert!(a.d.iter().all(|x|x.is_finite()));
    }
}
#[test] fn hostile_controls_in_renderer() {
    for v in [-1e30,0.0,1e30] {
        let c=cfg(json!({"count":12,"size_min":-2,"size_max":-30,"size_bias":v,"size_scale":v,"env_reflection":v,"env_roughness":v,"material":"glass"}));
        assert!(draw(&c,0.5).d.iter().all(|x|x.is_finite()));
    }
    let mut c=cfg(json!({"count":12,"material":"glass"}));
    c.size_min=f64::NAN; c.size_max=f64::INFINITY; c.size_bias=f64::NAN; c.size_scale=f64::NEG_INFINITY;
    c.env_reflection=f64::NAN; c.env_roughness=f64::INFINITY;
    assert!(draw(&c,0.5).d.iter().all(|x|x.is_finite()));
    assert!(serde_json::from_value::<ShardBurst>(json!({"material":"unknown"})).is_err());
}
#[test] fn write_review_frames() {
    let Some(dir)=std::env::var_os("SHARD_LOOK_EVIDENCE") else {return};
    let dir=std::path::PathBuf::from(dir); std::fs::create_dir_all(&dir).unwrap();
    let variants = [
        ("legacy",json!({})),
        ("fine_glass",json!({"material":"glass","size_min":2,"size_max":24,"size_bias":2.8,"size_scale":0.75,"count":450,"env_reflection":0.85,"env_roughness":0.15})),
        ("concrete",json!({"material":"concrete","tint":[0.75,0.71,0.66],"env_reflection":0.5})),
        ("metal",json!({"material":"metal","tint":[0.90,0.92,0.95],"env_reflection":0.85,"env_roughness":0.25})),
        ("glass_no_env",json!({"material":"glass"})),
        ("glass_env",json!({"material":"glass","env_reflection":0.85,"env_roughness":0.15})),
    ];
    for (name,extra) in variants { let now=std::time::Instant::now(); let f=draw(&cfg(extra),0.55);
        let raw: Vec<u8> = f.d.iter().flat_map(|x| x.to_le_bytes()).collect();
        std::fs::write(dir.join(format!("{name}.rgba32f")),raw).unwrap();
        let mut out=format!("P6\n{} {}\n255\n",f.w,f.h).into_bytes();
        for (i,p) in f.d.chunks_exact(4).enumerate() { let bg=if (i%f.w/32+i/f.w/32)%2==0 {0.06} else {0.09};
            for c in 0..3 {out.push(((p[c]*p[3]+bg*(1.0-p[3])).clamp(0.0,1.0)*255.0).round() as u8);} }
        std::fs::write(dir.join(format!("{name}.ppm")),out).unwrap();
        println!("{name}: {:?}",now.elapsed());
    }
}

#[test] fn reflection_respects_occluder_matte() {
    use std::sync::Arc;
    let c=cfg(json!({"material":"glass","env_reflection":1.0}));
    shards::EXT_OCC.with(|cell| *cell.borrow_mut()=Some(Arc::new(vec![1.0;640*360])));
    let f=draw(&c,0.55);
    assert!(f.d.iter().all(|x| *x==0.0));
}
