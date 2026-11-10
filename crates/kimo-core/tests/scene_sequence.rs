//! CP4 sequence verification (library level): sample keyframe tracks at several times and
//! render through the unchanged CPU executor. Motion must actually change pixels (frames
//! pairwise differ, translated object's color centroid crosses the frame), sampling must be
//! deterministic (same time -> bit-identical frame), an empty animation must leave the scene
//! untouched, and a shared contour cache across frames pays ONE polygonization for a static
//! SDF source. The tool path (render_scene3d_sequence) is verified through the kimo binary.
#![cfg(feature="surface-contour")]
use kimo_core::threed::{scene::{self,Scene,Source,Node,Cache},animate::{self,Animation,NodeAnim,CameraAnim,Keys},primitives::{generate,Primitive},vertex_stream::Stream,sdf_graph::{Field,Graph},contour_wrapped::Config,texture_material::Surface,lighting_stage::Light};
use kimo_core::camera_projection::Camera;

fn surf(rgb:[f64;3])->Surface{Surface{albedo:rgb,metallic:0.,roughness:0.5,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:false}}
fn t(x:f64,y:f64,z:f64)->[[f64;4];4]{[[1.,0.,0.,x],[0.,1.,0.,y],[0.,0.,1.,z],[0.,0.,0.,1.]]}
fn sphere(r:f64)->Stream{let g=generate(&Primitive::Sphere{radius:r,longitude_slices:20,latitude_slices:10}).unwrap();Stream{vertices:g.vertices,indices:g.indices}}

fn scene_desc()->Scene{Scene{
 sources:vec![Source::Primitive{primitive:Primitive::Sphere{radius:0.9,longitude_slices:20,latitude_slices:10}},Source::SdfContour{graph:Graph{nodes:vec![Field::Torus{center:[0.;3],major_radius:1.,minor_radius:0.4}],output:0},config:Config{bounds:[[-1.6;3],[1.6;3]],resolution:20,normal_epsilon:0.001}},Source::Stream{stream:sphere(0.7)}],
 nodes:vec![Node{source:0,world:t(0.,0.,3.),material:0},Node{source:1,world:t(0.,0.,4.5),material:1},Node{source:2,world:t(0.4,-0.4,3.6),material:2}],
 materials:vec![surf([0.9,0.15,0.15]),surf([0.15,0.25,0.9]),surf([0.15,0.85,0.25])],
 lights:vec![Light::Directional{travel_direction:[0.2,0.3,1.],radiance:[2.;3]}],
 camera:Camera{eye:[0.,0.,0.],pan:0.,tilt:0.,roll:0.,sensor_height:24.,focal_length:35.,near:0.5}}}

fn animation()->Animation{Animation{
 nodes:vec![
  NodeAnim{node:0,translation:Some(Keys{times:vec![0.,2.],values:vec![[-1.2,0.,3.],[1.2,0.,3.]]}),..Default::default()},
  NodeAnim{node:1,translation:Some(Keys{times:vec![0.],values:vec![[0.,0.,4.5]]}),rotation:Some(Keys{times:vec![0.,2.],values:vec![[1.,0.,0.,0.],[0.,0.,1.,0.]]}),..Default::default()}],
 camera:Some(CameraAnim{pan:Some(Keys{times:vec![0.,2.],values:vec![-2.,2.]}),..Default::default()}),
 lights:vec![]}}

fn centroid_red(frame:&[f32],w:usize,h:usize)->f64{let(mut sx,mut n)=(0usize,0usize);for y in 0..h{for x in 0..w{let i=(y*w+x)*4;if frame[i]>frame[i+1]&&frame[i]>frame[i+2]&&frame[i]>0.15{sx+=x;n+=1;}}}if n==0{return f64::NAN;}sx as f64/n as f64}

#[test]fn animated_sequence_moves_pixels_deterministically(){
 let s=scene_desc();let a=animation();let(w,h)=(96,54);
 let mut cache=Cache::new();let mut frames=Vec::new();let(mut poly,mut hits)=(0,0);
 for t in [0.,1.,2.]{
  let sampled=animate::sample(&s,&a,t).unwrap();
  let c=scene::compile(&sampled,&mut cache).unwrap();poly+=c.stats.contour_polygonized;hits+=c.stats.contour_cache_hits;
  frames.push(scene::render_cpu(&c,w,h).unwrap());}
 assert_eq!((poly,hits),(1,2),"static SDF source: ONE polygonization across 3 frames, 2 cache hits");
 for f in &frames{assert!(f.covered>500,"frame covers pixels: {}",f.covered);}
 assert!(frames[0].frame.d!=frames[1].frame.d&&frames[1].frame.d!=frames[2].frame.d,"frames pairwise differ (motion)");
 if let Ok(dir)=std::env::var("KIMO_SEQ_FRAMES"){for(k,f)in frames.iter().enumerate(){
  let mut ch=std::process::Command::new("ffmpeg").args(["-v","error","-y","-f","rawvideo","-pix_fmt","rgba","-s",&format!("{w}x{h}"),"-i","-","-frames:v","1",&format!("{dir}/seq_{k}.png")]).stdin(std::process::Stdio::piped()).spawn().unwrap();
  std::io::Write::write_all(&mut ch.stdin.take().unwrap(),&kimo_core::render::to_rgba8(&f.frame)).unwrap();assert!(ch.wait().unwrap().success());}}
 let(c0,c2)=(centroid_red(&frames[0].frame.d,w,h),centroid_red(&frames[2].frame.d,w,h));
 assert!(c0<w as f64/2.&&c2>w as f64/2.,"red sphere crosses the frame: {c0:.1} -> {c2:.1}");
 // determinism: same sample time -> bit-identical frame
 let again=scene::render_cpu(&scene::compile(&animate::sample(&s,&a,1.).unwrap(),&mut Cache::new()).unwrap(),w,h).unwrap();
 assert_eq!(again.frame.d,frames[1].frame.d,"t=1 resample bit-identical");
 println!("SCENE-SEQ: 3 frames, centroid_red {c0:.1}->{c2:.1}, contour 1 poly + 2 hits, deterministic");
}

#[test]fn empty_animation_leaves_scene_untouched(){
 let s=scene_desc();let out=animate::sample(&s,&Animation::default(),1.5).unwrap();
 for(a,b)in s.nodes.iter().zip(&out.nodes){assert_eq!(a.world,b.world,"node worlds unchanged");}
 assert_eq!(out.camera.pan,s.camera.pan);
 println!("SCENE-SEQ empty: identity animation, no drift");
}

#[test]fn animated_world_replaces_static_world_documented(){
 // a node WITH tracks ignores its static world (replaced by T*R*S from tracks, defaults identity)
 let mut s=scene_desc();s.nodes[0].world=t(99.,0.,3.);
 let a=Animation{nodes:vec![NodeAnim{node:0,translation:Some(Keys{times:vec![0.],values:vec![[0.,0.,3.]]}),..Default::default()}],..Default::default()};
 let out=animate::sample(&s,&a,0.).unwrap();
 assert_eq!(out.nodes[0].world,t(0.,0.,3.),"track pose REPLACES the static world, never composes the 99 offset");
}
