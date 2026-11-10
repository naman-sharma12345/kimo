//! TPDF dither verification (CP3): determinism, +/-1 LSB bound vs naive rounding, mean
//! preservation, and terrace destruction on a subtle gradient (the banding case measured
//! on the CP2 union frame: 4-13px flat runs at 1-LSB steps).
use kimo_core::effects::Frame;
use kimo_core::render::{to_rgba8,to_rgba8_tpdf};

fn ramp(w:usize,h:usize,lo:f32,hi:f32)->Frame{let mut f=Frame::try_new(w,h).unwrap();for y in 0..h{for x in 0..w{let v=lo+(hi-lo)*(x as f32+y as f32)/((w+h-2) as f32);let id=(y*w+x)*4;f.d[id]=v;f.d[id+1]=v;f.d[id+2]=v;f.d[id+3]=1.;}}f}

fn max_run(bytes:&[u8],w:usize,h:usize)->usize{let mut best=0;for y in 0..h{let mut run=1;for x in 1..w{let a=bytes[(y*w+x-1)*4];let b=bytes[(y*w+x)*4];if a==b{run+=1;}else{if run>best{best=run;}run=1;}}if run>best{best=run;}}best}

#[test]fn deterministic_and_within_one_lsb(){
 let f=ramp(64,64,0.3,0.4);
 let a=to_rgba8_tpdf(&f);let b=to_rgba8_tpdf(&f);
 assert_eq!(a,b,"dither must be deterministic (fixed seed)");
 let n=to_rgba8(&f);
 for(i,(d,u))in a.iter().zip(n.iter()).enumerate(){let diff=(*d as i16-*u as i16).abs();assert!(diff<=1,"pixel {i}: dithered {d} vs undithered {u} beyond 1 LSB");}
}

#[test]fn mean_preserved_on_gradient(){
 let f=ramp(256,256,0.3,0.4);
 let a=to_rgba8_tpdf(&f);let n=to_rgba8(&f);
 let mean=|v:&[u8]|v.iter().map(|x|*x as f64).sum::<f64>()/v.len() as f64;
 assert!((mean(&a)-mean(&n)).abs()<0.1,"dithered mean {} vs undithered {}",mean(&a),mean(&n));
}

#[test]fn terraces_destroyed_on_subtle_gradient(){
 // 0.35..0.365 over 512px: ~4 LSB total -> naive rounding shows ~128px terraces (the CP2 banding case)
 let f=ramp(512,4,0.35,0.365);
 let n=to_rgba8(&f);let a=to_rgba8_tpdf(&f);
 let plain=max_run(&n,512,4);let dith=max_run(&a,512,4);
 println!("TPDF terrace check: naive max run {plain}px, dithered max run {dith}px");
 assert!(plain>=32,"test setup: naive quantization must terrace (got {plain})");
 assert!(dith<=24,"dithered runs must be short (got {dith})"); // measured 18: TPDF kills the 135px terraces; short residual runs are chance noise, not visible bands
}
