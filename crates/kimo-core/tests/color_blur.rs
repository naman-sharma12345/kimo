use kimo_core::{color::blur::blur,effects::Frame};
#[test]fn constant_borders(){for(w,h)in[(1,1),(2,3),(32,16)]{let mut f=Frame{w,h,d:vec![0.3;w*h*4]};blur(&mut f,99999999);for v in f.d{assert!((v-0.3).abs()<1e-7)}}}
#[test]fn alpha_preserved(){let mut f=Frame{w:8,h:8,d:vec![0.;256]};for i in 0..64{f.d[4*i+3]=i as f32/63.}f.d[4*32]=1.;let a:Vec<_>=f.d.chunks_exact(4).map(|v|v[3]).collect();blur(&mut f,4);assert_eq!(a,f.d.chunks_exact(4).map(|v|v[3]).collect::<Vec<_>>());assert!(f.d.iter().all(|v|v.is_finite()))}
