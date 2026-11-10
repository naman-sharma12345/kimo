//! Octree-mesh CP3 frame matrix (review evidence, env-gated dumps via KIMO_SDF_FRAMES):
//! sphere/torus/crease-union under directional-only, point-only, and combined lighting,
//! metallic/roughness variants, and a crease closeup + a grazing-angle crease view.
//! Lighting reality (honest): single forward ACES lit pass, direct light only - NO shadow
//! pass, NO AO pass, NO environment map (metallic reads dark outside specular highlights).
//! The test itself asserts closed+manifold censuses + frame coverage so it is meaningful
//! without the env var; dumps land in /tmp/pm3-frames/cp3/.
#![cfg(feature="surface-contour")]
use kimo_core::threed::{sdf_graph::{Field,Graph},octree::{self,OctreeConfig},lit_raster,pbr::Material,lighting_stage::Light};
use kimo_core::camera_projection::Camera;

fn cfg()->OctreeConfig{OctreeConfig{bounds:[[-2.2;3],[2.2;3]],min_depth:2,max_depth:8,error_tolerance:0.02,normal_angle_degrees:20.,normal_epsilon:0.001}}
fn union_scene()->Graph{Graph{nodes:vec![Field::Sphere{center:[0.35,0.,0.],radius:0.8},Field::Box{center:[-0.35,0.,0.],half_size:[0.6;3]},Field::SmoothUnion{a:0,b:1,k:0.25}],output:2}}
fn camera()->Camera{Camera{eye:[4.8,3.8,-6.8],pan:324.79,tilt:24.55,roll:0.,sensor_height:24.,focal_length:35.,near:0.1}}
fn dir_light()->Light{Light::Directional{travel_direction:[-0.3,-0.5,1.],radiance:[2.5;3]}}
fn point_light()->Light{Light::Point{position:[2.,3.,-4.],intensity:[80.;3],min_distance:0.01}}
fn dump(frame:&kimo_core::effects::Frame,w:usize,h:usize,path:&str){
 let rgba=kimo_core::render::to_rgba8_tpdf(frame);
 let mut ch=std::process::Command::new("ffmpeg").args(["-v","error","-y","-f","rawvideo","-pix_fmt","rgba","-s",&format!("{w}x{h}"),"-i","-","-frames:v","1",path]).stdin(std::process::Stdio::piped()).spawn().unwrap();
 std::io::Write::write_all(&mut ch.stdin.take().unwrap(),&rgba).unwrap();assert!(ch.wait().unwrap().success());
 println!("CP3-FRAME dumped {path}");}

#[test]fn cp3_lighting_material_and_crease_closeup_frames(){
 let scenes=[("sphere",Graph{nodes:vec![Field::Sphere{center:[0.;3],radius:1.}],output:0}),
             ("torus",Graph{nodes:vec![Field::Torus{center:[0.;3],major_radius:1.,minor_radius:0.4}],output:0}),
             ("crease",union_scene())];
 let dump_env=std::env::var("KIMO_SDF_FRAMES").is_ok();
 if dump_env{std::fs::create_dir_all("/tmp/pm3-frames/cp3").unwrap();}
 let base_mat=Material{albedo:[0.74,0.68,0.6],metallic:0.,roughness:0.55};
 for(name,g)in &scenes{
  let m=octree::extract_mesh(g,&cfg()).unwrap();
  assert_eq!((m.boundary_edges,m.nonmanifold_edges),(0,0),"{name} closed manifold crack-free");
  for(lname,lights)in[("dir",vec![dir_light()]),("point",vec![point_light()]),("both",vec![dir_light(),point_light()])]{
   let out=lit_raster::render(&m.stream,&camera(),320,180,&base_mat,&lights).unwrap();
   assert!(out.covered>2000,"{name}/{lname} covered {}",out.covered);
   if dump_env{let out=lit_raster::render(&m.stream,&camera(),640,360,&base_mat,&lights).unwrap();dump(&out.frame,640,360,&format!("/tmp/pm3-frames/cp3/{name}_{lname}.png"));}}
  if dump_env{
   for(mname,mat)in[("metal",Material{albedo:[0.8,0.75,0.7],metallic:1.,roughness:0.25}),("rough",Material{albedo:[0.6,0.65,0.75],metallic:0.,roughness:0.95})]{
    let out=lit_raster::render(&m.stream,&camera(),640,360,&mat,&[dir_light(),point_light()]).unwrap();dump(&out.frame,640,360,&format!("/tmp/pm3-frames/cp3/{name}_{mname}.png"));}}}
 // crease closeup (same view direction, near the blend) + grazing-angle view of the crease line
 let m=octree::extract_mesh(&union_scene(),&cfg()).unwrap();
 let close=Camera{eye:[2.36,1.87,-3.34],pan:324.79,tilt:24.55,roll:0.,sensor_height:24.,focal_length:55.,near:0.05};
 let out=lit_raster::render(&m.stream,&close,320,180,&base_mat,&[dir_light(),point_light()]).unwrap();
 assert!(out.covered>2000,"closeup covered {}",out.covered);
 let grazing=Camera{eye:[4.5,0.4,-6.4],pan:324.79,tilt:2.9,roll:0.,sensor_height:24.,focal_length:50.,near:0.05};
 let out2=lit_raster::render(&m.stream,&grazing,320,180,&base_mat,&[dir_light(),point_light()]).unwrap();
 assert!(out2.covered>1000,"grazing covered {}",out2.covered);
 if dump_env{
  let out=lit_raster::render(&m.stream,&close,640,360,&base_mat,&[dir_light(),point_light()]).unwrap();dump(&out.frame,640,360,"/tmp/pm3-frames/cp3/crease_closeup.png");
  let out=lit_raster::render(&m.stream,&grazing,640,360,&base_mat,&[dir_light(),point_light()]).unwrap();dump(&out.frame,640,360,"/tmp/pm3-frames/cp3/crease_grazing.png");}
}
