//! Wavefront OBJ export of a validated vertex stream: positions/normals/UVs only.
//! f32 shortest round-trip decimals (Rust Display), single-indexed 1-based faces.
//! No materials, tangents, or groups (OBJ has no tangent field). Zero dependencies.
use super::vertex_stream::Stream;
pub fn to_obj(s:&Stream)->Result<String,String>{s.validate()?;crate::guard::reserve(s.vertices.len().saturating_mul(96).saturating_add(s.indices.len().saturating_mul(40)));
 let mut o=String::with_capacity(128);o.push_str("# kimo vertex stream OBJ export: f32 shortest round-trip decimals; single-indexed v/vn/vt; no materials/tangents/groups\n");
 for v in &s.vertices{o.push_str(&format!("v {} {} {}\n",v.position[0],v.position[1],v.position[2]));}
 for v in &s.vertices{o.push_str(&format!("vn {} {} {}\n",v.normal[0],v.normal[1],v.normal[2]));}
 for v in &s.vertices{o.push_str(&format!("vt {} {}\n",v.uv[0],v.uv[1]));}
 for t in &s.indices{o.push_str(&format!("f {0}/{0}/{0} {1}/{1}/{1} {2}/{2}/{2}\n",t[0]+1,t[1]+1,t[2]+1));}
 Ok(o)}
#[cfg(test)]mod tests{use super::*;use crate::threed::primitives::{generate,Primitive};
 #[test]fn sphere_obj_layout_and_roundtrip_decimals(){let g=generate(&Primitive::Sphere{radius:1.,longitude_slices:8,latitude_slices:4}).unwrap();let s=Stream{vertices:g.vertices,indices:g.indices};let o=to_obj(&s).unwrap();let nv=s.vertices.len();assert_eq!(o.matches("\nv ").count()+usize::from(o.starts_with("v ")),nv);assert_eq!(o.matches("\nvn ").count(),nv);assert_eq!(o.matches("\nvt ").count(),nv);assert_eq!(o.matches("\nf ").count(),s.indices.len());for line in o.lines().filter(|l|l.starts_with("f ")){let ids:Vec<&str>=line[2..].split_whitespace().collect();assert_eq!(ids.len(),3);for id in ids{let trip:Vec<&str>=id.split('/').collect();assert!(trip.len()==3&&trip[0]==trip[1]&&trip[1]==trip[2],"single-indexed face: {id}");let n:usize=trip[0].parse().unwrap();assert!((1..=nv).contains(&n));}}
  // shortest-decimal round trip: every v/vn/vt scalar parses back to the exact f32 bits
  let mut scalars=0;for line in o.lines(){let t=line.split_whitespace().collect::<Vec<_>>();if t.first().is_some_and(|k|matches!(*k,"v"|"vn"|"vt")){for x in &t[1..]{let f:f32=x.parse().unwrap();assert_eq!(f.to_string(),*x,"not a shortest round-trip decimal: {x}");scalars+=1;}}}assert!(scalars>=nv*8);}
 #[test]fn invalid_stream_rejected(){let s=Stream{vertices:vec![],indices:vec![]};assert!(to_obj(&s).is_err(),"empty stream must be a typed error, not an empty file");}
 #[cfg(feature="mesh-import")]
 #[test]fn tobj_ingest_roundtrip_mesh_identity(){let g=generate(&Primitive::Sphere{radius:1.5,longitude_slices:12,latitude_slices:6}).unwrap();let s=Stream{vertices:g.vertices,indices:g.indices};let text=to_obj(&s).unwrap();let back=crate::threed::ingest::obj(&text,true).unwrap().streams.into_iter().next().unwrap();
  let key=|v:&crate::threed::primitives::Vertex|{let mut k=Vec::new();for x in v.position.iter().chain(v.normal.iter()).chain(v.uv.iter()){k.extend(x.to_le_bytes());}k};
  let mut a:Vec<_>=s.vertices.iter().map(|v|key(v)).collect();let mut b:Vec<_>=back.vertices.iter().map(|v|key(v)).collect();a.sort();b.sort();
  if a!=b{let mut a2=a.clone();a2.dedup();let mut b2=b.clone();b2.dedup();println!("ROUNDTRIP-DIAG raw {} vs {}, dedup {} vs {}, sets_equal={}",a.len(),b.len(),a2.len(),b2.len(),a2==b2);let miss=a2.iter().filter(|k|!b2.contains(k)).count();let extra=b2.iter().filter(|k|!a2.contains(k)).count();println!("ROUNDTRIP-DIAG missing_in_back={} extra_in_back={}",miss,extra);}
  let mut a2=a;a2.dedup();let mut b2=b;b2.dedup();assert_eq!(a2,b2,"position/normal/uv vertex SETS must round-trip bit-exact (tobj dedups identical attribute triples)");
  let tri=|st:&Stream,i:usize|{let mut t=st.indices[i].map(|j|st.vertices[j as usize].position);t.sort_by(|x,y|x.partial_cmp(y).unwrap());t};
  let mut ta:Vec<_>=(0..s.indices.len()).map(|i|tri(&s,i)).collect();let mut tb:Vec<_>=(0..back.indices.len()).map(|i|tri(&back,i)).collect();ta.sort_by(|x,y|x.partial_cmp(y).unwrap());tb.sort_by(|x,y|x.partial_cmp(y).unwrap());assert_eq!(ta,tb,"triangle sets must round-trip exactly");}
}
