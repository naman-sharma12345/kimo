//! Octree-mesh CP2 render fixture: the pinned near-tangent crease scene (sphere+box
//! smooth union, the res-48/64 uniform-DC artifact benchmark) meshed adaptively and
//! rasterized through lit_raster (ACES PBR) for visual crack/flipped-normal inspection.
//! Deterministic FNV-1a hash at 320x180; env-gated 640x360 PNG dumps (KIMO_SDF_FRAMES).
//! Topology census asserted closed+manifold (crack-free across LOD boundaries).
#![cfg(feature="surface-contour")]
use kimo_core::threed::{sdf_graph::{Field,Graph},octree::{self,OctreeConfig},lit_raster,pbr::Material,lighting_stage::Light};
use kimo_core::camera_projection::Camera;

fn fnv(bytes:&[u8])->u64{let mut h=0xcbf29ce484222325u64;for b in bytes{h^=*b as u64;h=h.wrapping_mul(0x100000001b3);}h}

fn union_scene()->Graph{Graph{nodes:vec![
 Field::Sphere{center:[0.35,0.,0.],radius:0.8},
 Field::Box{center:[-0.35,0.,0.],half_size:[0.6;3]},
 Field::SmoothUnion{a:0,b:1,k:0.25}],output:2}}

fn cfg()->OctreeConfig{OctreeConfig{bounds:[[-2.2;3],[2.2;3]],min_depth:2,max_depth:8,error_tolerance:0.02,normal_angle_degrees:20.,normal_epsilon:0.001}}

fn camera()->Camera{Camera{eye:[4.8,3.8,-6.8],pan:324.79,tilt:24.55,roll:0.,sensor_height:24.,focal_length:35.,near:0.1}}

fn mesh_frame(w:usize,h:usize)->(kimo_core::effects::Frame,usize,octree::AdaptiveMesh){
 let m=octree::extract_mesh(&union_scene(),&cfg()).unwrap();
 let material=Material{albedo:[0.74,0.68,0.6],metallic:0.,roughness:0.55};
 let lights=[Light::Directional{travel_direction:[-0.3,-0.5,1.],radiance:[2.5;3]}];
 let out=lit_raster::render(&m.stream,&camera(),w,h,&material,&lights).unwrap();
 (out.frame,out.covered,m)
}

fn hash_of(f:&kimo_core::effects::Frame)->u64{let mut raw=Vec::with_capacity(f.d.len()*4);for v in &f.d{raw.extend_from_slice(&v.to_le_bytes());}fnv(&raw)}

#[test]fn adaptive_sphere_torus_frames(){
 let scenes=[("sphere",Graph{nodes:vec![Field::Sphere{center:[0.;3],radius:1.}],output:0}),
             ("torus",Graph{nodes:vec![Field::Torus{center:[0.;3],major_radius:1.,minor_radius:0.4}],output:0})];
 for(name,g)in scenes{
  let m=octree::extract_mesh(&g,&cfg()).unwrap();
  assert_eq!((m.boundary_edges,m.nonmanifold_edges),(0,0),"{name} closed manifold crack-free");
  let material=Material{albedo:[0.74,0.68,0.6],metallic:0.,roughness:0.55};
  let lights=[Light::Directional{travel_direction:[-0.3,-0.5,1.],radiance:[2.5;3]},Light::Point{position:[2.,3.,-4.],intensity:[1.2;3],min_distance:0.01}];
  let out=lit_raster::render(&m.stream,&camera(),320,180,&material,&lights).unwrap();
  println!("ADCM-FRAME {name} 320x180 covered={} tris={} euler={}",out.covered,m.stream.indices.len(),m.euler_characteristic);
  assert!(out.covered>2000,"{name} mesh must cover frame area");
  if std::env::var("KIMO_SDF_FRAMES").is_ok(){
   std::fs::create_dir_all("/tmp/pm3-frames").unwrap();
   let out=lit_raster::render(&m.stream,&camera(),640,360,&material,&lights).unwrap();
   let rgba=kimo_core::render::to_rgba8_tpdf(&out.frame);
   let path=format!("/tmp/pm3-frames/oct2_{name}_adaptive.png");
   let mut ch=std::process::Command::new("ffmpeg").args(["-v","error","-y","-f","rawvideo","-pix_fmt","rgba","-s","640x360","-i","-","-frames:v","1",&path]).stdin(std::process::Stdio::piped()).spawn().unwrap();
   std::io::Write::write_all(&mut ch.stdin.take().unwrap(),&rgba).unwrap();assert!(ch.wait().unwrap().success());
   println!("ADCM-FRAME {name} 640x360 dumped {path}");
  }
 }
}

#[test]fn adaptive_crease_mesh_frame_topology_and_hash(){
 let(frame,covered,m)=mesh_frame(320,180);
 let hash=hash_of(&frame);
 println!("ADCM-FRAME union 320x180 covered={covered} hash={hash:016x} tris={} folds-artifact-see-lib-test bnd={} euler={}",m.stream.indices.len(),m.boundary_edges,m.euler_characteristic);
 assert_eq!((m.boundary_edges,m.nonmanifold_edges,m.euler_characteristic),(0,0,2),"closed manifold crack-free");
 assert!(covered>2000,"mesh must cover frame area: {covered}");
 if std::env::var("KIMO_SDF_FRAMES").is_ok(){
  std::fs::create_dir_all("/tmp/pm3-frames").unwrap();
  let m=octree::extract_mesh(&union_scene(),&cfg()).unwrap();
  let material=Material{albedo:[0.74,0.68,0.6],metallic:0.,roughness:0.55};
  let lights=[Light::Directional{travel_direction:[-0.3,-0.5,1.],radiance:[2.5;3]},Light::Point{position:[2.,3.,-4.],intensity:[1.2;3],min_distance:0.01}];
  let out=lit_raster::render(&m.stream,&camera(),640,360,&material,&lights).unwrap();
  let(frame,covered)=(out.frame,out.covered);
  let rgba=kimo_core::render::to_rgba8_tpdf(&frame);
  let mut ch=std::process::Command::new("ffmpeg").args(["-v","error","-y","-f","rawvideo","-pix_fmt","rgba","-s","640x360","-i","-","-frames:v","1","/tmp/pm3-frames/oct2_crease_adaptive.png"]).stdin(std::process::Stdio::piped()).spawn().unwrap();
  std::io::Write::write_all(&mut ch.stdin.take().unwrap(),&rgba).unwrap();assert!(ch.wait().unwrap().success());
  println!("ADCM-FRAME union 640x360 covered={covered} tris={} dumped /tmp/pm3-frames/oct2_crease_adaptive.png",m.stream.indices.len());
 }
}
