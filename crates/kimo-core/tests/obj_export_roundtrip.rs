//! OBJ export verification (CP3): independent minimal strict parser (no tobj - second
//! code path as oracle), mesh-identity round-trip on a UV-bearing primitive AND on a
//! real contour mesh, and bit-identical lit_raster renders of original vs re-parsed.
use kimo_core::threed::{obj_export,primitives::{generate,Primitive,Vertex},vertex_stream::Stream,lit_raster,pbr::Material,lighting_stage::Light,sdf_graph::{Field,Graph},contour_wrapped::{self,Config}};
use kimo_core::camera_projection::Camera;

// Minimal strict reader for exactly what to_obj emits: v/vn/vt blocks + f i/i/i lines.
fn parse(text:&str)->Stream{
 let mut pos=Vec::new();let mut nrm=Vec::new();let mut uv=Vec::new();let mut idx=Vec::new();
 for line in text.lines(){
  if line.starts_with('#')||line.is_empty(){continue;}
  let t:Vec<&str>=line.split_whitespace().collect();
  match t[0]{
   "v"=>{assert_eq!(t.len(),4);pos.push([t[1].parse::<f32>().unwrap(),t[2].parse().unwrap(),t[3].parse().unwrap()]);}
   "vn"=>{assert_eq!(t.len(),4);nrm.push([t[1].parse::<f32>().unwrap(),t[2].parse().unwrap(),t[3].parse().unwrap()]);}
   "vt"=>{assert_eq!(t.len(),3);uv.push([t[1].parse::<f32>().unwrap(),t[2].parse().unwrap()]);}
   "f"=>{assert_eq!(t.len(),4);let tri:Vec<u32>=t[1..].iter().map(|c|{let ids:Vec<&str>=c.split('/').collect();assert_eq!(ids.len(),3);assert!(ids[0]==ids[1]&&ids[1]==ids[2]);ids[0].parse::<u32>().unwrap()-1}).collect();idx.push([tri[0],tri[1],tri[2]]);}
   other=>panic!("unexpected OBJ line: {other}"),
  }
 }
 assert_eq!(pos.len(),nrm.len());assert_eq!(pos.len(),uv.len());
 // tangents are not an OBJ field: rebuild a validate-safe orthonormal tangent per vertex
 let vertices=pos.into_iter().enumerate().map(|(i,position)|{let n=nrm[i];let ax=if n[0].abs()<0.8{[1.,0.,0.]}else{[0.,1.,0.]};let t=[n[1]*ax[2]-n[2]*ax[1],n[2]*ax[0]-n[0]*ax[2],n[0]*ax[1]-n[1]*ax[0]];let l=(t[0]*t[0]+t[1]*t[1]+t[2]*t[2]).sqrt();Vertex{position,normal:n,uv:uv[i],tangent:[t[0]/l,t[1]/l,t[2]/l,1.]}}).collect();
 let s=Stream{vertices,indices:idx};s.validate().unwrap();s
}

fn mesh_identity(a:&Stream,b:&Stream){
 assert_eq!(a.vertices.len(),b.vertices.len());assert_eq!(a.indices,b.indices,"index buffer must round-trip exactly (single-indexed, order preserved)");
 for(x,y)in a.vertices.iter().zip(&b.vertices){assert_eq!(x.position,y.position,"position bits");assert_eq!(x.normal,y.normal,"normal bits");assert_eq!(x.uv,y.uv,"uv bits");}
}

#[test]fn sphere_export_roundtrip_bit_exact_and_render_identical(){
 let g=generate(&Primitive::Sphere{radius:1.5,longitude_slices:24,latitude_slices:12}).unwrap();
 let s=Stream{vertices:g.vertices,indices:g.indices};
 let text=obj_export::to_obj(&s).unwrap();
 let back=parse(&text);
 mesh_identity(&s,&back);
 let cam=Camera{eye:[2.4,1.9,-3.4],pan:324.79,tilt:24.55,roll:0.,sensor_height:24.,focal_length:35.,near:0.1};
 let mat=Material{albedo:[0.6,0.3,0.15],metallic:0.3,roughness:0.35};
 let lights=[Light::Directional{travel_direction:[-0.3,-0.5,1.],radiance:[2.5;3]}];
 let fa=lit_raster::render(&s,&cam,320,180,&mat,&lights).unwrap();
 let fb=lit_raster::render(&back,&cam,320,180,&mat,&lights).unwrap();
 assert_eq!(fa.frame.d,fb.frame.d,"re-parsed OBJ must render bit-identical");
}

#[test]fn contour_torus_export_roundtrip_bit_exact(){
 let g=Graph{nodes:vec![Field::Torus{center:[0.;3],major_radius:1.,minor_radius:0.4}],output:0};
 let cfg=Config{bounds:[[-1.6;3],[1.6;3]],resolution:24,normal_epsilon:0.001};
 let m=contour_wrapped::polygonize(&g,&cfg).unwrap();
 let text=obj_export::to_obj(&m.stream).unwrap();
 let back=parse(&text);
 mesh_identity(&m.stream,&back);
 println!("OBJ-ROUNDTRIP contour torus: {} verts {} tris {} bytes, bit-exact",m.stream.vertices.len(),m.stream.indices.len(),text.len());
}
