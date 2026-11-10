//! GLB export verification (CP4): independent minimal GLB/glTF parser (second code path
//! as oracle, no gltf crate), full 48-byte vertex bit-exact round-trip on a primitive
//! AND on a contour mesh (tangents preserved - the advantage over OBJ).
use kimo_core::threed::{gltf_export,primitives::{generate,Primitive,Vertex},vertex_stream::Stream,sdf_graph::{Field,Graph},contour_wrapped::{self,Config}};

struct Parsed{vertices:Vec<Vertex>,indices:Vec<[u32;3]>}

// Minimal strict GLB reader for exactly what to_glb emits.
fn parse(g:&[u8])->Parsed{
 assert_eq!(&g[0..4],b"glTF","magic");assert_eq!(u32::from_le_bytes(g[4..8].try_into().unwrap()),2,"version");
 assert_eq!(u32::from_le_bytes(g[8..12].try_into().unwrap()) as usize,g.len(),"total length");
 let jl=u32::from_le_bytes(g[12..16].try_into().unwrap()) as usize;assert_eq!(&g[16..20],&[0x4A,0x53,0x4F,0x4E],"JSON chunk id");
 let j:serde_json::Value=serde_json::from_slice(&g[20..20+jl]).unwrap();
 let b0=20+jl;let bl=u32::from_le_bytes(g[b0..b0+4].try_into().unwrap()) as usize;assert_eq!(&g[b0+4..b0+8],&[0x42,0x49,0x4E,0x00],"BIN chunk id");
 let bin=&g[b0+8..b0+8+bl];
 let f32s=|bv:usize|->&[u8]{let v=&j["bufferViews"][bv];let o=v["byteOffset"].as_u64().unwrap() as usize;let l=v["byteLength"].as_u64().unwrap() as usize;&bin[o..o+l]};
 let n=j["accessors"][0]["count"].as_u64().unwrap() as usize;
 let read3=|bv:usize,i:usize|->[f32;3]{let b=f32s(bv);let o=i*12;[f32::from_le_bytes(b[o..o+4].try_into().unwrap()),f32::from_le_bytes(b[o+4..o+8].try_into().unwrap()),f32::from_le_bytes(b[o+8..o+12].try_into().unwrap())]};
 let read2=|bv:usize,i:usize|->[f32;2]{let b=f32s(bv);let o=i*8;[f32::from_le_bytes(b[o..o+4].try_into().unwrap()),f32::from_le_bytes(b[o+4..o+8].try_into().unwrap())]};
 let read4=|bv:usize,i:usize|->[f32;4]{let b=f32s(bv);let o=i*16;[f32::from_le_bytes(b[o..o+4].try_into().unwrap()),f32::from_le_bytes(b[o+4..o+8].try_into().unwrap()),f32::from_le_bytes(b[o+8..o+12].try_into().unwrap()),f32::from_le_bytes(b[o+12..o+16].try_into().unwrap())]};
 let attrs=j["meshes"][0]["primitives"][0]["attributes"].as_object().unwrap();
 let(p,nm,uv,tg)=(attrs["POSITION"].as_u64().unwrap() as usize,attrs["NORMAL"].as_u64().unwrap() as usize,attrs["TEXCOORD_0"].as_u64().unwrap() as usize,attrs["TANGENT"].as_u64().unwrap() as usize);
 assert_eq!((p,nm,uv,tg),(0,1,2,3));
 let vertices=(0..n).map(|i|Vertex{position:read3(0,i),normal:read3(1,i),uv:read2(2,i),tangent:read4(3,i)}).collect();
 let ib=f32s(4);let t=j["accessors"][4]["count"].as_u64().unwrap() as usize/3;
 let indices=(0..t).map(|i|{let o=i*12;[u32::from_le_bytes(ib[o..o+4].try_into().unwrap()),u32::from_le_bytes(ib[o+4..o+8].try_into().unwrap()),u32::from_le_bytes(ib[o+8..o+12].try_into().unwrap())]}).collect();
 Parsed{vertices,indices}
}

fn bit_exact(a:&Stream,b:&Parsed){
 assert_eq!(a.indices,b.indices,"indices bit-exact");
 assert_eq!(a.vertices.len(),b.vertices.len());
 for(x,y)in a.vertices.iter().zip(&b.vertices){assert_eq!((x.position,x.normal,x.uv,x.tangent),(y.position,y.normal,y.uv,y.tangent),"full 48-byte vertex bit-exact");}
}

