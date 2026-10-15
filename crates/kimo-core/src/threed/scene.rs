//! Scene descriptor + compile-to-draws (scene composition CP1). Original code, zero new deps.
//! A Scene binds named geometry sources (uniform SDF contour, adaptive octree contour, primitive,
//! raw stream, glTF scene) to
//! nodes carrying an explicit row-major affine world matrix and a material index, plus the
//! pass-level context (lights, camera-v2). compile() resolves each referenced source once,
//! then emits one Draw per (node, source instance): stream coordinates are SOURCE-local,
//! draw.world applies on top (matches the wgpu multi-draw contract; CPU executors bake via
//! node_ingest::transform). glTF scene sources arrive with their internal node worlds already
//! baked by instantiate(); the descriptor node world composes on top.
//! SDF contour sources are memoized by content hash (fnv1a-64 of the canonical JSON of
//! graph+config) in a caller-supplied Cache; octree contour sources share the same memo under a
//! discriminated key ("octree_contour" prefix), same FIFO bound, so repeated identical sources and repeated
//! compiles with the same cache pay for one polygonization. The MCP tool uses a fresh cache
//! per call (stateless); executors may carry a Cache across frames.
//! Empty is a result: a scene with no nodes compiles to zero draws without error.
//! Keyframed animation is deliberately NOT here (deferred to the Scene3D bridge, CP4).
//! Nodes reference sources by flat index only - node->node edges do not exist in the
//! descriptor, so cyclic graphs are inexpressible by construction (the timeline bridge
//! owns hierarchy). The contour cache is BOUNDED: at most CONTOUR_CACHE_CAP entries with
//! strict FIFO eviction (a cache hit does NOT refresh position; updating an existing key
//! keeps its slot). The MCP tool builds a fresh cache per call.
use serde::{Serialize,Deserialize};
use super::{mesh::Matrix,primitives::{generate,Primitive},vertex_stream::Stream,sdf_graph::Graph,contour_wrapped,lighting_stage,lighting_stage::Light,texture_material::Surface};
use crate::camera_projection::Camera;

#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
pub enum Source{Primitive{primitive:Primitive},SdfContour{graph:Graph,config:contour_wrapped::Config},#[cfg(feature="surface-contour")]OctreeContour{graph:Graph,config:super::octree::OctreeConfig},Stream{stream:Stream},GltfScene{bytes:Vec<u8>,buffers:Vec<Vec<u8>>,derive_tangents:bool,scene_index:usize}}

#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]
pub struct Node{pub source:usize,pub world:Matrix,pub material:usize}

#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]
pub struct Scene{pub sources:Vec<Source>,pub nodes:Vec<Node>,pub materials:Vec<Surface>,pub lights:Vec<Light>,pub camera:Camera}

/// One compiled draw call: source-local stream + world transform + surface. Same shape as
/// the wgpu multi-draw contract; CP3 converts field-by-field.
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]
pub struct Draw{pub stream:Stream,pub world:Matrix,pub surface:Surface}

#[derive(Clone,Debug,Serialize)]pub struct Stats{pub sources:usize,pub sources_resolved:usize,pub nodes:usize,pub draws:usize,pub vertices:usize,pub triangles:usize,pub contour_polygonized:usize,pub contour_cache_hits:usize}

#[derive(Clone,Debug,Serialize)]pub struct Compiled{pub draws:Vec<Draw>,pub lights:Vec<Light>,pub camera:Camera,pub stats:Stats}

fn fnv1a64(bytes:&[u8])->u64{let mut h=0xcbf29ce484222325u64;for b in bytes{h^=*b as u64;h=h.wrapping_mul(0x100000001b3);}h}

/// Max memoized SDF contours. Conservative: 64 x worst-case res-64 contour (~24MB stream)
/// is already ~1.5GB; realistic scenes are far smaller. Strict FIFO, no hit-refresh.
pub const CONTOUR_CACHE_CAP:usize=64;
/// Content-hash memo for SDF contour sources, bounded FIFO. Reusable across compiles
/// (executors); the MCP tool constructs a fresh one per call.
#[derive(Default)]pub struct Cache{contours:std::collections::HashMap<u64,Stream>,order:std::collections::VecDeque<u64>}
impl Cache{pub fn new()->Self{Self::default()} pub fn contour_entries(&self)->usize{self.contours.len()}
 fn contour_get(&self,key:u64)->Option<&Stream>{self.contours.get(&key)}
 fn contour_insert(&mut self,key:u64,stream:Stream){if self.contours.contains_key(&key){self.contours.insert(key,stream);return;}while self.order.len()>=CONTOUR_CACHE_CAP{if let Some(old)=self.order.pop_front(){self.contours.remove(&old);}}self.order.push_back(key);self.contours.insert(key,stream);}}

/// Surface -> base Material for the lit CPU path: texture slots are IGNORED (honest
/// label; textured execution is the textured-BVH/wgpu path, not this one).
fn surface_to_material(s:&Surface)->super::pbr::Material{super::pbr::Material{albedo:s.albedo,metallic:s.metallic,roughness:s.roughness}}
/// CPU multi-draw executor (CP2): bake each node's world into its stream (inverse-transpose
/// normals, reflection winding fix), then compose ALL draws over one shared depth buffer in
/// draw order via lit_raster::render_multi. Empty draw list renders an empty frame.
pub fn render_cpu(c:&Compiled,w:usize,h:usize)->Result<super::lit_raster::Output,String>{
 let mut baked=Vec::with_capacity(c.draws.len());
 for d in &c.draws{baked.push(super::node_ingest::transform(&d.stream,d.world)?);}
 let materials:Vec<super::pbr::Material>=c.draws.iter().map(|d|surface_to_material(&d.surface)).collect();
 let refs:Vec<(&Stream,&super::pbr::Material)>=baked.iter().zip(materials.iter()).map(|(s,m)|(s,m)).collect();
 super::lit_raster::render_multi(&refs,&c.camera,w,h,&c.lights)}
