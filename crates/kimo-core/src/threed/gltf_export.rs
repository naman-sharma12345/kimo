//! glTF 2.0 GLB export of a validated vertex stream. Zero new deps (serde_json only).
//! Two modes:
//! - full fidelity (default): POSITION/NORMAL/TEXCOORD_0/TANGENT + u32 indices in one
//!   binary chunk. Tangents preserved (glTF has the semantic), so the round-trip is
//!   bit-exact on every attribute - stronger than the OBJ path.
//! - spec_minimal: strict glTF 2.0 minimum - POSITION/NORMAL (required) + indices; the
//!   optional TEXCOORD_0/TANGENT are omitted when the stream carries the UV=(0,0)
//!   placeholder (contour streams). Real UVs keep all attributes: there is no reason to
//!   drop meaningful data. A placeholder-UV spec-minimal file re-ingests bit-exact on
//!   position/normal/uv/indices (UV fills back to (0,0)); tangents are re-derived to a
//!   valid orthonormal arbitrary frame - contract-equivalent for no-parameterization
//!   streams, NOT bit-identical. That is the tradeoff: bit-exact tangent bits vs a
//!   strictly minimal file with no meaningless attributes.
use super::vertex_stream::Stream;
pub fn to_glb(s:&Stream)->Result<Vec<u8>,String>{to_glb_opts(s,false)}
pub fn to_glb_opts(s:&Stream,spec_minimal:bool)->Result<Vec<u8>,String>{s.validate()?;let n=s.vertices.len();let t=s.indices.len();crate::guard::reserve(n.saturating_mul(48).saturating_add(t.saturating_mul(12)).saturating_add(4096));
 let placeholder_uv=s.vertices.iter().all(|v|v.uv==[0.,0.]);
 let write_optional=!(spec_minimal&&placeholder_uv);
 let mut bin=Vec::with_capacity(n*48+t*12);
 for v in &s.vertices{for x in v.position{bin.extend(x.to_le_bytes());}}
 for v in &s.vertices{for x in v.normal{bin.extend(x.to_le_bytes());}}
 if write_optional{for v in &s.vertices{for x in v.uv{bin.extend(x.to_le_bytes());}}
  for v in &s.vertices{for x in v.tangent{bin.extend(x.to_le_bytes());}}}
 for tri in &s.indices{for i in tri{bin.extend(i.to_le_bytes());}}
 let mut mn=[f32::INFINITY;3];let mut mx=[f32::NEG_INFINITY;3];for v in &s.vertices{for j in 0..3{mn[j]=mn[j].min(v.position[j]);mx[j]=mx[j].max(v.position[j]);}}
 let bin_len=bin.len();
 let idx_view=if write_optional{4}else{2};
 let idx_offset=if write_optional{n*48}else{n*24};
 let mut buffer_views=vec![
  serde_json::json!({"buffer":0,"byteOffset":0,"byteLength":n*12}),
  serde_json::json!({"buffer":0,"byteOffset":n*12,"byteLength":n*12})];
 let mut accessors=vec![
  serde_json::json!({"bufferView":0,"componentType":5126,"count":n,"type":"VEC3","min":mn,"max":mx}),
  serde_json::json!({"bufferView":1,"componentType":5126,"count":n,"type":"VEC3"})];
 let mut attributes=serde_json::json!({"POSITION":0,"NORMAL":1});
 if write_optional{buffer_views.push(serde_json::json!({"buffer":0,"byteOffset":n*24,"byteLength":n*8}));
  buffer_views.push(serde_json::json!({"buffer":0,"byteOffset":n*32,"byteLength":n*16}));
  accessors.push(serde_json::json!({"bufferView":2,"componentType":5126,"count":n,"type":"VEC2"}));
  accessors.push(serde_json::json!({"bufferView":3,"componentType":5126,"count":n,"type":"VEC4"}));
  attributes=serde_json::json!({"POSITION":0,"NORMAL":1,"TEXCOORD_0":2,"TANGENT":3});}
 buffer_views.push(serde_json::json!({"buffer":0,"byteOffset":idx_offset,"byteLength":t*12}));
 accessors.push(serde_json::json!({"bufferView":idx_view,"componentType":5125,"count":t*3,"type":"SCALAR"}));
 let json=serde_json::json!({
  "asset":{"version":"2.0","generator":"kimo vertex stream GLB export"},
  "scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],
  "meshes":[{"primitives":[{"attributes":attributes,"indices":idx_view,"mode":4}]}],
  "buffers":[{"byteLength":bin_len}],
  "bufferViews":buffer_views,
  "accessors":accessors});
 let mut jb=serde_json::to_vec(&json).map_err(|e|e.to_string())?;while jb.len()%4!=0{jb.push(0x20);}
 let mut bb=bin;while bb.len()%4!=0{bb.push(0);}
 let total=12usize+8+jb.len()+8+bb.len();
 let mut g=Vec::with_capacity(total);
 g.extend(0x46546C67u32.to_le_bytes());g.extend(2u32.to_le_bytes());g.extend((total as u32).to_le_bytes());
 g.extend((jb.len() as u32).to_le_bytes());g.extend(0x4E4F534Au32.to_le_bytes());g.extend(&jb);
 g.extend((bb.len() as u32).to_le_bytes());g.extend(0x004E4942u32.to_le_bytes());g.extend(&bb);
 Ok(g)}
