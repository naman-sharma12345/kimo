use kimo_core::{effects::Frame,transition_raw::{render,Edge,Operator}};
fn frame(rgb:[f32;4])->Frame{Frame{w:2,h:2,d:rgb.repeat(4)}}
#[test]fn endpoints_are_bit_exact_for_all_operators(){let a=frame([-2.0,4.0,0.2,0.0]);let b=frame([8.0,-1.0,0.3,0.5]);let m=frame([0.5,0.0,0.0,1.0]);for op in [Operator::Luma{smoothing:0.1},Operator::DirectionalWarp{direction:[1.0,0.0],smoothing:0.1,distortion:1.0},Operator::CrossZoom{center:[0.5,0.5],strength:0.4,samples:16},Operator::Displacement{strength:[0.1,0.2],neutral:0.5}]{assert_eq!(render(&a,&b,Some(&m),0.0,&op,Edge::Clamp).unwrap().d,a.d);assert_eq!(render(&a,&b,Some(&m),1.0,&op,Edge::Clamp).unwrap().d,b.d);}}
#[test]fn premult_mix_ignores_transparent_hidden_color_and_preserves_hdr(){let a=frame([100.0,0.0,0.0,0.0]);let b=frame([0.0,4.0,-2.0,1.0]);let op=Operator::CrossZoom{center:[0.5,0.5],strength:0.0,samples:1};let f=render(&a,&b,None,0.5,&op,Edge::Clamp).unwrap();for p in f.d.chunks_exact(4){assert_eq!(p,[0.0,4.0,-2.0,0.5]);}}
#[test]fn luma_zero_smoothing_is_finite_and_threshold_correct(){let a=frame([1.0,0.0,0.0,1.0]);let b=frame([0.0,1.0,0.0,1.0]);for (l,expected) in [(0.2,&b),(0.8,&a)]{let m=frame([l,0.0,0.0,1.0]);assert_eq!(render(&a,&b,Some(&m),0.5,&Operator::Luma{smoothing:0.0},Edge::Clamp).unwrap().d,expected.d);}}
#[test]fn zero_displacement_is_raw_crossfade(){let a=frame([2.0,0.0,0.0,1.0]);let b=frame([0.0,2.0,0.0,1.0]);let m=frame([0.5,0.0,0.0,1.0]);let f=render(&a,&b,Some(&m),0.25,&Operator::Displacement{strength:[1.0,1.0],neutral:0.5},Edge::Clamp).unwrap();assert_eq!(&f.d[0..4],&[1.5,0.5,0.0,1.0]);}
#[test]fn invalid_controls_and_singular_direction_rejected(){let a=frame([1.0;4]);for dir in [[0.0;2],[f64::NAN,1.0]]{assert!(render(&a,&a,None,0.5,&Operator::DirectionalWarp{direction:dir,smoothing:0.0,distortion:1.0},Edge::Clamp).is_err());}assert!(render(&a,&a,None,0.5,&Operator::Luma{smoothing:0.1},Edge::Clamp).is_err());assert!(render(&a,&a,None,0.5,&Operator::CrossZoom{center:[0.5;2],strength:0.0,samples:0},Edge::Clamp).is_err());}
#[test]fn extreme_direction_normalization_and_operators_are_finite(){let a=frame([1.0;4]);for direction in [[f64::MAX,f64::MAX],[f64::from_bits(1),0.0]]{let f=render(&a,&a,None,0.5,&Operator::DirectionalWarp{direction,smoothing:0.0,distortion:1.0},Edge::Repeat).unwrap();assert!(f.d.iter().all(|v|v.is_finite()));}}
/// Produces actual Rust-rendered 1080p PPM frames for external contact-sheet/clip
/// assembly. Not a golden update and never substitutes for visual inspection.
#[test]#[ignore="1080p render gate after compile and functional tests"]
fn render_1080p_contact_sources(){
 use std::{io::Write,path::PathBuf};
 let dir=PathBuf::from(std::env::var("KIMO_TRANSITION_FRAMES").expect("set output directory"));std::fs::create_dir_all(&dir).unwrap();
 let (w,h)=(1920,1080);let mut a=Frame{w,h,d:vec![0.0;w*h*4]};let mut b=Frame{w,h,d:vec![0.0;w*h*4]};let mut m=Frame{w,h,d:vec![0.0;w*h*4]};
 for y in 0..h{for x in 0..w{let i=(y*w+x)*4;let check=if (x/32+y/32)%2==0{0.1}else{0.9};a.d[i..i+4].copy_from_slice(&[check,x as f32/w as f32,0.2,1.0]);b.d[i..i+4].copy_from_slice(&[0.2,check,y as f32/h as f32,if x<w/2{0.4}else{1.0}]);m.d[i]=x as f32/(w-1) as f32;m.d[i+3]=1.0;}}
 let ops=[("luma",Operator::Luma{smoothing:0.08}),("directional",Operator::DirectionalWarp{direction:[1.0,0.4],smoothing:0.08,distortion:1.0}),("zoom",Operator::CrossZoom{center:[0.5,0.5],strength:0.4,samples:32}),("displace",Operator::Displacement{strength:[0.2,0.0],neutral:0.5})];
 for (name,op) in ops{for step in 0..=8{let f=render(&a,&b,Some(&m),step as f64/8.0,&op,Edge::Clamp).unwrap();let mut file=std::fs::File::create(dir.join(format!("{name}-{step:02}.ppm"))).unwrap();write!(file,"P6\n{w} {h}\n255\n").unwrap();let mut rgb=Vec::with_capacity(w*h*3);for (i,p) in f.d.chunks_exact(4).enumerate(){let background=if (i%w/16+i/w/16)%2==0{0.2}else{0.7};for v in p.iter().take(3){rgb.push(((v*p[3]+background*(1.0-p[3])).clamp(0.0,1.0)*255.0+0.5) as u8);}}file.write_all(&rgb).unwrap();}}
}