#[cfg(all(feature="mesh-import",feature="material-textures",feature="flat-sah"))]
/// CP3: wgpu multi-draw executor bridge. Draws convert field-by-field to the pbr_offscreen
/// contract (same shape by design): ONE shared Depth32Float, depth_compare Less - an exact
/// depth tie keeps the EARLIER draw, matching the CPU painter tiebreak. The draw cap here is
/// 64 (pbr_offscreen bound), tighter than the CPU executor's 256. Without the gpu3d feature
/// this returns the typed "native gpu3d not compiled" error; with allow_cpu it falls back to
/// the flat-SAH textured reference (gpu_rendered=false, reason reported). Output is UNORM
/// clamped linear, NOT the ACES display-referred frame of render_cpu: CPU-vs-wgpu agreement
/// is by tolerance metrics (pbr_offscreen::compare vs the flat-SAH reference; coverage IoU
/// vs render_cpu), never bit-exact.
pub fn render_wgpu(c:&Compiled,images:&[super::texture_material::Image],w:usize,h:usize,ambient:[f64;3],time:f64,software_only:bool,allow_cpu:bool)->Result<(crate::effects::Frame,super::pbr_offscreen::Status),String>{
 if c.draws.len()>64{return Err(format!("wgpu executor max64 draws per scene pass (got {}); the CPU executor allows 256 - split the scene or use render_cpu",c.draws.len()));}
 let draws:Vec<super::pbr_offscreen::Draw>=c.draws.iter().map(|d|super::pbr_offscreen::Draw{stream:d.stream.clone(),world:d.world,surface:d.surface.clone()}).collect();
 super::pbr_offscreen::render(&draws,images,&c.camera,w,h,&c.lights,ambient,time,software_only,allow_cpu)}
/// PL1b: render_cpu + one optional directional shadow map. The map's light is
/// lights[index]; its bounds/texel constants are shared verbatim with the wgpu and
/// raycast-reference paths. None shadows render EXACTLY the render_cpu frame.
pub fn render_cpu_shadowed(c:&Compiled,w:usize,h:usize,shadow:Option<(usize,&super::shadow::DirectionalShadowMap)>)->Result<super::lit_raster::Output,String>{
 let mut baked=Vec::with_capacity(c.draws.len());
 for d in &c.draws{baked.push(super::node_ingest::transform(&d.stream,d.world)?);}
 let materials:Vec<super::pbr::Material>=c.draws.iter().map(|d|surface_to_material(&d.surface)).collect();
 let refs:Vec<(&Stream,&super::pbr::Material)>=baked.iter().zip(materials.iter()).map(|(s,m)|(s,m)).collect();
 let mut slots:Vec<Option<super::shadow::ShadowMapRef>>=vec![None;c.lights.len()];
 if let Some((i,m))=shadow{if i>=slots.len(){return Err("shadowed light index in range required".into());}slots[i]=Some(super::shadow::ShadowMapRef::Directional(m));}
 super::lit_raster::render_shadowed(&refs,&c.camera,w,h,&c.lights,&slots)}
#[cfg(all(feature="mesh-import",feature="material-textures",feature="flat-sah"))]
/// PL1b: render_wgpu + one optional directional shadow map (depth pre-pass + PCF).
pub fn render_wgpu_shadowed(c:&Compiled,images:&[super::texture_material::Image],w:usize,h:usize,ambient:[f64;3],time:f64,software_only:bool,allow_cpu:bool,shadow:Option<(usize,&super::shadow::DirectionalShadowMap)>)->Result<(crate::effects::Frame,super::pbr_offscreen::Status),String>{
 if c.draws.len()>64{return Err(format!("wgpu executor max64 draws per scene pass (got {})",c.draws.len()));}
 let draws:Vec<super::pbr_offscreen::Draw>=c.draws.iter().map(|d|super::pbr_offscreen::Draw{stream:d.stream.clone(),world:d.world,surface:d.surface.clone()}).collect();
 super::pbr_offscreen::render_shadowed(&draws,images,&c.camera,w,h,&c.lights,ambient,time,software_only,allow_cpu,shadow)}
fn check_affine(m:&Matrix,what:&str)->Result<(),String>{if m[3]!=[0.,0.,0.,1.]||m.iter().flatten().any(|v|!v.is_finite()){return Err(format!("{what}: finite affine transform required (last row 0,0,0,1)"));}Ok(())}

/// Structural validation only - no geometry is built. Unreferenced sources are
/// shape-checked (graph validate / stream validate), not compiled.
pub fn validate(s:&Scene)->Result<(),String>{
 if s.sources.len()>64||s.nodes.len()>256||s.materials.len()>64{return Err("max64 sources/materials, max256 nodes".into());}
 lighting_stage::validate(&s.lights)?;s.camera.validate()?;
 for(i,src)in s.sources.iter().enumerate(){match src{
  Source::Primitive{..}|Source::GltfScene{..}=>{}
  Source::Stream{stream}=>stream.validate().map_err(|e|format!("source {i}: {e}"))?,
  Source::SdfContour{graph,..}=>graph.validate().map_err(|e|format!("source {i}: {e}"))?,
  #[cfg(feature="surface-contour")]
  Source::OctreeContour{graph,..}=>graph.validate().map_err(|e|format!("source {i}: {e}"))?,}}
 for(i,n)in s.nodes.iter().enumerate(){
  if n.source>=s.sources.len(){return Err(format!("node {i} references source {} of {}",n.source,s.sources.len()));}
  if n.material>=s.materials.len(){return Err(format!("node {i} references material {} of {}",n.material,s.materials.len()));}
  check_affine(&n.world,&format!("node {i}"))?;}
 Ok(())}

