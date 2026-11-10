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
// PROBE NOTE (approved): cross-zoom renders show hard lines at x=959|960 and y=543|544 growing with t
// (row 12.7->39.3, col 34.8->62.6 mean max-channel deltas, t=1..7). These are the checker source's
// cell boundaries adjacent to the zoom center, preserved by the near-zero-pull quadrature samples and
// magnified by zoom convergence - inherent to a discontinuous source + declared no-jitter quadrature,
// not a sampling singularity. Premium path would supersample.
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
// Stability sweep: t in {0,.25,.5,.75,1} (and finer), outputs finite, alpha 0..1,
// straight RGB within the convex hull of input RGB (premult mixing is convex).
#[test]fn sweep_all_operators_finite_and_bounded(){
 let (w,h)=(7usize,5usize);
 let mk=|f:fn(usize,usize)->[f32;4]|->Frame{let mut d=vec![0.0;w*h*4];for y in 0..h{for x in 0..w{let p=f(x,y);d[(y*w+x)*4..(y*w+x)*4+4].copy_from_slice(&p);}}Frame{w,h,d}};
 let a=mk(|x,y|[x as f32/6.0*3.0-1.0,y as f32/4.0,if (x+y)%2==0{1.0}else{0.0},if x<3{0.4}else{1.0}]);
 let b=mk(|x,y|[1.0-x as f32/6.0,(x*y)as f32/24.0*8.0-2.0,0.25,if (x*3+y)%4==0{0.0}else{1.0}]);
 let m=mk(|x,y|[x as f32/6.0,0.0,0.0,1.0]);
 let ops=[Operator::Luma{smoothing:0.2},Operator::DirectionalWarp{direction:[3.0,-2.0],smoothing:0.15,distortion:1.7},Operator::CrossZoom{center:[0.5,0.5],strength:0.6,samples:24},Operator::Displacement{strength:[0.35,-0.2],neutral:0.5}];
 let hull=|f:&Frame,c:usize|->(f64,f64){let mut lo=f64::INFINITY;let mut hi=f64::NEG_INFINITY;for p in f.d.chunks_exact(4){if p[3]>0.0{lo=lo.min(p[c]as f64);hi=hi.max(p[c]as f64);}}(lo,hi)};
 for op in &ops{for step in 0..=64{let t=step as f64/64.0;
  let f=render(&a,&b,Some(&m),t,op,Edge::Clamp).unwrap();
  assert!(f.d.iter().all(|v|v.is_finite()),"nonfinite at t={t} op={op:?}");
  for p in f.d.chunks_exact(4){assert!((0.0..=1.0).contains(&p[3]),"alpha {} at t={t} op={op:?}",p[3]);
   if p[3]>0.0{for c in 0..3{let (lo,hi)=(hull(&a,c).0.min(hull(&b,c).0),hull(&a,c).1.max(hull(&b,c).1));
    assert!((lo-1e-4..=hi+1e-4).contains(&(p[c]as f64)),"rgb ch{c} {} outside [{lo},{hi}] at t={t} op={op:?}",p[c]);}}}
 }}}
// Edge policies: wrap identity, clamp border replication, transparent zero-alpha outside.
#[test]fn edge_policies_wrap_clamp_transparent(){
 let (w,h)=(4usize,1usize);
 let mk=|v:&[[f32;4]]|->Frame{Frame{w,h,d:v.concat()}};
 let a=mk(&[[1.0,0.0,0.0,1.0],[0.0,1.0,0.0,1.0],[0.0,0.0,1.0,1.0],[1.0,1.0,0.0,1.0]]);
 let b=mk(&[[0.0;4],[0.0;4],[0.0;4],[0.0;4]]);
 let m=mk(&[[1.0,0.0,0.0,1.0],[1.0,0.0,0.0,1.0],[1.0,0.0,0.0,1.0],[1.0,0.0,0.0,1.0]]);
 // displacement map red=1, neutral=0, strength=[-1.0,0] shifts u by +t*1.0 for the A sample -> full wrap at any t
 let op=Operator::Displacement{strength:[-1.0,0.0],neutral:0.0};
 let wrap=render(&a,&b,Some(&m),0.5,&op,Edge::Repeat).unwrap();
 let id0=render(&a,&b,Some(&m),0.0,&op,Edge::Repeat).unwrap();
 // at t=0.5 A-sample wrapped by exactly one width == unshifted A; B sample shifted by -(1-t) wraps too (B is zeros)
 for (p,q) in wrap.d.chunks_exact(4).zip(id0.d.chunks_exact(4)){assert!((p[3]-q[3]*0.5).abs()<1e-6,"repeat wrap alpha");}
 // PROBE NOTE (approved): displacement sign follows upstream Codrops convention (A at uv + t*d*strength,
