use kimo_core::{color::scopes,effects::Frame};
#[test]fn seeded_scope_accounting_all_counted_pixels(){
    let mut seed=1234567u32;let mut d=Vec::new();let mut expected=0u64;
    for i in 0..1024{let mut px=[0f32;4];for v in &mut px[..3]{seed=seed.wrapping_mul(1664525).wrapping_add(1013904223);*v=(seed%10000) as f32/1000.-2.;}px[3]=if i%7==0{0.}else{0.5};if px[3]>0.{expected+=1}d.extend(px);}
    let f=Frame{w:64,h:16,d};
    for bins in [16,257,4096]{let v=scopes::data_domain(&f,bins,-0.5,2.,[0.2126,0.7152,0.0722],1.,true).unwrap();assert_eq!(v["pixels"],expected);assert_eq!(v["nonfinite_pixels"],0);assert_eq!(v["invalid_alpha_pixels"],0);assert_eq!(v["scaled_overflow_pixels"],0);
        for channel in 0..4{let inside:u64=v["histogram"][channel].as_array().unwrap().iter().map(|n|n.as_u64().unwrap()).sum();assert_eq!(inside+v["under_range"][channel].as_u64().unwrap()+v["over_range"][channel].as_u64().unwrap(),expected);}
        let wave:u64=v["waveform"].as_array().unwrap().iter().flat_map(|col|col.as_array().unwrap()).map(|n|n.as_u64().unwrap()).sum();let luma:u64=v["histogram"][3].as_array().unwrap().iter().map(|n|n.as_u64().unwrap()).sum();assert_eq!(wave,luma);
        let vectors:u64=v["vectorscope_709"].as_array().unwrap().iter().flat_map(|row|row.as_array().unwrap()).map(|n|n.as_u64().unwrap()).sum();assert_eq!(vectors,expected);
    }
}
