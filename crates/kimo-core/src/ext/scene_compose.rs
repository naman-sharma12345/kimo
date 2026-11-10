use super::kit::*;fn read<T:serde::de::DeserializeOwned>(a:&Value,k:&str)->Result<T,String>{serde_json::from_value(a.get(k).cloned().ok_or_else(||crate::errors::arg_err(a,k,"value"))?).map_err(|e|format!("{k}: {e}"))}
fn split_scene(a:&Value)->Result<(crate::threed::scene::Scene,Option<crate::threed::scene::CpuLighting>),String>{if a.get("scene").and_then(|v|v.get("cpu_lighting")).is_none(){return Ok((read(a,"scene")?,None));}let mut v=a["scene"].clone();let l=crate::threed::scene::take_cpu_lighting(&mut v)?;Ok((serde_json::from_value(v).map_err(|e|format!("scene: {e}"))?,l))}
pub fn commands()->Vec<Command>{vec![
 Command{name:"compile_scene3d_descriptor",description:"Compile a scene descriptor to resolved draws. Sources: primitive / sdf_contour (memoized by content hash within the call) / raw vertex stream / gltf_scene (internal node worlds baked by the glTF instantiate path). Nodes: explicit row-major affine world matrix + material index; draws carry source-local streams with UNBAKED world matrices (wgpu multi-draw contract; a CPU executor bakes via the inverse-transpose stream transform). Lights (directional/point, max64) and camera-v2 are validated and echoed for the pass level. Unreferenced sources are shape-validated, never built; empty node list compiles to zero draws. No keyframes (deferred to the Scene3D bridge), no rendering in this tool.",mutates:false,schema:obj(vec![("scene",json!({"type":"object"}))],&["scene"]),run:|_,a|{
  let scene:crate::threed::scene::Scene=read(a,"scene")?;
  let mut cache=crate::threed::scene::Cache::new();
  let out=crate::threed::scene::compile(&scene,&mut cache)?;
  Ok(serde_json::to_value(&out).map_err(|e|e.to_string())?)}},
 Command{name:"render_scene3d_cpu",description:"Render a compiled 3D scene on the CPU with one shared depth buffer, so occlusion resolves per pixel across sources. CPU multi-draw executor over a compiled scene descriptor: every (node, source-instance) draw rasterized onto ONE shared frame + depth buffer, so cross-source occlusion, intersecting surfaces and near-plane clipping resolve per-pixel (exact depth ties keep the earlier draw; no sorting, painter order is the documented tiebreak). Surface textures are NOT sampled by this executor - albedo/metallic/roughness scalars only. Camera-v2 has no far plane (near-plane clip only). Bit-identical to the argmin-depth composite of isolated per-draw renders (tested). Frame tone-mapped with the ACES fitted curve (exposure 1.0), then deterministic TPDF-dithered before 8-bit PNG quantization. Fresh contour cache per call (stateless). No shadows, no specular IBL; CPU correctness pass - premium look pending.",mutates:false,schema:obj(vec![("scene",json!({"type":"object"})),("width",json!({"type":"integer"})),("height",json!({"type":"integer"})),("path",s("PNG"))],&["scene","width","height","path"]),run:|_,a|{
  let path=str_arg(a,"path")?;if !path.to_lowercase().ends_with(".png"){return Err("PNG required".into());}
  let(scene,lighting)=split_scene(a)?;
  let mut cache=crate::threed::scene::Cache::new();
  let compiled=crate::threed::scene::compile(&scene,&mut cache)?;
  if let Some(l)=lighting{let(out,info)=crate::threed::scene::render_cpu_lit(&compiled,read(a,"width")?,read(a,"height")?,&l,None)?;crate::render::write_image_tpdf(&out.frame,path)?;
   return Ok(json!({"path":path,"covered_pixels":out.covered,"clipped_triangles":out.clipped_triangles,"draws":compiled.stats.draws,"sources_resolved":compiled.stats.sources_resolved,"vertices":compiled.stats.vertices,"triangles":compiled.stats.triangles,"contour_polygonized":compiled.stats.contour_polygonized,"contour_cache_hits":compiled.stats.contour_cache_hits,"backend":"cpu_multi_draw_shared_depth","depth_tiebreak":"exact ties keep the earlier draw (painter order)","textures":"not sampled (scalar albedo/metallic/roughness only)","far_plane":"none (camera-v2 is near-clip only)","png_encoding":"ACES fitted tone map (exposure 1.0), TPDF-dithered 8-bit, display-referred","cpu_lighting":info}));}
  let out=crate::threed::scene::render_cpu(&compiled,read(a,"width")?,read(a,"height")?)?;
  crate::render::write_image_tpdf(&out.frame,path)?;
  Ok(json!({"path":path,"covered_pixels":out.covered,"clipped_triangles":out.clipped_triangles,"draws":compiled.stats.draws,"sources_resolved":compiled.stats.sources_resolved,"vertices":compiled.stats.vertices,"triangles":compiled.stats.triangles,"contour_polygonized":compiled.stats.contour_polygonized,"contour_cache_hits":compiled.stats.contour_cache_hits,"backend":"cpu_multi_draw_shared_depth","depth_tiebreak":"exact ties keep the earlier draw (painter order)","textures":"not sampled (scalar albedo/metallic/roughness only)","far_plane":"none (camera-v2 is near-clip only)","png_encoding":"ACES fitted tone map (exposure 1.0), TPDF-dithered 8-bit, display-referred"}))}},
 Command{name:"render_scene3d_wgpu",description:"Render a compiled 3D scene on the GPU (wgpu, GGX shading) with one shared depth buffer. wgpu multi-draw executor over a compiled scene descriptor (CP3): every (node, source-instance) draw rasterized by the WGSL GGX pass onto ONE shared Depth32Float (Less compare; an exact depth tie keeps the earlier draw, matching the CPU painter tiebreak). Requires the gpu3d feature - otherwise a typed error, or the flat-SAH CPU textured reference when allow_cpu (gpu_rendered:false + fallback_reason reported). Draw cap 64, target max 1920x1080 (tighter than the CPU executor). Output is UNORM clamped linear, NO display transform - NOT bit-comparable to render_scene3d_cpu; agreement is by tolerance metrics only. Scene materials must be scalar (no texture image inputs in this tool). Defaults: ambient 0.2, time 0, software_only false, allow_cpu true.",mutates:false,schema:obj(vec![("scene",json!({"type":"object"})),("width",json!({"type":"integer"})),("height",json!({"type":"integer"})),("path",s("PNG")),("ambient_irradiance",json!({"type":"array","default":[0.2,0.2,0.2]})),("time",json!({"type":"number","default":0})),("software_only",json!({"type":"boolean","default":false})),("allow_cpu",json!({"type":"boolean","default":true}))],&["scene","width","height","path"]),run:|_,a|{
  #[cfg(all(feature="mesh-import",feature="material-textures",feature="flat-sah"))]
  {let path=str_arg(a,"path")?;if !path.to_lowercase().ends_with(".png"){return Err("PNG required".into());}
  let scene:crate::threed::scene::Scene=read(a,"scene")?;
  let mut cache=crate::threed::scene::Cache::new();
  let compiled=crate::threed::scene::compile(&scene,&mut cache)?;
  let ambient:Vec<f64>=read(a,"ambient_irradiance").unwrap_or_else(|_|vec![0.2;3]);let ambient:[f64;3]=ambient.try_into().map_err(|_|"ambient_irradiance: [f64;3] required".to_string())?;
  let time:Option<f64>=a.get("time").and_then(|v|v.as_f64());
  let software_only=a.get("software_only").and_then(|v|v.as_bool()).unwrap_or(false);
  let allow_cpu=a.get("allow_cpu").and_then(|v|v.as_bool()).unwrap_or(true);
  let(frame,status)=crate::threed::scene::render_wgpu(&compiled,&[],read(a,"width")?,read(a,"height")?,ambient,time.unwrap_or(0.),software_only,allow_cpu)?;
  crate::render::write_image(&frame,path)?;
  Ok(json!({"path":path,"backend":status.actual_backend,"gpu_rendered":status.gpu_rendered,"fallback_reason":status.fallback_reason,"target":status.target,"draws":compiled.stats.draws,"sources_resolved":compiled.stats.sources_resolved,"vertices":compiled.stats.vertices,"triangles":compiled.stats.triangles,"contour_polygonized":compiled.stats.contour_polygonized,"contour_cache_hits":compiled.stats.contour_cache_hits,"depth_tiebreak":"exact ties keep the earlier draw (Depth32Float Less, painter order)","textures":"scene materials must be scalar; no texture image inputs in this tool","parity_vs_cpu_executor":"NOT bit-comparable (UNORM clamped linear vs ACES display-referred); tolerance metrics only"}))}
  #[cfg(not(all(feature="mesh-import",feature="material-textures",feature="flat-sah")))]
  {let _=a;Err("requires mesh-import,material-textures,flat-sah".into())}}},
 Command{name:"sample_scene3d_animation",description:"Sample scene keyframe tracks at one time into a STATIC scene descriptor (CP4 bridge). Raw composable primitives: per-node translation/scale vec3 tracks (linear), rotation unit-quaternion [w,x,y,z] tracks (slerp, shorter arc), camera eye/pan/tilt/roll tracks, directional-light travel_direction/radiance and point-light position/intensity/min_distance tracks. Times clamp to first/last key; single key is constant; no easing curves or presets (easing is upstream choice of sample times). An animated node's world matrix is REPLACED by T(t)*R(t)*S(t) composed from its tracks (defaults: translation 0, identity rotation, scale 1). Wrong-variant light tracks, duplicate/dangling indices and malformed tracks are typed errors. Render the sampled descriptor with render_scene3d_cpu or render_scene3d_wgpu.",mutates:false,schema:obj(vec![("scene",json!({"type":"object"})),("animation",json!({"type":"object"})),("time",json!({"type":"number"}))],&["scene","animation","time"]),run:|_,a|{
  let scene:crate::threed::scene::Scene=read(a,"scene")?;
  let animation:crate::threed::animate::Animation=read(a,"animation")?;
  let sampled=crate::threed::animate::sample(&scene,&animation,read(a,"time")?)?;
  Ok(json!({"scene":serde_json::to_value(&sampled).map_err(|e|e.to_string())?,"nodes_animated":animation.nodes.len(),"lights_animated":animation.lights.len(),"camera_animated":animation.camera.is_some()}))}},
 Command{name:"render_scene3d_sequence",description:"Render an animated scene at a list of times to a numbered PNG sequence dir/frame_000.png... (CP4 dual verification + frame export). Each time samples the keyframe tracks (see sample_scene3d_animation) and renders through the CPU executor (backend cpu, ACES display-referred) or the wgpu executor (backend wgpu, UNORM clamped linear; requires the gpu3d feature combo, software_only/allow_cpu honored). Times: 1..64 finite values, rendered in the given order (not sorted - upstream owns pacing). Fresh contour cache per frame. Returns per-frame paths, covered pixels and stats. Max 64 frames, target max per executor.",mutates:false,schema:obj(vec![("scene",json!({"type":"object"})),("animation",json!({"type":"object"})),("times",json!({"type":"array"})),("dir",s("output directory")),("width",json!({"type":"integer"})),("height",json!({"type":"integer"})),("backend",json!({"type":"string","default":"cpu"})),("software_only",json!({"type":"boolean","default":false})),("allow_cpu",json!({"type":"boolean","default":true}))],&["scene","animation","times","dir","width","height"]),run:|_,a|{
  let(scene,lighting)=split_scene(a)?;
  let animation:crate::threed::animate::Animation=read(a,"animation")?;
  let times:Vec<f64>=read(a,"times")?;
  if times.is_empty()||times.len()>64||times.iter().any(|t|!t.is_finite()){return Err("times: 1..64 finite values required".into());}
  let dir=str_arg(a,"dir")?;std::fs::create_dir_all(dir).map_err(|e|e.to_string())?;
  let w:usize=read(a,"width")?;let h:usize=read(a,"height")?;
  let backend=a.get("backend").and_then(|v|v.as_str()).unwrap_or("cpu");
  let software_only=a.get("software_only").and_then(|v|v.as_bool()).unwrap_or(false);
  let allow_cpu=a.get("allow_cpu").and_then(|v|v.as_bool()).unwrap_or(true);
  if lighting.is_some()&&backend!="cpu"{return Err("scene.cpu_lighting requires backend cpu".into());}
  let baked_ibl=match lighting.as_ref().and_then(|l|l.ibl.as_ref()){Some(o)=>Some(crate::threed::scene::bake_ibl(o)?),None=>None};
  let mut frames=Vec::new();
  for(i,t)in times.iter().enumerate(){
   let sampled=crate::threed::animate::sample(&scene,&animation,*t)?;
   let mut cache=crate::threed::scene::Cache::new();
   let compiled=crate::threed::scene::compile(&sampled,&mut cache)?;
   let path=format!("{dir}/frame_{i:03}.png");
   let info=match backend{
    "cpu"=>if let Some(l)=&lighting{let(out,info)=crate::threed::scene::render_cpu_lit(&compiled,w,h,l,baked_ibl.as_ref())?;crate::render::write_image_tpdf(&out.frame,&path)?;json!({"covered_pixels":out.covered,"backend":"cpu_multi_draw_shared_depth","cpu_lighting":info})}else{let out=crate::threed::scene::render_cpu(&compiled,w,h)?;crate::render::write_image_tpdf(&out.frame,&path)?;json!({"covered_pixels":out.covered,"backend":"cpu_multi_draw_shared_depth"})},
    "wgpu"=>{
     #[cfg(all(feature="mesh-import",feature="material-textures",feature="flat-sah"))]
     {let(frame,status)=crate::threed::scene::render_wgpu(&compiled,&[],w,h,[0.2;3],0.,software_only,allow_cpu)?;crate::render::write_image(&frame,&path)?;json!({"backend":status.actual_backend,"gpu_rendered":status.gpu_rendered,"fallback_reason":status.fallback_reason})}
     #[cfg(not(all(feature="mesh-import",feature="material-textures",feature="flat-sah")))]
     {let _=(software_only,allow_cpu);return Err("backend wgpu requires mesh-import,material-textures,flat-sah".into());}}
    other=>return Err(format!("backend: cpu|wgpu required (got {other:?})")),};
   frames.push(json!({"index":i,"time":t,"path":path,"draws":compiled.stats.draws,"render":info}));}
  Ok(json!({"frames":frames,"count":frames.len(),"dir":dir}))}},
]}