#[test]fn crossfade_and_slide_endpoints_exact_including_hidden_rgb(){
 let a=frame([-2.0,8.0,0.3,0.0]);let b=frame([4.0,-1.0,0.2,0.5]);
 for op in [Operator::Crossfade,Operator::Slide{from_offset:[2.0,-3.0],to_offset:[-2.0,3.0]}]{for edge in [Edge::Clamp,Edge::Repeat,Edge::Transparent]{
  assert_eq!(render(&a,&b,None,0.0,&op,edge).unwrap().d,a.d);assert_eq!(render(&a,&b,None,1.0,&op,edge).unwrap().d,b.d);
 }}
}
#[test]fn crossfade_is_linear_in_premult_color_and_alpha(){
 let a=frame([4.0,-2.0,1.0,0.25]);let b=frame([0.0,2.0,3.0,0.75]);
 let f=render(&a,&b,None,0.5,&Operator::Crossfade,Edge::Transparent).unwrap();
 for p in f.d.chunks_exact(4){assert_eq!(p,[1.0,1.0,2.5,0.5]);}
 let hidden=frame([100.0,-100.0,8.0,0.0]);let f=render(&hidden,&hidden,None,0.5,&Operator::Crossfade,Edge::Clamp).unwrap();assert!(f.d.iter().all(|v|*v==0.0));
}
#[test]fn slide_zero_offsets_equal_crossfade_with_float_tolerance(){
 let a=Frame{w:3,h:1,d:vec![4.0,0.0,1.0,0.2, 0.0,2.0,-1.0,1.0, 100.0,0.0,0.0,0.0]};let b=frame([0.0,0.0,0.0,1.0]);let b=Frame{w:3,h:1,d:b.d[..12].to_vec()};
 for t in [0.1,0.5,0.9]{let x=render(&a,&b,None,t,&Operator::Crossfade,Edge::Clamp).unwrap();let y=render(&a,&b,None,t,&Operator::Slide{from_offset:[0.0;2],to_offset:[0.0;2]},Edge::Clamp).unwrap();for (x,y) in x.d.iter().zip(&y.d){assert!((x-y).abs()<1e-5);}}
}
#[test]fn slide_direct_uv_offsets_and_edge_policy(){
 let a=Frame{w:2,h:1,d:vec![1.0,0.0,0.0,1.0, 0.0,1.0,0.0,1.0]};let b=Frame{w:2,h:1,d:vec![0.0;8]};
 let op=Operator::Slide{from_offset:[1.0,0.0],to_offset:[0.0;2]};
 let f=render(&a,&b,None,0.5,&op,Edge::Clamp).unwrap();assert_eq!(&f.d[..4],&[0.0,1.0,0.0,0.5]);
 let f=render(&a,&b,None,0.5,&op,Edge::Repeat).unwrap();assert_eq!(&f.d[4..8],&[1.0,0.0,0.0,0.5]);
 let f=render(&a,&b,None,0.5,&op,Edge::Transparent).unwrap();assert_eq!(&f.d[4..8],&[0.0;4]);
}
#[test]fn added_operators_finite_safety(){
 let a=frame([f32::MAX,-f32::MAX,0.0,1.0]);
 for op in [Operator::Crossfade,Operator::Slide{from_offset:[f64::MAX,-f64::MAX],to_offset:[0.0;2]}]{let f=render(&a,&a,None,0.5,&op,Edge::Clamp).unwrap();assert!(f.d.iter().all(|v|v.is_finite()));assert!(render(&a,&a,None,f64::NAN,&op,Edge::Clamp).is_err());}
 for bad in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY]{for axis in 0..2{let mut offset=[0.0;2];offset[axis]=bad;for op in [Operator::Slide{from_offset:offset,to_offset:[0.0;2]},Operator::Slide{from_offset:[0.0;2],to_offset:offset}]{assert!(render(&a,&a,None,0.5,&op,Edge::Clamp).is_err());}}}
}
/// Separate new-operator harness: existing four-port render function is unchanged.
#[test]#[ignore="additional 1080p render gate after compile and functional tests"]
fn render_1080p_crossfade_slide_sources(){
 use std::{io::Write,path::PathBuf};
 let dir=PathBuf::from(std::env::var("KIMO_TRANSITION_FRAMES").expect("set output directory"));std::fs::create_dir_all(&dir).unwrap();
 let (w,h)=(1920usize,1080usize);let mut a=Frame{w,h,d:vec![0.0;w*h*4]};let mut b=Frame{w,h,d:vec![0.0;w*h*4]};
 for y in 0..h{for x in 0..w{let i=(y*w+x)*4;let check=if (x/32+y/32)%2==0{0.15}else{0.85};a.d[i..i+4].copy_from_slice(&[check,0.2,x as f32/w as f32,if x<w/2{0.3}else{1.0}]);b.d[i..i+4].copy_from_slice(&[0.2,check,y as f32/h as f32,if y<h/2{0.5}else{1.0}]);}}
 for (name,op) in [("crossfade",Operator::Crossfade),("slide",Operator::Slide{from_offset:[1.0,0.0],to_offset:[-1.0,0.0]})]{for step in 0..=8{
  let f=render(&a,&b,None,step as f64/8.0,&op,Edge::Clamp).unwrap();let mut file=std::fs::File::create(dir.join(format!("{name}-{step:02}.ppm"))).unwrap();write!(file,"P6\n{w} {h}\n255\n").unwrap();let mut rgb=Vec::with_capacity(w*h*3);
  for (i,p) in f.d.chunks_exact(4).enumerate(){let background=if (i%w/16+i/w/16)%2==0{0.2}else{0.7};for v in p.iter().take(3){rgb.push(((v*p[3]+background*(1.0-p[3])).clamp(0.0,1.0)*255.0+0.5) as u8);}}file.write_all(&rgb).unwrap();
 }}
}