/// Resolve referenced sources and emit draws. Sources are resolved lazily (only when a
/// node references them) and memoized per index within the call.
pub fn compile(s:&Scene,cache:&mut Cache)->Result<Compiled,String>{
 validate(s)?;
 let mut resolved:Vec<Option<Vec<Stream>>>=(0..s.sources.len()).map(|_|None).collect();
 let mut polygonized=0usize;let mut hits=0usize;
 for n in &s.nodes{
  if resolved[n.source].is_none(){
   let streams=match &s.sources[n.source]{
    Source::Primitive{primitive}=>{let g=generate(primitive)?;vec![Stream{vertices:g.vertices,indices:g.indices}]}
    Source::Stream{stream}=>vec![stream.clone()],
    Source::SdfContour{graph,config}=>{
     let key=fnv1a64(&serde_json::to_vec(&(graph,config)).map_err(|e|e.to_string())?);
     if let Some(cached)=cache.contour_get(key){hits+=1;vec![cached.clone()]}
     else{
      #[cfg(feature="surface-contour")]
      {let m=contour_wrapped::polygonize(graph,config)?;polygonized+=1;cache.contour_insert(key,m.stream.clone());vec![m.stream]}
      #[cfg(not(feature="surface-contour"))]
      {let _=(graph,config);return Err("surface-contour feature not compiled".into());}}}
    #[cfg(feature="surface-contour")]
    Source::OctreeContour{graph,config}=>{
     let key=fnv1a64(&serde_json::to_vec(&("octree_contour",graph,config)).map_err(|e|e.to_string())?);
     if let Some(cached)=cache.contour_get(key){hits+=1;vec![cached.clone()]}
     else{let m=super::octree::extract_mesh(graph,config)?;polygonized+=1;cache.contour_insert(key,m.stream.clone());vec![m.stream]}}
    Source::GltfScene{bytes,buffers,derive_tangents,scene_index}=>{
     #[cfg(feature="mesh-import")]
     {super::node_ingest::instantiate(bytes,buffers,*derive_tangents,*scene_index)?.streams}
     #[cfg(not(feature="mesh-import"))]
     {let _=(bytes,buffers,derive_tangents,scene_index);return Err("mesh-import feature not compiled".into());}}};
   resolved[n.source]=Some(streams);}}
 let mut draws=Vec::new();let mut vertices=0usize;let mut triangles=0usize;
 for n in &s.nodes{let streams=resolved[n.source].as_ref().ok_or("internal: source resolution skipped")?;
  for st in streams{vertices=vertices.checked_add(st.vertices.len()).ok_or("aggregate vertex overflow")?;triangles+=st.indices.len();
   if vertices>4_000_000{return Err("scene aggregate max4M vertices".into());}
   draws.push(Draw{stream:st.clone(),world:n.world,surface:s.materials[n.material].clone()});}}
 let resolved_count=resolved.iter().filter(|r|r.is_some()).count();
 crate::guard::check();let draw_count=draws.len();
 Ok(Compiled{draws,lights:s.lights.clone(),camera:s.camera,stats:Stats{sources:s.sources.len(),sources_resolved:resolved_count,nodes:s.nodes.len(),draws:draw_count,vertices,triangles,contour_polygonized:polygonized,contour_cache_hits:hits}})}

