use super::kit::*;
#[cfg(feature="flat-sah")]fn read<T:serde::de::DeserializeOwned>(a:&Value,k:&str)->Result<T,String>{serde_json::from_value(a.get(k).ok_or_else(||format!("{k} required"))?.clone()).map_err(|e|e.to_string())}
pub fn commands()->Vec<Command>{vec![Command{name:"query_flat_sah",description:"Feature-gated MIT bvh serial binned-SAH builder + contiguous FlatBvh escape-index traversal over validated interleaved stream. Raw inspect/ray/AABB modes, no presets. Build budget cap100k triangles, no measured speed or1GB peak claim.",mutates:false,schema:obj(vec![("stream",json!({"type":"object"})),("op",s("inspect|ray|aabb")),("ray",json!({"type":"object"})),("bounds",json!({"type":"array"}))],&["stream","op"]),run:|_,a|{
#[cfg(feature="flat-sah")]{let index=crate::threed::flat_sah::Index::build(&read(a,"stream")?)?;match str_arg(a,"op")?{"inspect"=>Ok(index.inspect()),"ray"=>Ok(json!({"hit":index.raycast(&read(a,"ray")?,false)?})),"aabb"=>Ok(json!({"candidate_triangles":index.cull_aabb(read(a,"bounds")?)?})),_=>Err("op inspect|ray|aabb".into())}}
#[cfg(not(feature="flat-sah"))]{let _=a;Err("flat-sah feature not compiled".into())}
}}]}