// B at uv - (1-t)*d*strength); premult accumulation unpremultiplies to straight on return.
// clamp: u = uv + t*d*strength samples A off the RIGHT border -> right texel (yellow) replicated;
 // premultiplied mix with zero-alpha B unpremultiplies back to straight yellow at alpha 0.5
 let cl=render(&a,&b,Some(&m),0.5,&Operator::Displacement{strength:[2.0,0.0],neutral:0.0},Edge::Clamp).unwrap();
 for p in cl.d.chunks_exact(4){assert!((p[0]-1.0).abs()<1e-6&&(p[1]-1.0).abs()<1e-6&&p[2].abs()<1e-6&&(p[3]-0.5).abs()<1e-6,"clamp right-border replicate straight-alpha got {p:?}");}
 // transparent: same shift pushes the A sample fully outside -> alpha from A is 0, only B (zeros) remains
 let tr=render(&a,&b,Some(&m),0.5,&Operator::Displacement{strength:[2.0,0.0],neutral:0.0},Edge::Transparent).unwrap();
 for p in tr.d.chunks_exact(4){assert!(p[3].abs()<1e-6,"transparent outside alpha got {p:?}");}
}
// PROBE NOTE (approved): port is dark-first - w = 1 - smooth(t-s/2, t+s/2, l), w = B weight,
// matching upstream gl-transitions luma; band is the documented symmetric [t-s/2, t+s/2].
// Luma smoothstep shape: symmetric midpoint, full-A below the window, full-B above, monotone.
#[test]fn luma_smoothstep_shape(){
 let a=frame([1.0,0.0,0.0,1.0]);let b=frame([0.0,0.0,1.0,1.0]);
 let w_at=|l:f32,t:f64,s:f64|->f32{let m=frame([l,0.0,0.0,1.0]);render(&a,&b,Some(&m),t,&Operator::Luma{smoothing:s},Edge::Clamp).unwrap().d[2]};
 let (t,s)=(0.5,0.2);
 assert!((w_at(0.5,t,s)-0.5).abs()<1e-6,"midpoint symmetric");
 // port is dark-first, matching upstream gl-transitions luma: w = 1 - smooth(t-s/2, t+s/2, l), w = B weight
 assert!(w_at(0.39,t,s)>1.0-1e-6,"below window full B (dark-first)");
 assert!(w_at(0.61,t,s)<1e-6,"above window full A");
 assert!(w_at(0.55,t,s)<w_at(0.45,t,s),"monotone decreasing in l (dark-first)");
}
// Extreme centers/directions stay finite and in-bounds on all three edge policies.
#[test]fn extreme_centers_and_directions_stay_finite(){
 let a=frame([0.8,0.1,0.2,1.0]);let b=frame([0.1,0.9,0.3,1.0]);
 for edge in [Edge::Clamp,Edge::Repeat,Edge::Transparent]{
  for op in [Operator::CrossZoom{center:[1e6,-1e6],strength:1.0,samples:8},Operator::CrossZoom{center:[0.0,1.0],strength:5.0,samples:128},Operator::DirectionalWarp{direction:[1e300,1.0],smoothing:0.9,distortion:-3.0},Operator::DirectionalWarp{direction:[-1e-300,1e-300],smoothing:0.0,distortion:0.0}]{
   for step in 0..=8{let t=step as f64/8.0;let f=render(&a,&b,None,t,&op,edge).unwrap();
    assert!(f.d.iter().all(|v|v.is_finite()),"nonfinite t={t} edge={edge:?} op={op:?}");
    for p in f.d.chunks_exact(4){assert!((0.0..=1.0).contains(&p[3]));}}}
 }}
