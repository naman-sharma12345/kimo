use kimo_core::{commands::{dispatch,Session},dsp::pitch::pitch_detect};
use serde_json::json;
fn tone(hz:f64) -> Vec<f32> { (0..4096).map(|i| (0.4*(std::f64::consts::TAU*hz*i as f64/48000.).sin()) as f32).collect() }
#[test]
fn pure_tones_half_percent_and_threads() {
    for hz in [110.,440.,1760.] {
        let x=tone(hz); let mut values=vec![];
        for n in [1,4] { kimo_core::par::set_threads(n); let (f,c)=pitch_detect(&x,48000.,50.,2000.,0.15).unwrap(); assert!((f as f64/hz-1.).abs()<0.005,"{hz} -> {f}"); assert!(c>0.9); values.push((f.to_bits(),c.to_bits())); }
        assert_eq!(values[0],values[1]);
    }
}
#[test]
fn noise_silence_and_invalid_unvoiced() {
    let mut seed=1u64; let noise:Vec<f32>=(0..4096).map(|_| {seed=seed.wrapping_mul(6364136223846793005).wrapping_add(1); ((seed>>32) as u32 as f64/u32::MAX as f64*2.-1.) as f32}).collect();
    assert!(pitch_detect(&noise,48000.,50.,2000.,0.15).map_or(true,|(_,c)|c<0.3));
    assert!(pitch_detect(&vec![0.;4096],48000.,50.,2000.,0.15).is_none());
    assert!(pitch_detect(&[f32::NAN;4096],48000.,50.,2000.,0.15).is_none());
    assert!(pitch_detect(&tone(440.),48000.,2000.,50.,0.15).is_none());
}
#[test]
fn read_only_tool_tones_and_noise() {
    let mut s=Session::new(); let before=serde_json::to_string(&s.project).unwrap();
    for hz in [110.,440.,1760.] {
        let pcm:Vec<f32>=(0..9600).flat_map(|i| {let x=(0.4*(std::f64::consts::TAU*hz*i as f64/48000.).sin()) as f32; [x,x]}).collect();
        let path=std::env::temp_dir().join(format!("kimo-pitch-{}-{hz}.wav",std::process::id()));
        kimo_core::audio::write_wav(path.to_str().unwrap(),&pcm).unwrap();
        let args=json!({"target":{"path":path},"min_hz":50,"max_hz":2000});
        let mut results=vec![];
        for n in [1,4] {kimo_core::par::set_threads(n); results.push(dispatch(&mut s,"audio_pitch",&args).unwrap());}
        std::fs::remove_file(path).unwrap();
        assert_eq!(serde_json::to_vec(&results[0]).unwrap(),serde_json::to_vec(&results[1]).unwrap());
        for row in results[0]["frames"].as_array().unwrap() {assert!((row["hz"].as_f64().unwrap()/hz-1.).abs()<0.005);assert!(row["confidence"].as_f64().unwrap()>0.9);}
        assert!(!results[0]["frames"].as_array().unwrap().is_empty());
    }
    assert_eq!(serde_json::to_string(&s.project).unwrap(),before);
}

#[test]
fn beatbox_yin_error_is_small_and_bad_parameters() {
    for hz in [41.2,55.,110.,261.63,440.,987.77] {
        let x=tone(hz); let mut bytes=vec![];
        for n in [1,4] {kimo_core::par::set_threads(n); let (f,c)=pitch_detect(&x,48000.,30.,1600.,0.15).unwrap();
            let cents=1200.*(f as f64/hz).log2(); eprintln!("YIN tone={hz} hz={f:.7} cents={cents:.7} confidence={c:.7} threads={n}");
            assert!(cents.abs()<5.,"{hz}: {cents} cents");assert!(c>0.9);bytes.push((f.to_bits(),c.to_bits()));}
        assert_eq!(bytes[0],bytes[1]);
    }
    let x=tone(440.);
    assert!(pitch_detect(&x[..63],48000.,30.,1600.,0.15).is_none());
    for (min,max,thr) in [(10.,1600.,0.15),(30.,24000.,0.15),(30.,1600.,0.),(30.,1600.,1.)] {
        assert!(pitch_detect(&x,48000.,min,max,thr).is_none());
    }
}
#[test]
fn strict_threshold_boundary_tone() {
    let x=tone(1760.); let max_tau=960usize;let w=x.len()-max_tau;
    let d:Vec<f32>=(0..=max_tau).map(|tau| (0..w).map(|j| {let v=x[j]-x[j+tau];v*v}).sum()).collect();
    let mut sum=0f32;let mut cmnd=vec![1f32;max_tau+1];
    for t in 1..=max_tau {sum+=d[t];cmnd[t]=d[t]*t as f32/sum;}
    // Select the minimum CMND lag among the first few periods to make an exact-equality boundary.
    let tau=(24..100).min_by(|a,b| cmnd[*a].total_cmp(&cmnd[*b])).unwrap();
    let threshold=cmnd[tau];
    let exact=pitch_detect(&x,48000.,50.,2000.,threshold);
    // At equality that lag must not be accepted. Another lag can be selected, but not this one.
    assert!(exact.map_or(true,|(f,_)|(f-48000./tau as f32).abs()>20.),"strict equality accepted boundary lag: {exact:?}");
}

#[test]
fn tool_noise_cap_and_parameter_errors() {
    let mut s=Session::new(); let mut seed=17u64;
    let pcm:Vec<f32>=(0..9600).flat_map(|_| {seed=seed.wrapping_mul(6364136223846793005).wrapping_add(1); let x=((seed>>32) as u32 as f64/u32::MAX as f64*0.8-0.4) as f32; [x,x]}).collect();
    let path=std::env::temp_dir().join(format!("kimo-pitch-noise-{}.wav",std::process::id()));
    kimo_core::audio::write_wav(path.to_str().unwrap(),&pcm).unwrap();
    let args=json!({"target":{"path":path},"max_items":2});
    let mut out=vec![];
    for n in [1,4] {kimo_core::par::set_threads(n);out.push(dispatch(&mut s,"audio_pitch",&args).unwrap());}
    assert_eq!(serde_json::to_vec(&out[0]).unwrap(),serde_json::to_vec(&out[1]).unwrap());
    assert_eq!(out[0]["count"],2);assert_eq!(out[0]["truncated"],true);
    for f in out[0]["frames"].as_array().unwrap(){assert!(f["confidence"].as_f64().unwrap()<0.3);assert_eq!(f["hz"],serde_json::Value::Null);}
    for (k,v) in [("min_hz",10),("max_hz",24000),("threshold",0),("threshold",1),("max_items",0)] {
        let mut a=json!({"target":{"path":path}});a[k]=json!(v);assert!(dispatch(&mut s,"audio_pitch",&a).is_err());
    }
    std::fs::remove_file(path).unwrap();
}