#[cfg(test)]mod tests{use super::*;use crate::threed::primitives::{generate,Primitive};
 fn sphere()->Stream{let g=generate(&Primitive::Sphere{radius:1.5,longitude_slices:12,latitude_slices:6}).unwrap();Stream{vertices:g.vertices,indices:g.indices}}
 fn json_of(g:&[u8])->serde_json::Value{let jl=u32::from_le_bytes(g[12..16].try_into().unwrap()) as usize;serde_json::from_slice(&g[20..20+jl]).unwrap()}
 #[test]fn empty_stream_typed_error(){assert!(to_glb(&Stream{vertices:vec![],indices:vec![]}).is_err());assert!(to_glb_opts(&Stream{vertices:vec![],indices:vec![]},true).is_err());}
 #[test]fn glb_container_structure(){let g=to_glb(&sphere()).unwrap();assert_eq!(&g[0..4],&[0x67,0x6C,0x54,0x46],"magic glTF");assert_eq!(u32::from_le_bytes(g[4..8].try_into().unwrap()),2);assert_eq!(u32::from_le_bytes(g[8..12].try_into().unwrap()) as usize,g.len(),"total length field");let jl=u32::from_le_bytes(g[12..16].try_into().unwrap()) as usize;assert_eq!(u32::from_le_bytes(g[16..20].try_into().unwrap()),0x4E4F534A,"JSON chunk");assert_eq!(jl%4,0);let j:serde_json::Value=serde_json::from_slice(&g[20..20+jl]).unwrap();let bl0=20+jl;let bl=u32::from_le_bytes(g[bl0..bl0+4].try_into().unwrap()) as usize;assert_eq!(u32::from_le_bytes(g[bl0+4..bl0+8].try_into().unwrap()),0x004E4942,"BIN chunk");assert_eq!(bl%4,0);assert_eq!(bl0+8+bl,g.len());let bvs=j["bufferViews"].as_array().unwrap();for bv in bvs{assert_eq!(bv["byteOffset"].as_u64().unwrap()%4,0,"4-byte aligned bufferViews");}
  // accessor min/max match the actual positions (spec-required on POSITION)
  let s=sphere();let mut mn=[f32::INFINITY;3];let mut mx=[f32::NEG_INFINITY;3];for v in &s.vertices{for k in 0..3{mn[k]=mn[k].min(v.position[k]);mx[k]=mx[k].max(v.position[k]);}}
  let amin=j["accessors"][0]["min"].as_array().unwrap();let amax=j["accessors"][0]["max"].as_array().unwrap();for k in 0..3{assert_eq!(amin[k].as_f64().unwrap() as f32,mn[k]);assert_eq!(amax[k].as_f64().unwrap() as f32,mx[k]);}}
 #[test]fn spec_minimal_omits_optional_attributes_only_for_placeholder_uv(){
  // placeholder-UV stream (contour-shaped): spec_minimal drops TEXCOORD_0+TANGENT
  let mut s=sphere();for v in &mut s.vertices{v.uv=[0.,0.];}
  let j=json_of(&to_glb_opts(&s,true).unwrap());
  let attrs=j["meshes"][0]["primitives"][0]["attributes"].as_object().unwrap();
  assert_eq!(attrs.len(),2,"spec_minimal placeholder-UV: POSITION+NORMAL only, got {:?}",attrs);
  assert!(attrs.get("TEXCOORD_0").is_none()&&attrs.get("TANGENT").is_none());
  assert_eq!(j["accessors"].as_array().unwrap().len(),3,"pos/normal/indices accessors only");
  // default (full) mode keeps them even for placeholder UV
  let jf=json_of(&to_glb(&s).unwrap());
  assert!(jf["meshes"][0]["primitives"][0]["attributes"].get("TANGENT").is_some(),"full mode keeps TANGENT");
  // real-UV stream: spec_minimal is a no-op, meaningful data never dropped
  let jr=json_of(&to_glb_opts(&sphere(),true).unwrap());
  assert_eq!(jr["meshes"][0]["primitives"][0]["attributes"].as_object().unwrap().len(),4,"real UV keeps all attributes in spec_minimal");}
 #[cfg(all(feature="mesh-import",feature="surface-contour"))]
 #[test]fn ingest_roundtrip_bit_exact_all_attributes(){let s=sphere();let g=to_glb(&s).unwrap();let back=crate::threed::ingest::gltf(&g,&[],false).unwrap().streams.into_iter().next().unwrap();
  assert_eq!(s.indices,back.indices,"index buffer bit-exact");
  for(a,b)in s.vertices.iter().zip(&back.vertices){assert_eq!((a.position,a.normal,a.uv,a.tangent),(b.position,b.normal,b.uv,b.tangent),"full 48-byte vertex bit-exact");}
  // contour mesh: UV=(0,0) placeholder round-trips because glTF carries tangents (no derive)
  let graph=crate::threed::sdf_graph::Graph{nodes:vec![crate::threed::sdf_graph::Field::Torus{center:[0.;3],major_radius:1.,minor_radius:0.4}],output:0};
  let m=crate::threed::contour_wrapped::polygonize(&graph,&crate::threed::contour_wrapped::Config{bounds:[[-1.6;3],[1.6;3]],resolution:16,normal_epsilon:0.001}).unwrap();
  let g2=to_glb(&m.stream).unwrap();let back2=crate::threed::ingest::gltf(&g2,&[],false).unwrap().streams.into_iter().next().unwrap();
  assert_eq!(m.stream.indices,back2.indices);for(a,b)in m.stream.vertices.iter().zip(&back2.vertices){assert_eq!((a.position,a.normal,a.uv,a.tangent),(b.position,b.normal,b.uv,b.tangent),"contour vertex bit-exact");}}
 #[cfg(all(feature="mesh-import",feature="surface-contour"))]
 #[test]fn spec_minimal_contour_reingest_no_failure(){
  let graph=crate::threed::sdf_graph::Graph{nodes:vec![crate::threed::sdf_graph::Field::Torus{center:[0.;3],major_radius:1.,minor_radius:0.4}],output:0};
  let m=crate::threed::contour_wrapped::polygonize(&graph,&crate::threed::contour_wrapped::Config{bounds:[[-1.6;3],[1.6;3]],resolution:16,normal_epsilon:0.001}).unwrap();
  let g=to_glb_opts(&m.stream,true).unwrap();
  // no TEXCOORD_0/TANGENT in file; re-ingest must NOT fail (tangents derived)
  let back=crate::threed::ingest::gltf(&g,&[],true).unwrap().streams.into_iter().next().unwrap();
  assert_eq!(m.stream.indices,back.indices,"indices bit-exact");
  for(a,b)in m.stream.vertices.iter().zip(&back.vertices){assert_eq!((a.position,a.normal,a.uv),(b.position,b.normal,b.uv),"position/normal/uv bit-exact");}
  // tangents re-derived: valid orthonormal frame (contract-equivalent), bits not compared
  back.validate().unwrap();
  for v in &back.vertices{let d=v.normal[0]*v.tangent[0]+v.normal[1]*v.tangent[1]+v.normal[2]*v.tangent[2];assert!(d.abs()<1e-3,"derived tangent orthogonal to normal, dot={}",d);}
  // derive=false still a typed error when tangents absent (explicit opt-in required)
  assert!(crate::threed::ingest::gltf(&g,&[],false).is_err());}
}