#[test]fn sphere_glb_roundtrip_bit_exact(){
 let g=generate(&Primitive::Sphere{radius:1.5,longitude_slices:24,latitude_slices:12}).unwrap();
 let s=Stream{vertices:g.vertices,indices:g.indices};
 let glb=gltf_export::to_glb(&s).unwrap();
 let back=parse(&glb);
 bit_exact(&s,&back);
 println!("GLB-ROUNDTRIP sphere: {} verts {} tris {} bytes bit-exact",s.vertices.len(),s.indices.len(),glb.len());
}

#[test]fn contour_torus_glb_roundtrip_bit_exact_with_placeholder_uv(){
 let g=Graph{nodes:vec![Field::Torus{center:[0.;3],major_radius:1.,minor_radius:0.4}],output:0};
 let cfg=Config{bounds:[[-1.6;3],[1.6;3]],resolution:24,normal_epsilon:0.001};
 let m=contour_wrapped::polygonize(&g,&cfg).unwrap();
 let glb=gltf_export::to_glb(&m.stream).unwrap();
 let back=parse(&glb);
 bit_exact(&m.stream,&back);
 println!("GLB-ROUNDTRIP contour torus: {} verts {} tris {} bytes bit-exact (UV=(0,0) placeholder preserved)",m.stream.vertices.len(),m.stream.indices.len(),glb.len());
}

// Spec-minimal mode: placeholder-UV contour stream exports POSITION/NORMAL/indices only
// (strict glTF2 minimum). Independent parser verifies the reduced attribute set and that
// position/normal/indices stay bit-exact. Tangents are absent by design (re-derived on
// ingest - see the mesh-import lib test).
#[test]fn spec_minimal_contour_glb_structure_and_geometry_bit_exact(){
 let g=Graph{nodes:vec![Field::Torus{center:[0.;3],major_radius:1.,minor_radius:0.4}],output:0};
 let cfg=Config{bounds:[[-1.6;3],[1.6;3]],resolution:24,normal_epsilon:0.001};
 let m=contour_wrapped::polygonize(&g,&cfg).unwrap();
 let glb=gltf_export::to_glb_opts(&m.stream,true).unwrap();
 assert_eq!(&glb[0..4],b"glTF");
 let jl=u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
 let j:serde_json::Value=serde_json::from_slice(&glb[20..20+jl]).unwrap();
 let attrs=j["meshes"][0]["primitives"][0]["attributes"].as_object().unwrap();
 assert_eq!(attrs.len(),2,"spec-minimal writes POSITION+NORMAL only");
 let b0=20+jl;let bl=u32::from_le_bytes(glb[b0..b0+4].try_into().unwrap()) as usize;let bin=&glb[b0+8..b0+8+bl];
 let read3=|bv:usize,i:usize|->[f32;3]{let v=&j["bufferViews"][bv];let o=v["byteOffset"].as_u64().unwrap() as usize+i*12;[f32::from_le_bytes(bin[o..o+4].try_into().unwrap()),f32::from_le_bytes(bin[o+4..o+8].try_into().unwrap()),f32::from_le_bytes(bin[o+8..o+12].try_into().unwrap())]};
 let n=j["accessors"][0]["count"].as_u64().unwrap() as usize;
 assert_eq!(n,m.stream.vertices.len());
 for(i,v)in m.stream.vertices.iter().enumerate(){assert_eq!(read3(0,i),v.position,"position bit-exact");assert_eq!(read3(1,i),v.normal,"normal bit-exact");}
 let iv=j["meshes"][0]["primitives"][0]["indices"].as_u64().unwrap() as usize;
 let vo=j["bufferViews"][iv]["byteOffset"].as_u64().unwrap() as usize;
 for(t,tri)in m.stream.indices.iter().enumerate(){let o=vo+t*12;let got=[u32::from_le_bytes(bin[o..o+4].try_into().unwrap()),u32::from_le_bytes(bin[o+4..o+8].try_into().unwrap()),u32::from_le_bytes(bin[o+8..o+12].try_into().unwrap())];assert_eq!(&got,tri,"indices bit-exact");}
 println!("GLB-SPEC-MINIMAL contour torus: {} verts {} tris {} bytes, POSITION/NORMAL/indices only, bit-exact",n,m.stream.indices.len(),glb.len());
}