// Malformed buffers rejected.
#[test]fn malformed_buffers_rejected(){
 let good=frame([1.0;4]);
 let badlen=Frame{w:2,h:2,d:vec![1.0;15]};
 let badalpha=frame([0.0,0.0,0.0,1.5]);
 let nanrgb=frame([f32::NAN,0.0,0.0,1.0]);
 let zero=Frame{w:0,h:2,d:vec![]};
 let op=Operator::Luma{smoothing:0.1};
 for bad in [&badlen,&badalpha,&nanrgb,&zero]{assert!(render(bad,&good,Some(&good),0.5,&op,Edge::Clamp).is_err());assert!(render(&good,bad,Some(&good),0.5,&op,Edge::Clamp).is_err());}
 let wrongmap=Frame{w:1,h:1,d:vec![0.5,0.0,0.0,1.0]};
 assert!(render(&good,&good,Some(&wrongmap),0.5,&op,Edge::Clamp).is_err());
}

// == crossfade+slide supplementary rows ==
// Banding: alpha-ramp crossfade is exactly linear per pixel (no quantization/steps in f32 pipeline).
#[test]fn crossfade_alpha_ramp_exact_linear(){
 let n=64usize;let mut a=Frame{w:n,h:1,d:vec![0.0;n*4]};let mut b=Frame{w:n,h:1,d:vec![0.0;n*4]};
 for x in 0..n{let u=x as f32/(n-1) as f32;a.d[x*4..x*4+4].copy_from_slice(&[0.2,0.4,u,1.0-u]);b.d[x*4..x*4+4].copy_from_slice(&[0.9,u,0.1,u]);}
 for t in [0.0,0.125,0.5,0.875,1.0].iter(){let f=render(&a,&b,None,*t,&Operator::Crossfade,Edge::Clamp).unwrap();
  for x in 0..n{let p=&f.d[x*4..x*4+4];
   let (aa,ba)=(a.d[x*4+3] as f64,b.d[x*4+3] as f64);let aw=(1.0-t)*aa+ t*ba;
   assert!((p[3] as f64-aw).abs()<1e-6,"alpha not linear at x={x} t={t}");
   if aw>1e-9{let pc=((1.0-t)*a.d[x*4] as f64*aa+t*b.d[x*4] as f64*ba)/aw;assert!((p[0] as f64-pc).abs()<1e-5,"premult color off at x={x} t={t}: {} vs {pc}",p[0]);}
 }}
}
// Dark halo: opaque red vs alpha-0 B (hidden RGB must stay hidden) -> straight red at reduced alpha, never darkened.
#[test]fn crossfade_no_dark_halo_on_zero_alpha(){
 let a=frame([1.0,0.0,0.0,1.0]);let b=frame([0.0,1.0,0.0,0.0]);
 for t in [0.25,0.5,0.75]{let f=render(&a,&b,None,t,&Operator::Crossfade,Edge::Clamp).unwrap();
  for p in f.d.chunks_exact(4){assert!((p[0]-1.0).abs()<1e-6&&p[1].abs()<1e-6&&p[2].abs()<1e-6&&(p[3]-(1.0-t)as f32).abs()<1e-6,"halo/darkening at t={t}: {p:?}");}}
}
// HDR: signed HDR values pass through crossfade unclipped.
#[test]fn crossfade_hdr_unclipped(){
 let a=frame([3.0,-1.5,0.5,1.0]);let b=frame([-1.0,4.0,2.0,1.0]);
 let f=render(&a,&b,None,0.25,&Operator::Crossfade,Edge::Clamp).unwrap();
 for p in f.d.chunks_exact(4){assert!((p[0]-2.0).abs()<1e-6&&(p[1]-(-0.125)).abs()<1e-6&&(p[2]-0.875).abs()<1e-6,"hdr clipped: {p:?}");}
}
// Slide subpixel: half-pixel shift on a smooth ramp gives exact neighbor averages (no 1px seam/step).
#[test]fn slide_subpixel_half_offset_exact_bilinear(){
 let n=8usize;let mut a=Frame{w:n,h:1,d:vec![0.0;n*4]};let mut b=Frame{w:n,h:1,d:vec![0.0;n*4]};
 for x in 0..n{a.d[x*4..x*4+4].copy_from_slice(&[x as f32/(n-1) as f32,0.0,0.0,1.0]);}
 // t*from_offset = half texel: t=0.5, from_offset=1/n; B is alpha-0 so straight roundtrip returns A values
 let f=render(&a,&b,None,0.5,&Operator::Slide{from_offset:[1.0/n as f64,0.0],to_offset:[0.0;2]},Edge::Clamp).unwrap();
 for x in 1..n-1{let exp=(x as f64+0.5)/(n-1) as f64;assert!((f.d[x*4] as f64-exp).abs()<1e-5,"subpixel seam at x={x}: {} vs {exp}",f.d[x*4]);}
}
// No neighbor bleed with Transparent: integer 1px shift, vacated column gets zero A contribution, interior gets exactly the neighbor.
#[test]fn slide_transparent_no_bleed_at_vacated_edge(){
 let n=4usize;let mut a=Frame{w:n,h:1,d:vec![0.0;n*4]};let mut b=Frame{w:n,h:1,d:vec![0.0;n*4]};
 for x in 0..n{a.d[x*4..x*4+4].copy_from_slice(&[(x+1) as f32,0.0,0.0,1.0]);}
 // t*from_offset = one full texel: t=0.5, from_offset=2/n; B is all-zero so straight output is the A sample
 let f=render(&a,&b,None,0.5,&Operator::Slide{from_offset:[2.0/n as f64,0.0],to_offset:[0.0;2]},Edge::Transparent).unwrap();
 // pixel x samples A at x+1: p0<-A1=2, p1<-A2=3, p2<-A3=4, p3<-outside=0
 for (x,e) in [(0,2.0),(1,3.0),(2,4.0)]{assert!((f.d[x*4] as f64-e).abs()<1e-5,"bleed at x={x}: {}",f.d[x*4]);}
 assert!(f.d[12..16].iter().all(|v|v.abs()<1e-6),"vacated edge not zero: {:?}",&f.d[12..16]);
}
// Repeat with fractional offset: wrap blends across the seam exactly (bilinear of x=w-1 and x=0).
#[test]fn slide_repeat_wrap_blend_exact(){
 let n=4usize;let mut a=Frame{w:n,h:1,d:vec![0.0;n*4]};let mut b=Frame{w:n,h:1,d:vec![0.0;n*4]};
 for x in 0..n{a.d[x*4..x*4+4].copy_from_slice(&[(x+1) as f32,0.0,0.0,1.0]);}
 // half-texel shift at t=0.5 (from_offset=1/n); texel values [1,2,3,4]; wrap blends texel3 with texel0
 let f=render(&a,&b,None,0.5,&Operator::Slide{from_offset:[1.0/n as f64,0.0],to_offset:[0.0;2]},Edge::Repeat).unwrap();
 for (x,e) in [(0,1.5),(1,2.5),(2,3.5),(3,2.5)]{assert!((f.d[x*4]as f64-e).abs()<1e-5,"wrap blend x={x}: {} vs {e}",f.d[x*4]);}
}
// Per-policy 1080p slide renders (Repeat and Transparent; Clamp covered by the existing harness).
#[test]#[ignore="per-policy 1080p slide render gate"]
fn render_1080p_slide_policies(){
 use std::{io::Write,path::PathBuf};
 let dir=PathBuf::from(std::env::var("KIMO_TRANSITION_FRAMES").expect("set output directory"));std::fs::create_dir_all(&dir).unwrap();
 let (w,h)=(1920usize,1080usize);let mut a=Frame{w,h,d:vec![0.0;w*h*4]};let mut b=Frame{w,h,d:vec![0.0;w*h*4]};
 for y in 0..h{for x in 0..w{let i=(y*w+x)*4;let check=if (x/32+y/32)%2==0{0.15}else{0.85};a.d[i..i+4].copy_from_slice(&[check,0.2,x as f32/w as f32,if x<w/2{0.3}else{1.0}]);b.d[i..i+4].copy_from_slice(&[0.2,check,y as f32/h as f32,if y<h/2{0.5}else{1.0}]);}}
 for (name,edge) in [("slide-repeat",Edge::Repeat),("slide-transparent",Edge::Transparent)]{for step in 0..=8{
  let f=render(&a,&b,None,step as f64/8.0,&Operator::Slide{from_offset:[1.0,0.0],to_offset:[-1.0,0.0]},edge).unwrap();let mut file=std::fs::File::create(dir.join(format!("{name}-{step:02}.ppm"))).unwrap();write!(file,"P6\n{w} {h}\n255\n").unwrap();let mut rgb=Vec::with_capacity(w*h*3);
  for (i,p) in f.d.chunks_exact(4).enumerate(){let background=if (i%w/16+i/w/16)%2==0{0.2}else{0.7};for v in p.iter().take(3){rgb.push(((v*p[3]+background*(1.0-p[3])).clamp(0.0,1.0)*255.0+0.5) as u8);}}file.write_all(&rgb).unwrap();
 }}
}