#[cfg(test)]mod tests{use super::*;
 fn cam()->Camera{Camera{eye:[0.,0.,4.],pan:0.,tilt:0.,roll:0.,sensor_height:24.,focal_length:35.,near:0.1}}
 fn surf()->Surface{Surface{albedo:[0.8,0.2,0.2],metallic:0.,roughness:0.5,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:false}}
 fn ident()->Matrix{[[1.,0.,0.,0.],[0.,1.,0.,0.],[0.,0.,1.,0.],[0.,0.,0.,1.]]}
 fn sphere_source()->Source{Source::Primitive{primitive:Primitive::Sphere{radius:1.,longitude_slices:12,latitude_slices:6}}}
 #[test]fn empty_scene_is_zero_draws_not_error(){let s=Scene{sources:vec![],nodes:vec![],materials:vec![],lights:vec![],camera:cam()};let c=compile(&s,&mut Cache::new()).unwrap();assert!(c.draws.is_empty());assert_eq!((c.stats.vertices,c.stats.triangles,c.stats.sources_resolved),(0,0,0));}
 #[test]fn unreferenced_sources_not_resolved(){let s=Scene{sources:vec![sphere_source()],nodes:vec![],materials:vec![surf()],lights:vec![],camera:cam()};let c=compile(&s,&mut Cache::new()).unwrap();assert_eq!(c.stats.sources_resolved,0,"no node -> no generate");assert!(c.draws.is_empty());}
 #[test]fn dangling_refs_and_bad_matrix_are_typed_errors(){let base=Scene{sources:vec![sphere_source()],nodes:vec![],materials:vec![surf()],lights:vec![],camera:cam()};
  let mut s=base.clone();s.nodes.push(Node{source:1,world:ident(),material:0});assert!(compile(&s,&mut Cache::new()).unwrap_err().contains("node 0 references source 1 of 1"));
  let mut s=base.clone();s.nodes.push(Node{source:0,world:ident(),material:7});assert!(compile(&s,&mut Cache::new()).unwrap_err().contains("node 0 references material 7 of 1"));
  let mut s=base;let mut m=ident();m[0][0]=f64::NAN;s.nodes.push(Node{source:0,world:m,material:0});assert!(compile(&s,&mut Cache::new()).unwrap_err().contains("node 0: finite affine"));}
 #[test]fn primitive_node_draw_carries_unbaked_world_and_surface(){let mut w=ident();w[0][3]=2.5;let s=Scene{sources:vec![sphere_source()],nodes:vec![Node{source:0,world:w,material:0}],materials:vec![surf()],lights:vec![Light::Directional{travel_direction:[0.,0.,-1.],radiance:[1.;3]}],camera:cam()};
  let c=compile(&s,&mut Cache::new()).unwrap();assert_eq!(c.draws.len(),1);let d=&c.draws[0];assert_eq!(d.world,w,"world echoed unbaked");assert_eq!(d.surface.albedo,[0.8,0.2,0.2]);
  let g=generate(&Primitive::Sphere{radius:1.,longitude_slices:12,latitude_slices:6}).unwrap();assert_eq!(d.stream.vertices.len(),g.vertices.len());assert_eq!(d.stream.vertices[0].position,g.vertices[0].position,"stream source-local, bit-identical to direct generate");
  assert_eq!(c.stats.draws,1);assert_eq!(c.lights.len(),1);}
 #[cfg(feature="surface-contour")]
 #[test]fn sdf_contour_cached_by_content_hash(){let graph=Graph{nodes:vec![crate::threed::sdf_graph::Field::Sphere{center:[0.;3],radius:1.}],output:0};let config=contour_wrapped::Config{bounds:[[-1.5;3],[1.5;3]],resolution:12,normal_epsilon:0.001};
  // two byte-identical SDF sources + three nodes -> ONE polygonization, one hash hit
  let s=Scene{sources:vec![Source::SdfContour{graph:graph.clone(),config:config.clone()},Source::SdfContour{graph,config}],nodes:vec![Node{source:0,world:ident(),material:0},Node{source:1,world:ident(),material:0},Node{source:0,world:ident(),material:0}],materials:vec![surf()],lights:vec![],camera:cam()};
  let mut cache=Cache::new();let c=compile(&s,&mut cache).unwrap();assert_eq!((c.stats.contour_polygonized,c.stats.contour_cache_hits),(1,1),"one polygonize + one hit, got {:?}",c.stats.draws);assert_eq!(c.draws.len(),3);assert_eq!(cache.contour_entries(),1);
  // second compile with the same cache: pure hits, zero polygonization
  let c2=compile(&s,&mut cache).unwrap();assert_eq!((c2.stats.contour_polygonized,c2.stats.contour_cache_hits),(0,2),"second compile: both sources hash-hit");
  for(a,b)in c.draws[0].stream.vertices.iter().zip(&c.draws[1].stream.vertices){assert_eq!(a.position,b.position);}}
 #[cfg(feature="surface-contour")]
 fn octree_cfg()->crate::threed::octree::OctreeConfig{crate::threed::octree::OctreeConfig{bounds:[[-2.2;3],[2.2;3]],min_depth:2,max_depth:7,error_tolerance:0.02,normal_angle_degrees:20.,normal_epsilon:0.001}}
 #[cfg(feature="surface-contour")]
 fn octree_sphere_source()->Source{Source::OctreeContour{graph:Graph{nodes:vec![crate::threed::sdf_graph::Field::Sphere{center:[0.;3],radius:1.}],output:0},config:octree_cfg()}}
 #[cfg(feature="surface-contour")]
 #[test]fn octree_contour_resolves_memoized_and_matches_direct_extraction(){
  let graph=Graph{nodes:vec![crate::threed::sdf_graph::Field::Sphere{center:[0.;3],radius:1.}],output:0};
  let s=Scene{sources:vec![octree_sphere_source()],nodes:vec![Node{source:0,world:ident(),material:0}],materials:vec![surf()],lights:vec![],camera:cam()};
  let mut cache=Cache::new();
  let c1=compile(&s,&mut cache).unwrap();assert_eq!((c1.stats.contour_polygonized,c1.stats.contour_cache_hits),(1,0));
  let c2=compile(&s,&mut cache).unwrap();assert_eq!((c2.stats.contour_polygonized,c2.stats.contour_cache_hits),(0,1),"second compile hits the content memo");
  let direct=crate::threed::octree::extract_mesh(&graph,&octree_cfg()).unwrap();
  let st=&c1.draws[0].stream;
  assert_eq!((st.vertices.len(),st.indices.len()),(direct.stream.vertices.len(),direct.stream.indices.len()));
  for i in [0usize,st.vertices.len()/2,st.vertices.len()-1]{assert_eq!(st.vertices[i].position,direct.stream.vertices[i].position);assert_eq!(st.vertices[i].normal,direct.stream.vertices[i].normal,"vertex {i}: Hermite normals flow into scene3d unchanged");}
  assert_eq!((direct.boundary_edges,direct.nonmanifold_edges),(0,0),"closed manifold into scene3d");}
 #[cfg(feature="surface-contour")]
 #[test]fn octree_contour_cpu_frame_bit_matches_stream_source(){
  let graph=Graph{nodes:vec![crate::threed::sdf_graph::Field::Sphere{center:[0.;3],radius:1.}],output:0};
  let mesh=crate::threed::octree::extract_mesh(&graph,&octree_cfg()).unwrap();
  let lights=vec![Light::Directional{travel_direction:[-0.3,-0.5,1.],radiance:[2.5;3]}];
  let fwd={let mut m=ident();m[2][3]=8.;m};
  let mk=|src:Source|Scene{sources:vec![src],nodes:vec![Node{source:0,world:fwd,material:0}],materials:vec![surf()],lights:lights.clone(),camera:cam()};
  let a=render_cpu(&compile(&mk(octree_sphere_source()),&mut Cache::new()).unwrap(),96,54).unwrap();
  let b=render_cpu(&compile(&mk(Source::Stream{stream:mesh.stream}),&mut Cache::new()).unwrap(),96,54).unwrap();
  assert!(a.covered>0);assert_eq!(a.frame.d,b.frame.d,"same mesh, same raster: octree source bit-matches raw stream source on the CPU path");}
 #[cfg(feature="surface-contour")]
 #[test]fn octree_contour_bad_graph_and_bad_config_are_typed_errors(){
  let bad_graph=Scene{sources:vec![Source::OctreeContour{graph:Graph{nodes:vec![],output:0},config:octree_cfg()}],nodes:vec![],materials:vec![],lights:vec![],camera:cam()};
  assert!(validate(&bad_graph).unwrap_err().contains("source 0"),"empty graph: typed source error");
  let mut cfg=octree_cfg();cfg.max_depth=99;
  let s=Scene{sources:vec![Source::OctreeContour{graph:Graph{nodes:vec![crate::threed::sdf_graph::Field::Sphere{center:[0.;3],radius:1.}],output:0},config:cfg}],nodes:vec![Node{source:0,world:ident(),material:0}],materials:vec![surf()],lights:vec![],camera:cam()};
  assert!(compile(&s,&mut Cache::new()).unwrap_err().contains("depth"),"bad octree config: typed resolve error, not panic");}
 #[test]fn camera_required_and_degenerate_rejected_no_fallback(){
  // missing camera: typed serde error - there is NO default-camera fallback
  let j=serde_json::json!({"sources":[],"nodes":[],"materials":[],"lights":[]});
  assert!(serde_json::from_value::<Scene>(j).is_err(),"camera is a required field");
  // degenerate camera (near=0): typed validation error
  let mut s=Scene{sources:vec![],nodes:vec![],materials:vec![],lights:vec![],camera:cam()};s.camera.near=0.;
  assert!(compile(&s,&mut Cache::new()).unwrap_err().contains("positive sensor_height/focal_length/near"));}
 #[cfg(feature="surface-contour")]
 #[test]fn contour_cache_fifo_eviction_order_and_recontour(){
  let mk=|i|Source::SdfContour{graph:Graph{nodes:vec![crate::threed::sdf_graph::Field::Sphere{center:[0.;3],radius:0.5+i as f64*0.01}],output:0},config:contour_wrapped::Config{bounds:[[-2.;3],[2.;3]],resolution:8,normal_epsilon:0.001}};
  let run=|src:Source,cache:&mut Cache|{let s=Scene{sources:vec![src],nodes:vec![Node{source:0,world:ident(),material:0}],materials:vec![surf()],lights:vec![],camera:cam()};compile(&s,cache).unwrap().stats};
  let mut cache=Cache::new();
  for i in 0..CONTOUR_CACHE_CAP{let st=run(mk(i),&mut cache);assert_eq!(st.contour_polygonized,1,"first request contours");}
  assert_eq!(cache.contour_entries(),CONTOUR_CACHE_CAP);
  // a HIT does not refresh FIFO position: hit entry 0, then insert one more -> 0 still evicted first
  let st=run(mk(0),&mut cache);assert_eq!((st.contour_polygonized,st.contour_cache_hits),(0,1),"re-request before cap overflow hits");
  let st=run(mk(CONTOUR_CACHE_CAP),&mut cache);assert_eq!(st.contour_polygonized,1);assert_eq!(cache.contour_entries(),CONTOUR_CACHE_CAP,"cap holds");
  // exactly the front was evicted: entry 1 is still a pure hit
  let st=run(mk(1),&mut cache);assert_eq!((st.contour_polygonized,st.contour_cache_hits),(0,1),"FIFO: only the front was evicted");
  // entry 0 was evicted despite the earlier hit (strict FIFO, no refresh): re-request re-contours
  let st=run(mk(0),&mut cache);assert_eq!((st.contour_polygonized,st.contour_cache_hits),(1,0),"evicted entry re-contours");}
 #[test]fn render_cpu_occludes_nearer_over_farther(){
  // camera at origin, +z view: near red sphere at z=+2, far blue at z=+6, same lateral spot: center pixels
  // must be the near material's shaded tone, and output must differ from the far-only render
  let near=Stream{vertices:generate(&Primitive::Sphere{radius:1.,longitude_slices:16,latitude_slices:8}).unwrap().vertices,indices:generate(&Primitive::Sphere{radius:1.,longitude_slices:16,latitude_slices:8}).unwrap().indices};
  let m_node=|z|{let mut m=ident();m[2][3]=z;m};
  let red=Surface{albedo:[0.9,0.1,0.1],metallic:0.,roughness:0.5,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:false};
  let blue=Surface{albedo:[0.1,0.1,0.9],metallic:0.,roughness:0.5,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:false};
  let cam=Camera{eye:[0.,0.,0.],pan:0.,tilt:0.,roll:0.,sensor_height:24.,focal_length:35.,near:0.1};
  let lights=vec![Light::Directional{travel_direction:[0.,0.,1.],radiance:[2.;3]}];
  let s=Scene{sources:vec![Source::Stream{stream:near.clone()}],nodes:vec![Node{source:0,world:m_node(2.),material:0},Node{source:0,world:m_node(6.),material:1}],materials:vec![red.clone(),blue.clone()],lights:lights.clone(),camera:cam};
  let c=compile(&s,&mut Cache::new()).unwrap();assert_eq!(c.draws.len(),2);
  let multi=render_cpu(&c,64,36).unwrap();assert!(multi.covered>0);
  // isolated far-only baseline: same source at z=-6 with the blue material
  let s_far=Scene{sources:vec![Source::Stream{stream:near}],nodes:vec![Node{source:0,world:m_node(6.),material:1}],materials:vec![red,blue],lights,camera:cam};
  let far=render_cpu(&compile(&s_far,&mut Cache::new()).unwrap(),64,36).unwrap();
  let cx=(18*64+32)*4;assert!(multi.frame.d[cx]>far.frame.d[cx],"center pixel: near red out-radiances far blue channel (occlusion), {} vs {}",multi.frame.d[cx],far.frame.d[cx]);
  assert!(multi.frame.d.iter().zip(&far.frame.d).any(|(a,b)|a!=b),"multi differs from far-only baseline");}
 #[cfg(all(feature="mesh-import",feature="material-textures",feature="flat-sah",not(feature="gpu3d")))]
 #[test]fn render_wgpu_combo_without_gpu3d_typed_error_or_honest_fallback(){
  let s=Scene{sources:vec![sphere_source()],nodes:vec![Node{source:0,world:ident(),material:0}],materials:vec![surf()],lights:vec![Light::Directional{travel_direction:[0.,0.,1.],radiance:[1.;3]}],camera:cam()};
  let c=compile(&s,&mut Cache::new()).unwrap();
  assert!(render_wgpu(&c,&[],64,36,[0.2;3],0.,true,false).err().unwrap().contains("native gpu3d not compiled"),"no gpu3d: typed error, never silent");
  let(_,st)=render_wgpu(&c,&[],64,36,[0.2;3],0.,true,true).unwrap();assert!(!st.gpu_rendered);assert!(st.fallback_reason.is_some(),"allow_cpu: honest labeled fallback");}
 #[test]fn scene_json_roundtrip_deterministic(){let s=Scene{sources:vec![sphere_source()],nodes:vec![Node{source:0,world:ident(),material:0}],materials:vec![surf()],lights:vec![Light::Point{position:[0.,0.,3.],intensity:[1.;3],min_distance:0.01}],camera:cam()};
  let j=serde_json::to_string(&s).unwrap();let back:Scene=serde_json::from_str(&j).unwrap();assert_eq!(serde_json::to_string(&back).unwrap(),j,"JSON round-trip stable");}
 #[cfg(all(feature="gpu3d",feature="mesh-import",feature="material-textures",feature="flat-sah",feature="surface-contour"))]
 #[test]fn render_wgpu_mixed_scene_parity_llvmpipe(){
  // mixed glTF + SDF contour + primitive through the wgpu executor (software adapter),
  // VOLUMETRICALLY SEPARATED (screen-space overlap only): (a) gpu_rendered, (b) occlusion
  // order matches the CPU executor, (c) tight tolerance parity vs the flat-SAH reference
  // via pbr_offscreen::compare, (d) coverage IoU vs render_cpu. Intersecting surfaces are
  // covered separately below - f32 raster vs f64 references disagree on a thin band there.
  let cube=Stream{vertices:generate(&Primitive::Cube{size:[1.2;3],subdivisions:1}).unwrap().vertices,indices:generate(&Primitive::Cube{size:[1.2;3],subdivisions:1}).unwrap().indices};
  let glb=crate::threed::gltf_export::to_glb(&cube).unwrap();
  let graph=Graph{nodes:vec![crate::threed::sdf_graph::Field::Sphere{center:[0.;3],radius:0.8}],output:0};
  let blue=Surface{albedo:[0.15,0.25,0.95],metallic:0.,roughness:0.5,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:false};
  let green=Surface{albedo:[0.15,0.85,0.25],metallic:0.,roughness:0.5,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:false};
  let s=Scene{
   sources:vec![Source::Primitive{primitive:Primitive::Sphere{radius:0.8,longitude_slices:24,latitude_slices:12}},Source::GltfScene{bytes:glb,buffers:vec![],derive_tangents:false,scene_index:0},Source::SdfContour{graph,config:contour_wrapped::Config{bounds:[[-1.2;3],[1.2;3]],resolution:16,normal_epsilon:0.001}}],
   nodes:vec![Node{source:0,world:{let mut m=ident();m[0][3]=-0.5;m[2][3]=2.5;m},material:0},Node{source:1,world:{let mut m=ident();m[0][3]=0.6;m[2][3]=4.0;m},material:2},Node{source:2,world:{let mut m=ident();m[0][3]=0.2;m[1][3]=0.55;m[2][3]=5.5;m},material:1},
    Node{source:0,world:{let mut m=ident();m[0][0]=0.5;m[1][1]=0.5;m[2][2]=0.5;m[0][3]=0.75;m[1][3]=-0.8;m[2][3]=2.9;m},material:3},
    Node{source:1,world:[[-0.45,0.,0.,-0.85],[0.,0.45,0.,-0.75],[0.,0.,0.45,4.4],[0.,0.,0.,1.]],material:3},
    Node{source:1,world:[[-0.45,0.,0.,1.05],[0.,0.45,0.,0.75],[0.,0.,0.45,3.0],[0.,0.,0.,1.]],material:2}],
   materials:vec![surf(),blue,green,Surface{albedo:[0.9,0.8,0.15],metallic:0.,roughness:0.5,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:true}],
   lights:vec![Light::Directional{travel_direction:[0.2,0.3,1.],radiance:[2.;3]}],
   camera:Camera{eye:[0.,0.,0.],pan:0.,tilt:0.,roll:0.,sensor_height:24.,focal_length:35.,near:0.5}};
  let (w,h)=(96,54);
  let compiled=compile(&s,&mut Cache::new()).unwrap();assert_eq!(compiled.stats.draws,6);
  // all 4 pipeline branches exercised: (double_sided x reflected-world) across the 6 draws
  let branches:std::collections::BTreeSet<(bool,bool)>=compiled.draws.iter().map(|d|{let m=d.world;let det=m[0][0]*(m[1][1]*m[2][2]-m[1][2]*m[2][1])-m[0][1]*(m[1][0]*m[2][2]-m[1][2]*m[2][0])+m[0][2]*(m[1][0]*m[2][1]-m[1][1]*m[2][0]);(d.surface.double_sided,det<0.)}).collect();
  assert_eq!(branches.len(),4,"all (double,reflect) pipeline branches present: {branches:?}");
  let(gpu,status)=render_wgpu(&compiled,&[],w,h,[0.2;3],0.,true,false).unwrap();
  assert!(status.gpu_rendered,"llvmpipe adapter must render: {:?}",status.fallback_reason);
  // (b) occlusion: center-left pixel is the NEAR red sphere, not anything behind it
  let cid=((27*96)+40)*4;assert!(gpu.d[cid]>gpu.d[cid+1]&&gpu.d[cid]>gpu.d[cid+2],"near red wins at center-left: {:?}",&gpu.d[cid..cid+4]);
  // (c) tolerance parity vs the flat-SAH reference on identical draws
  let draws:Vec<crate::threed::pbr_offscreen::Draw>=compiled.draws.iter().map(|d|crate::threed::pbr_offscreen::Draw{stream:d.stream.clone(),world:d.world,surface:d.surface.clone()}).collect();
  let reference=crate::threed::pbr_offscreen::cpu(&draws,&[],&s.camera,w,h,&s.lights,[0.2;3]).unwrap();
  let cmp=crate::threed::pbr_offscreen::compare(&gpu,&reference,w,h).unwrap();
  if let Ok(dir)=std::env::var("KIMO_GPU_PBR_FRAMES"){crate::render::write_image(&gpu,&format!("{dir}/cp3_gpu.png")).unwrap();crate::render::write_image(&reference,&format!("{dir}/cp3_ref.png")).unwrap();}
  assert!(cmp.passed,"gpu vs flat-SAH reference parity: {cmp:?}");
  // (d) coverage IoU vs the CPU scene executor (different shading path: metric, not bit-exact)
  let cpu=render_cpu(&compiled,w,h).unwrap();
  let cov=|f:&crate::effects::Frame|(0..w*h).map(|i|f.d[i*4+3]>0.5).collect::<Vec<_>>();
  let(a,b)=(cov(&gpu),cov(&cpu.frame));
  let inter=a.iter().zip(&b).filter(|(x,y)|**x&&**y).count();let union=a.iter().zip(&b).filter(|(x,y)|**x||**y).count();
  let iou=inter as f64/union as f64;assert!(iou>0.98,"coverage IoU gpu vs render_cpu {iou:.4}");
  println!("SCENE-WGPU parity: compare={cmp:?} iou_vs_render_cpu={iou:.4}");}
 #[cfg(all(feature="gpu3d",feature="mesh-import",feature="material-textures",feature="flat-sah",feature="surface-contour"))]
 #[test]fn render_wgpu_octree_contour_scene(){
  // octree adaptive contour source through the wgpu executor (software adapter): vertex/index
  // upload + Hermite normals + pbr ingest of the Surface factors. Same contract as the mixed
  // parity test: gpu_rendered, tolerance parity vs flat-SAH reference, coverage IoU vs render_cpu.
  let fwd={let mut m=ident();m[2][3]=8.;m};
  let s=Scene{sources:vec![octree_sphere_source()],nodes:vec![Node{source:0,world:fwd,material:0}],materials:vec![surf()],lights:vec![Light::Directional{travel_direction:[0.3,-0.4,1.],radiance:[2.;3]},Light::Point{position:[0.,2.,4.],intensity:[1.;3],min_distance:0.01}],camera:cam()};
  let(w,h)=(96,54);
  let compiled=compile(&s,&mut Cache::new()).unwrap();assert_eq!(compiled.stats.draws,1);
  let(gpu,status)=render_wgpu(&compiled,&[],w,h,[0.2;3],0.,true,false).unwrap();
  assert!(status.gpu_rendered,"llvmpipe adapter must render: {:?}",status.fallback_reason);
  let draws:Vec<crate::threed::pbr_offscreen::Draw>=compiled.draws.iter().map(|d|crate::threed::pbr_offscreen::Draw{stream:d.stream.clone(),world:d.world,surface:d.surface.clone()}).collect();
  let reference=crate::threed::pbr_offscreen::cpu(&draws,&[],&s.camera,w,h,&s.lights,[0.2;3]).unwrap();
  let cmp=crate::threed::pbr_offscreen::compare(&gpu,&reference,w,h).unwrap();
  assert!(cmp.passed,"octree gpu vs flat-SAH reference parity: {cmp:?}");
  let cpu=render_cpu(&compiled,w,h).unwrap();
  let cov=|f:&crate::effects::Frame|(0..w*h).map(|i|f.d[i*4+3]>0.5).collect::<Vec<_>>();
  let(a,b)=(cov(&gpu),cov(&cpu.frame));
  let inter=a.iter().zip(&b).filter(|(x,y)|**x&&**y).count();let union=a.iter().zip(&b).filter(|(x,y)|**x||**y).count();
  let iou=inter as f64/union as f64;assert!(iou>0.98,"coverage IoU gpu vs render_cpu {iou:.4}");
  if let Ok(dir)=std::env::var("KIMO_GPU_PBR_FRAMES"){std::fs::create_dir_all(&dir).unwrap();crate::render::write_image(&gpu,&format!("{dir}/cp3_octree_gpu.png")).unwrap();crate::render::write_image(&reference,&format!("{dir}/cp3_octree_ref.png")).unwrap();}
  println!("SCENE-WGPU octree: compare={cmp:?} iou_vs_render_cpu={iou:.4}");}
 #[cfg(all(feature="gpu3d",feature="mesh-import",feature="material-textures",feature="flat-sah",feature="surface-contour"))]
 #[test]fn render_wgpu_intersecting_surfaces_band_is_bounded_and_layering_holds(){
  // DOCUMENTED LIMIT, pinned honestly: where surfaces genuinely intersect, the f32 wgpu
  // rasterizer and the f64 flat-SAH raycast reference disagree on a thin band around the
  // intersection curve (measured: one scanline, ~0.5% coverage, full material flips inside
  // the band). Not a shading bug: coarse layering must hold everywhere else. We pin the
  // bounded-band behavior + coarse layering, and REPORT the metrics rather than hiding them.
  let blue=Surface{albedo:[0.15,0.25,0.95],metallic:0.,roughness:0.5,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:false};
  let green=Surface{albedo:[0.15,0.85,0.25],metallic:0.,roughness:0.5,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:false};
  let s=Scene{
   sources:vec![Source::Primitive{primitive:Primitive::Sphere{radius:1.,longitude_slices:24,latitude_slices:12}},Source::Primitive{primitive:Primitive::Sphere{radius:1.,longitude_slices:24,latitude_slices:12}}],
   nodes:vec![Node{source:0,world:{let mut m=ident();m[2][3]=3.4;m},material:0},Node{source:1,world:{let mut m=ident();m[0][3]=0.4;m[2][3]=4.0;m},material:1}],
   materials:vec![green,blue],
   lights:vec![Light::Directional{travel_direction:[0.2,0.3,1.],radiance:[2.;3]}],
   camera:Camera{eye:[0.,0.,0.],pan:0.,tilt:0.,roll:0.,sensor_height:24.,focal_length:35.,near:0.5}};
  let (w,h)=(96,54);
  let compiled=compile(&s,&mut Cache::new()).unwrap();
  let(gpu,status)=render_wgpu(&compiled,&[],w,h,[0.2;3],0.,true,false).unwrap();assert!(status.gpu_rendered);
  let draws:Vec<crate::threed::pbr_offscreen::Draw>=compiled.draws.iter().map(|d|crate::threed::pbr_offscreen::Draw{stream:d.stream.clone(),world:d.world,surface:d.surface.clone()}).collect();
  let reference=crate::threed::pbr_offscreen::cpu(&draws,&[],&s.camera,w,h,&s.lights,[0.2;3]).unwrap();
  let cmp=crate::threed::pbr_offscreen::compare(&gpu,&reference,w,h).unwrap();
  let(gd,rd)=(&gpu.d,&reference.d);
  let rows:std::collections::BTreeSet<usize>=(1..h-1).flat_map(|y|(1..w-1).filter_map(move |x|{let id=(y*w+x)*4;let e=(0..3).map(|j|(gd[id+j]-rd[id+j]).abs()).fold(0f32,f32::max);(e>8./255.).then_some(y)})).collect();
  assert!(rows.len()<=3,"intersection disagreement confined to a thin band, rows={rows:?}");
  assert!((cmp.coverage_mismatches as f64/(w*h) as f64)<0.02,"coverage disagreement bounded: {}",cmp.coverage_mismatches);
  // coarse layering: left-of-center the NEAR green sphere wins in BOTH executors
  let cid=((27*96)+34)*4;assert!(gpu.d[cid+1]>gpu.d[cid]&&gpu.d[cid+1]>gpu.d[cid+2],"gpu near green at left: {:?}",&gpu.d[cid..cid+4]);
  assert!(reference.d[cid+1]>reference.d[cid]&&reference.d[cid+1]>reference.d[cid+2],"ref near green at left");
  println!("SCENE-WGPU intersecting: band_rows={} coverage_mismatches={} max_rgb={:.3} (documented f32-vs-f64 band)",rows.len(),cmp.coverage_mismatches,cmp.max_rgb_error);}
 #[cfg(all(feature="mesh-import",feature="material-textures",feature="flat-sah"))]
 #[test]fn render_wgpu_over_64_draws_is_a_descriptive_typed_error(){
  let s=Scene{sources:vec![sphere_source()],nodes:(0..65).map(|_|Node{source:0,world:ident(),material:0}).collect(),materials:vec![surf()],lights:vec![],camera:cam()};
  let c=compile(&s,&mut Cache::new()).unwrap();assert_eq!(c.draws.len(),65);
  let e=match render_wgpu(&c,&[],64,36,[0.2;3],0.,true,true){Err(e)=>e,Ok(_)=>panic!("65 draws must be rejected")};
  assert_eq!(e,"wgpu executor max64 draws per scene pass (got 65); the CPU executor allows 256 - split the scene or use render_cpu");
  // at exactly 64 the error must NOT fire (dispatch proceeds to the backend)
  let s=Scene{sources:vec![sphere_source()],nodes:(0..64).map(|_|Node{source:0,world:ident(),material:0}).collect(),materials:vec![surf()],lights:vec![],camera:cam()};
  let c=compile(&s,&mut Cache::new()).unwrap();
  #[cfg(not(feature="gpu3d"))]
  assert!(render_wgpu(&c,&[],64,36,[0.2;3],0.,true,false).err().unwrap().contains("native gpu3d not compiled"),"64 draws: past the cap check, fails only at backend dispatch");
  #[cfg(feature="gpu3d")]
  {let(_,st)=render_wgpu(&c,&[],64,36,[0.2;3],0.,true,false).unwrap();assert!(st.gpu_rendered,"64 draws render on the adapter");}}
 #[cfg(all(feature="mesh-import",feature="surface-contour"))]
 #[test]fn render_cpu_mixed_gltf_and_sdf_sources_matches_argmin_oracle(){
  // in-memory glTF scene (round-tripped cube) + SDF contour sphere, overlapping: the
  // multi-draw frame must equal the pixelwise argmin-depth composite of isolated draws
  let cube=Stream{vertices:generate(&Primitive::Cube{size:[1.6;3],subdivisions:1}).unwrap().vertices,indices:generate(&Primitive::Cube{size:[1.6;3],subdivisions:1}).unwrap().indices};
  let glb=crate::threed::gltf_export::to_glb(&cube).unwrap();
  let graph=Graph{nodes:vec![crate::threed::sdf_graph::Field::Sphere{center:[0.;3],radius:1.}],output:0};
  let s=Scene{
   sources:vec![Source::GltfScene{bytes:glb,buffers:vec![],derive_tangents:false,scene_index:0},Source::SdfContour{graph,config:contour_wrapped::Config{bounds:[[-1.5;3],[1.5;3]],resolution:16,normal_epsilon:0.001}}],
   nodes:vec![Node{source:0,world:{let mut m=ident();m[0][3]=-0.4;m[2][3]=4.0;m},material:0},Node{source:1,world:{let mut m=ident();m[0][3]=0.4;m[2][3]=4.4;m},material:1}],
   materials:vec![surf(),Surface{albedo:[0.2,0.2,0.9],metallic:0.,roughness:0.5,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:false}],
   lights:vec![Light::Directional{travel_direction:[0.2,0.3,1.],radiance:[2.;3]}],
   camera:Camera{eye:[0.,0.,0.],pan:0.,tilt:0.,roll:0.,sensor_height:24.,focal_length:35.,near:0.5}};
  let (w,h)=(96,54);
  let multi=render_cpu(&compile(&s,&mut Cache::new()).unwrap(),w,h).unwrap();assert!(multi.covered>100);
  // per-node isolated baselines -> argmin-depth oracle
  let iso:Vec<_>=(0..2).map(|i|{let one=Scene{sources:s.sources.clone(),nodes:vec![s.nodes[i].clone()],materials:s.materials.clone(),lights:s.lights.clone(),camera:s.camera};render_cpu(&compile(&one,&mut Cache::new()).unwrap(),w,h).unwrap()}).collect();
  for px in 0..w*h{
   let mut best:Option<(f64,usize)>=None;
   for(i,o)in iso.iter().enumerate(){let z=o.depth[px];if z.is_finite()&&best.map(|(bz,_)|z<bz).unwrap_or(true){best=Some((z,i));}}
   match best{None=>assert!(multi.frame.d[px*4..px*4+4].iter().all(|v|*v==0.)),Some((_,i))=>assert_eq!(&multi.frame.d[px*4..px*4+4],&iso[i].frame.d[px*4..px*4+4],"px {px} from nearest draw {i}"),}}
 }
}
