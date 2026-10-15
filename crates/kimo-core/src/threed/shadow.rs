//! Directional shadow maps, shared by lit_raster (CPU production), pbr_offscreen (wgpu
//! executor) and textured_bvh (raycast reference). ONE light-space definition (view frame
//! + ortho bounds), ONE bias contract, so the three paths cannot drift: the wgpu pass gets
//! the same constants as uniforms and mirrors visibility() tap-for-tap (compare-then-average
//! PCF, never average-then-compare).
//!
//! PL1 scope: directional lights only (point cube shadows are PL2). Hard shadows = PCF
//! radius 0; the raycast reference matches PCF radius 0 exactly (its shadow rays are hard).
//! Bias contract (pinned, mirrored in WGSL uniforms): receiver point is first offset along
//! the surface normal by NORMAL_OFFSET_TEXELS shadow texels (world units), then the depth
//! compare uses bias_depth = (BIAS_CONST_TEXELS + BIAS_SLOPE_TEXELS * tan(acos(nl))) texels,
//! slope term clamped to [0,4], converted to normalized depth by the ortho depth range.
use super::mesh::Matrix;
use super::vertex_stream::unit;

pub const BIAS_CONST_TEXELS:f64=1.0;
pub const BIAS_SLOPE_TEXELS:f64=2.0;
pub const NORMAL_OFFSET_TEXELS:f64=1.0;
pub const PCF_RADIUS:i32=1; // 3x3 taps; 0 = single-tap hard shadow (matches raycast reference)

#[derive(Clone,Debug)]
pub struct DirectionalShadowMap{
 /// world -> light frame (x right, y up, z along travel direction, origin at light)
 pub view:Matrix,
 /// ortho bounds in the light frame: [min,max] per axis; depth = (z-minz)/(maxz-minz) in [0,1]
 pub bounds:[[f64;3];2],
 pub width:usize,pub height:usize,
 /// nearest occluder depth per texel, normalized [0,1]; f64::INFINITY = no occluder
 pub depth:Vec<f64>,
}

fn mv(m:&Matrix,v:[f64;4])->[f64;4]{std::array::from_fn(|i|m[i][0]*v[0]+m[i][1]*v[1]+m[i][2]*v[2]+m[i][3]*v[3])}
fn sub(a:[f64;3],b:[f64;3])->[f64;3]{[a[0]-b[0],a[1]-b[1],a[2]-b[2]]}
fn cross(a:[f64;3],b:[f64;3])->[f64;3]{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}

/// Light frame + ortho bounds covering the scene AABB. Deterministic: no sampling.
pub fn directional_light_space(travel_direction:[f64;3],aabb:[[f64;3];2])->Result<(Matrix,[[f64;3];2]),String>{
 if aabb.iter().flatten().any(|v|!v.is_finite()){return Err("finite scene bounds required".into());}
 let f=unit(travel_direction)?; // light looks along the travel direction
 let seed=if f[1].abs()<0.99{[0.,1.,0.]}else{[1.,0.,0.]};
 let r=unit(cross(f,seed))?;let u=cross(r,f);
 // frame origin: AABB center pulled back along -f by the AABB radius (covers everything)
 let c=std::array::from_fn(|j|(aabb[0][j]+aabb[1][j])/2.);
 let rad=(0..3).map(|j|(aabb[1][j]-aabb[0][j])/2.).fold(0.,|s,d|s+d*d).sqrt();
 let eye=sub(c,f.map(|v|v*(rad+1e-6)));
 let view=[[r[0],r[1],r[2],-(r[0]*eye[0]+r[1]*eye[1]+r[2]*eye[2])],
           [u[0],u[1],u[2],-(u[0]*eye[0]+u[1]*eye[1]+u[2]*eye[2])],
           [f[0],f[1],f[2],-(f[0]*eye[0]+f[1]*eye[1]+f[2]*eye[2])],
           [0.,0.,0.,1.]];
 let(mut lo,mut hi)=([f64::INFINITY;3],[f64::NEG_INFINITY;3]);
 for k in 0..8{let p=[if k&1==0{aabb[0][0]}else{aabb[1][0]},if k&2==0{aabb[0][1]}else{aabb[1][1]},if k&4==0{aabb[0][2]}else{aabb[1][2]}];
  let q=mv(&view,[p[0],p[1],p[2],1.]);for j in 0..3{lo[j]=lo[j].min(q[j]);hi[j]=hi[j].max(q[j]);}}
 let pad=1e-3*(1.+rad); // keep min-depth texels off the exact near plane
 lo[2]-=pad;hi[2]+=pad;
 Ok((view,[lo,hi]))}

/// world -> (u,v,depth) in shadow-map space; None when outside [0,1] on u/v (treated lit).
pub fn shadow_uvz(map:&DirectionalShadowMap,world:[f64;3])->Option<(f64,f64,f64)>{
 let q=mv(&map.view,[world[0],world[1],world[2],1.]);
 let u=(q[0]-map.bounds[0][0])/(map.bounds[1][0]-map.bounds[0][0]);
 let v=(q[1]-map.bounds[0][1])/(map.bounds[1][1]-map.bounds[0][1]);
 let d=(q[2]-map.bounds[0][2])/(map.bounds[1][2]-map.bounds[0][2]);
 if !(0.0..=1.0).contains(&u)||!(0.0..=1.0).contains(&v){None}else{Some((u,v,d))}
}

/// World size of one depth texel along the light direction is implicit; we expose the
/// SCREEN texel in world units (max of the two axes) to scale the normal offset, and the
/// normalized-depth value of one texel for the depth bias.
fn texel_scales(map:&DirectionalShadowMap)->(f64,f64){
 let wx=(map.bounds[1][0]-map.bounds[0][0])/map.width as f64;
 let wy=(map.bounds[1][1]-map.bounds[0][1])/map.height as f64;
 let world=wx.max(wy);
 let depth_norm=world/(map.bounds[1][2]-map.bounds[0][2]); // 1 texel ~ 1 texel of depth slope scale
 (world,depth_norm)}

/// Pinned bias contract: normal offset (world) + normalized depth bias from nl in (0,1].
pub fn bias(map:&DirectionalShadowMap,n_dot_l:f64)->(f64,f64){
 let(world,depth_norm)=texel_scales(map);
 let slope=((1.-n_dot_l*n_dot_l).max(0.)).sqrt()/n_dot_l.max(1e-3);
 let slope=slope.min(4.);
 (NORMAL_OFFSET_TEXELS*world,(BIAS_CONST_TEXELS+BIAS_SLOPE_TEXELS*slope)*depth_norm)}

/// PCF visibility in [0,1]: fraction of taps where the (biased) receiver depth is nearer
 /// than the stored occluder depth. radius 0 = single-tap hard shadow.
pub fn visibility(map:&DirectionalShadowMap,world:[f64;3],normal:[f64;3],n_dot_l:f64,radius:i32)->f64{
 let(no,db)=bias(map,n_dot_l);
 let p=std::array::from_fn(|j|world[j]+normal[j]*no);
 let Some((u,v,d))=shadow_uvz(map,p)else{return 1.}; // outside the map: lit (scene AABB covers casters+receivers)
 let d=d-db;
 if !d.is_finite(){return 1.;}
 let(du,dv)=(1./map.width as f64,1./map.height as f64);
 let(mut lit,mut taps)=(0usize,0usize);
 for oy in -radius..=radius{for ox in -radius..=radius{
  let(uu,vv)=(u+ox as f64*du,v+oy as f64*dv);
  if !(0.0..=1.0).contains(&uu)||!(0.0..=1.0).contains(&vv){lit+=1;taps+=1;continue;} // off-map taps count lit
  let x=(uu*map.width as f64).min(map.width as f64-1e-9) as usize;
  let y=(vv*map.height as f64).min(map.height as f64-1e-9) as usize;
  let stored=map.depth[y*map.width+x];
  if stored==f64::INFINITY||d<=stored{lit+=1;}
  taps+=1;}}
 lit as f64/taps as f64}

// Rotated-disk soft shadows (PL1b). Tap positions are the D3D standard MSAA sample points
// with their Gaussian PDF weights, and per-pixel rotation by interleaved gradient noise -
// both adapted from Bevy (github.com/bevyengine/bevy,
// crates/bevy_pbr/src/render/shadow_sampling.wgsl, MIT OR Apache-2.0). Bevy cites
// https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ne-d3d11-d3d11_standard_multisample_quality_levels
// for the positions. Weights are NOT normalized in Bevy's table (they sum ~1.0); we normalize.
pub const DISK_TAPS:[(f64,f64);8]=[(0.125,-0.375),(-0.125,0.375),(0.625,0.125),(-0.375,-0.625),(-0.625,0.625),(-0.875,-0.125),(0.375,0.875),(0.875,-0.875)];
pub const DISK_WEIGHTS:[f64;8]=[0.157112,0.157112,0.138651,0.130251,0.114946,0.110269,0.098503,0.093156];
/// Jimenez 2014 interleaved gradient noise - stable per-pixel rotation seed (no temporal crawl in stills).
pub fn interleaved_gradient_noise(x:usize,y:usize)->f64{let v=52.9829189*(0.06711056*x as f64+0.00583715*y as f64).fract();v.fract()}
/// visibility() with a rotated Gaussian disk instead of the 3x3 grid: same bias contract,
/// same compare, dithered soft edges. radius_texels scales the disk in map texels.
pub fn visibility_disk(map:&DirectionalShadowMap,world:[f64;3],normal:[f64;3],n_dot_l:f64,radius_texels:f64,px:usize,py:usize)->f64{
 if n_dot_l<=0.||radius_texels<=0.{return if n_dot_l<=0.{0.}else{visibility(map,world,normal,n_dot_l,0)};}
 let(no,dnb)=bias(map,n_dot_l);
 let p=std::array::from_fn(|j|world[j]+normal[j]*no);
 let Some((u,v,d))=shadow_uvz(map,p) else{return 1.};
 let d=d-dnb;
 let angle=interleaved_gradient_noise(px,py)*std::f64::consts::TAU;let(sn,cs)=angle.sin_cos();
 let(du,dv)=(1./map.width as f64,1./map.height as f64);
 let(mut lit,mut wsum)=(0.,0.);
 for k in 0..8{let(tx,ty)=DISK_TAPS[k];let rx=cs*tx-sn*ty;let ry=sn*tx+cs*ty;
  let stored=sample_map(map,u+rx*radius_texels*du,v+ry*radius_texels*dv);
  lit+=DISK_WEIGHTS[k]*if d<=stored{1.}else{0.};wsum+=DISK_WEIGHTS[k];}
 lit/wsum}
fn sample_map(map:&DirectionalShadowMap,u:f64,v:f64)->f64{
 if !(0.0..=1.0).contains(&u)||!(0.0..=1.0).contains(&v){return f64::INFINITY;} // off-map taps count lit
 // pinned texel addressing: identical to visibility()/raster_depth (u*width truncated)
 let x=(u*map.width as f64).min(map.width as f64-1e-9) as usize;
 let y=(v*map.height as f64).min(map.height as f64-1e-9) as usize;
 map.depth[y*map.width+x]}

/// CPU light-POV depth rasterizer: world-space triangles -> nearest normalized depth.
/// Same edge-function convention as lit_raster (top-left fill via positive barycentrics).
pub fn raster_depth(positions:&[[f64;3]],indices:&[[u32;3]],map:&mut DirectionalShadowMap)->Result<(),String>{
 if map.width==0||map.height==0||map.width>4096||map.height>4096{return Err("shadow map 1..4096 per axis".into());}
 if map.depth.len()!=map.width*map.height{return Err("shadow map depth buffer size mismatch".into());}
 for d in map.depth.iter_mut(){*d=f64::INFINITY;}
 raster_depth_into(positions,indices,map)}
/// Accumulate one draw's casters into the map WITHOUT clearing (nearest wins across calls).
/// raster_depth = clear + into; build multi-draw maps with one clear then into() per draw.
pub fn raster_depth_into(positions:&[[f64;3]],indices:&[[u32;3]],map:&mut DirectionalShadowMap)->Result<(),String>{
 if map.width==0||map.height==0||map.width>4096||map.height>4096{return Err("shadow map 1..4096 per axis".into());}
 if map.depth.len()!=map.width*map.height{return Err("shadow map depth buffer size mismatch".into());}
 let edge=|a:[f64;2],b:[f64;2],x:f64,y:f64|(b[0]-a[0])*(y-a[1])-(b[1]-a[1])*(x-a[0]);
 for t in indices{
  let mut p=[[0.;3];3];
  for k in 0..3{let w=positions[t[k] as usize];let q=mv(&map.view,[w[0],w[1],w[2],1.]);
   p[k]=[(q[0]-map.bounds[0][0])/(map.bounds[1][0]-map.bounds[0][0])*map.width as f64,
         (q[1]-map.bounds[0][1])/(map.bounds[1][1]-map.bounds[0][1])*map.height as f64,
         (q[2]-map.bounds[0][2])/(map.bounds[1][2]-map.bounds[0][2])];}
  let area=edge([p[0][0],p[0][1]],[p[1][0],p[1][1]],p[2][0],p[2][1]);
  if !area.is_finite(){return Err("shadow triangle area overflow".into());}
  if area==0.{continue;}
  let(a,b,c)=([p[0][0],p[0][1]],[p[1][0],p[1][1]],[p[2][0],p[2][1]]);
  // rasterize both windings: casters are single- or double-sided, shadow depth is winding-agnostic
  let x0=p.iter().map(|q|q[0]).fold(f64::INFINITY,f64::min).floor().clamp(0.,map.width as f64) as usize;
  let x1=p.iter().map(|q|q[0]).fold(f64::NEG_INFINITY,f64::max).ceil().clamp(0.,map.width as f64) as usize;
  let y0=p.iter().map(|q|q[1]).fold(f64::INFINITY,f64::min).floor().clamp(0.,map.height as f64) as usize;
  let y1=p.iter().map(|q|q[1]).fold(f64::NEG_INFINITY,f64::max).ceil().clamp(0.,map.height as f64) as usize;
  for y in y0..y1{for x in x0..x1{let(sx,sy)=(x as f64+0.5,y as f64+0.5);
   let(w0,w1,w2)=(edge(b,c,sx,sy)/area,edge(c,a,sx,sy)/area,edge(a,b,sx,sy)/area);
   if (w0<0.||w1<0.||w2<0.)&&(w0>0.||w1>0.||w2>0.){continue;} // outside (either winding)
   let d=w0*p[0][2]+w1*p[1][2]+w2*p[2][2];
   if !d.is_finite(){continue;}
   let id=y*map.width+x;if d<map.depth[id]{map.depth[id]=d;}}}
 }
 Ok(())}

/// Combined world-space AABB over draw position arrays.
pub fn scene_aabb(draws:&[&[[f64;3]]])->Result<[[f64;3];2],String>{
 let(mut lo,mut hi)=([f64::INFINITY;3],[f64::NEG_INFINITY;3]);let mut n=0usize;
 for ps in draws.iter().copied(){for p in ps{if p.iter().any(|v|!v.is_finite()){return Err("finite positions required".into());}for j in 0..3{lo[j]=lo[j].min(p[j]);hi[j]=hi[j].max(p[j]);}n+=1;}}
 if n==0{return Err("at least one vertex required".into());}
 Ok([lo,hi])}


// ---------------------------------------------------------------------------
// Point-light cube shadow maps (PL2). Six 90-degree persp depth maps centered on the
// light. Face depth is stored as face-space forward distance in WORLD units (not
// normalized): the receiver's own face-space z comes from the same transform, so the
// compare needs no range conversion and the world-unit bias contract applies on every
// face (same pinned constants as the directional maps), with ONE perspective-specific
// change: the receiver acne offset runs ALONG THE LIGHT RAY (toward the light), not
// along the normal. Under a face's 90-degree perspective projection a normal offset
// shortens the forward distance and shifts the angular sample coordinate outward,
// leaking light just inside silhouettes (measured: a receiver 0.01 face-units inside a
// quad silhouette sampled lit with a 1-texel normal offset); the ray offset preserves
// the angular position exactly while still clearing self-depth. Face order +X,-X,
// +Y,-Y,+Z,-Z; face bases share the directional_light_space convention (r = f x seed,
// u = r x f, z along f). The raycast reference (textured_bvh point shadow rays) matches
/// cube_visibility radius 0 exactly (its shadow rays are hard).
#[derive(Clone,Debug)]
pub struct PointCubeShadowMap{
 pub position:[f64;3],
 pub views:[Matrix;6], // world -> face view (x right, y up, z along face forward), origin at the light
 pub near:f64,pub far:f64, // world units; far covers the scene AABB from the light
 pub size:usize, // faces are square
 pub depth:Vec<f64>, // 6*size*size; f64::INFINITY = no occluder
}

/// Borrowed shadow map of either kind. lit_raster::render_shadowed matches the kind
/// against the light kind (mismatch is a typed error at validation).
#[derive(Clone,Copy,Debug)]
pub enum ShadowMapRef<'a>{Directional(&'a DirectionalShadowMap),PointCube(&'a PointCubeShadowMap),Cascaded(&'a super::csm::CascadedShadowMaps)}

pub const CUBE_FACE_FORWARD:[[f64;3];6]=[[1.,0.,0.],[-1.,0.,0.],[0.,1.,0.],[0.,-1.,0.],[0.,0.,1.],[0.,0.,-1.]];
pub const CUBE_FACE_SEED:[[f64;3];6]=[[0.,1.,0.],[0.,1.,0.],[0.,0.,-1.],[0.,0.,1.],[0.,1.,0.],[0.,1.,0.]];

/// Light space for a point light: 6 face views + near/far from the scene AABB.
/// near = max(min_distance, epsilon) so casters closer than the falloff floor are
/// clipped, matching shade()'s min_distance clamp; far covers every AABB corner + pad.
pub fn point_cube_light_space(position:[f64;3],aabb:[[f64;3];2],min_distance:f64)->Result<([Matrix;6],f64,f64),String>{
 if position.iter().chain(aabb.iter().flatten()).any(|v|!v.is_finite()){return Err("finite light position and scene bounds required".into());}
 if !min_distance.is_finite()||min_distance<=0.{return Err("positive finite min_distance required".into());}
 let mut views=[[[0.;4];4];6];
 for i in 0..6{
  let f=CUBE_FACE_FORWARD[i]; // already unit
  let r=unit(cross(f,CUBE_FACE_SEED[i]))?;let u=cross(r,f);
  views[i]=[[r[0],r[1],r[2],-(r[0]*position[0]+r[1]*position[1]+r[2]*position[2])],
            [u[0],u[1],u[2],-(u[0]*position[0]+u[1]*position[1]+u[2]*position[2])],
            [f[0],f[1],f[2],-(f[0]*position[0]+f[1]*position[1]+f[2]*position[2])],
            [0.,0.,0.,1.]];
 }
 let rad=(0..3).map(|j|(aabb[1][j]-aabb[0][j])/2.).fold(0.,|s,d|s+d*d).sqrt();
 let near=min_distance.max(1e-6*(1.+rad));
 let mut far2=0f64;
 for k in 0..8{let p=[if k&1==0{aabb[0][0]}else{aabb[1][0]},if k&2==0{aabb[0][1]}else{aabb[1][1]},if k&4==0{aabb[0][2]}else{aabb[1][2]}];
  let d=sub(p,position);far2=far2.max(d[0]*d[0]+d[1]*d[1]+d[2]*d[2]);}
 let far=far2.sqrt()+1e-3*(1.+rad);
 if far<=near{return Err("scene AABB collapses onto the light position".into());}
 Ok((views,near,far))
}

/// world -> (face, u, v, face-z in world units). Face = major axis of the light->point
/// direction, so u,v always land in [0,1] for z>0.
pub fn cube_uvz(map:&PointCubeShadowMap,world:[f64;3])->(usize,f64,f64,f64){
 let d=sub(world,map.position);
 let(ax,ay,az)=(d[0].abs(),d[1].abs(),d[2].abs());
 let axis=if ax>=ay&&ax>=az{0}else if ay>=az{1}else{2};
 let face=axis*2+usize::from(d[axis]<0.);
 let q=mv(&map.views[face],[world[0],world[1],world[2],1.]);
 (face,(q[0]/q[2]+1.)/2.,(q[1]/q[2]+1.)/2.,q[2])
}

/// World size of one face texel at distance z (90-degree faces: full width spans 2z).
fn cube_texel_world(map:&PointCubeShadowMap,z:f64)->f64{2.*z.max(map.near)/map.size as f64}
/// Pinned bias contract, world units: same constants as directional bias().
pub fn cube_bias(map:&PointCubeShadowMap,z:f64,n_dot_l:f64)->(f64,f64){
 let w=cube_texel_world(map,z);
 let slope=((1.-n_dot_l*n_dot_l).max(0.)).sqrt()/n_dot_l.max(1e-3);
 let slope=slope.min(4.);
 (NORMAL_OFFSET_TEXELS*w,(BIAS_CONST_TEXELS+BIAS_SLOPE_TEXELS*slope)*w)}

fn cube_sample(map:&PointCubeShadowMap,face:usize,u:f64,v:f64)->f64{
 if !(0.0..=1.0).contains(&u)||!(0.0..=1.0).contains(&v){return f64::INFINITY;} // off-map taps count lit
 // pinned texel addressing: identical to the directional maps (u*size truncated)
 let x=(u*map.size as f64).min(map.size as f64-1e-9) as usize;
 let y=(v*map.size as f64).min(map.size as f64-1e-9) as usize;
 map.depth[(face*map.size+y)*map.size+x]}

/// Local face-space depth gradient at a texel, world units per texel (max of the two
/// axis magnitudes, central differences over finite neighbors; 0 when unknown). This is
/// the perspective-cube analogue of hardware slope-scaled depth bias (the Vulkan/D3D
/// fixed-function max|dz/dx|,|dz/dy| semantics; algorithm only, no code taken): an
/// ortho map's nl-scaled constant bias covers a plane's UNIFORM depth gradient, but on
/// a 90-degree face the gradient grows without bound as a receiver plane turns edge-on
/// to the face, so multi-texel PCF/disk taps walking down-gradient self-shadow (measured
/// on the PL2 parity fixture: exactly one 8-tap disk tap shadowed on lit far-field
/// ground, r_ratio pinned at 7/8). The gradient allowance restores them without
/// touching the umbra (the caster's depth gap there is far larger than grad*tap_dist).
fn cube_depth_gradient(map:&PointCubeShadowMap,face:usize,u:f64,v:f64)->f64{
 let du=1./map.size as f64;
 let diff=|a:f64,b:f64|if a.is_finite()&&b.is_finite(){(a-b).abs()/2.}else{0.};
 let gx=diff(cube_sample(map,face,u+du,v),cube_sample(map,face,u-du,v));
 let gy=diff(cube_sample(map,face,u,v+du),cube_sample(map,face,u,v-du));
 gx.max(gy)}

/// PCF visibility in [0,1]; radius 0 = single-tap hard shadow (matches the shadow-ray
/// reference). The acne offset moves the receiver along the light ray, so face/u/v
/// never shift (see the PL2 section header); `normal` is kept for the shared call
/// contract and enters the bias only through n_dot_l.
pub fn cube_visibility(map:&PointCubeShadowMap,world:[f64;3],normal:[f64;3],n_dot_l:f64,radius:i32)->f64{
 let _=normal; // offset is ray-based; the normal enters the bias via n_dot_l (see doc)
 if map.size==0||map.depth.len()!=6*map.size*map.size{return 1.;}
 let(_,_,_,z0)=cube_uvz(map,world);
 if z0<=map.near{return 1.;} // receiver inside the near clip region: nothing can occlude
 let(no,db)=cube_bias(map,z0,n_dot_l);
 let Ok(ray)=unit(sub(world,map.position))else{return 1.;}; // degenerate/non-finite receiver: lit (pinned)
 let p=std::array::from_fn(|j|world[j]-ray[j]*no);
 let(face,u,v,z)=cube_uvz(map,p);
 if z<=map.near{return 1.;}
 let d=z-db;
 if !d.is_finite(){return 1.;}
 let du=1./map.size as f64;
 let g=if radius>0{cube_depth_gradient(map,face,u,v)}else{0.};
 let(mut lit,mut taps)=(0usize,0usize);
 for oy in -radius..=radius{for ox in -radius..=radius{
  let stored=cube_sample(map,face,u+ox as f64*du,v+oy as f64*du);
  let td=((ox*ox+oy*oy)as f64).sqrt();
  if stored==f64::INFINITY||d<=stored+g*td{lit+=1;}
  taps+=1;}}
 lit as f64/taps as f64}

/// cube_visibility with the rotated Gaussian disk (same D3D taps/IGN as the directional
/// disk): dithered soft edges, deterministic per pixel.
pub fn cube_visibility_disk(map:&PointCubeShadowMap,world:[f64;3],normal:[f64;3],n_dot_l:f64,radius_texels:f64,px:usize,py:usize)->f64{
 if n_dot_l<=0.||radius_texels<=0.{return if n_dot_l<=0.{0.}else{cube_visibility(map,world,normal,n_dot_l,0)};}
 if map.size==0||map.depth.len()!=6*map.size*map.size{return 1.;}
 let(_,_,_,z0)=cube_uvz(map,world);
 if z0<=map.near{return 1.;}
 let(no,db)=cube_bias(map,z0,n_dot_l);
 let Ok(ray)=unit(sub(world,map.position))else{return 1.;};
 let p=std::array::from_fn(|j|world[j]-ray[j]*no);
 let(face,u,v,z)=cube_uvz(map,p);
 if z<=map.near{return 1.;}
 let d=z-db;
 if !d.is_finite(){return 1.;}
 let angle=interleaved_gradient_noise(px,py)*std::f64::consts::TAU;let(sn,cs)=angle.sin_cos();
 let du=1./map.size as f64;
 let(mut lit,mut wsum)=(0.,0.);
 let g=cube_depth_gradient(map,face,u,v);
 for k in 0..8{let(tx,ty)=DISK_TAPS[k];let rx=cs*tx-sn*ty;let ry=sn*tx+cs*ty;
  let stored=cube_sample(map,face,u+rx*radius_texels*du,v+ry*radius_texels*du);
  let td=radius_texels*(rx*rx+ry*ry).sqrt();
  lit+=DISK_WEIGHTS[k]*if d<=stored+g*td{1.}else{0.};wsum+=DISK_WEIGHTS[k];}
 lit/wsum}

/// CPU light-POV cube depth: rasterize all 6 faces (nearest wins), clearing first.
/// Persp-correct depth: 1/z is affine in screen space, so per-texel z = 1/(sum b_i/z_i).
pub fn raster_cube_depth(positions:&[[f64;3]],indices:&[[u32;3]],map:&mut PointCubeShadowMap)->Result<(),String>{
 if map.size==0||map.size>4096{return Err("cube shadow map 1..4096 per face axis".into());}
 if map.depth.len()!=6*map.size*map.size{return Err("cube shadow depth buffer size mismatch".into());}
 for d in map.depth.iter_mut(){*d=f64::INFINITY;}
 raster_cube_depth_into(positions,indices,map)}
/// Accumulate one draw's casters into all faces WITHOUT clearing (nearest wins across calls).
pub fn raster_cube_depth_into(positions:&[[f64;3]],indices:&[[u32;3]],map:&mut PointCubeShadowMap)->Result<(),String>{
 if map.size==0||map.size>4096{return Err("cube shadow map 1..4096 per face axis".into());}
 if map.depth.len()!=6*map.size*map.size{return Err("cube shadow depth buffer size mismatch".into());}
 let edge=|a:[f64;2],b:[f64;2],x:f64,y:f64|(b[0]-a[0])*(y-a[1])-(b[1]-a[1])*(x-a[0]);
 for face in 0..6{
  let base=face*map.size*map.size;
  for t in indices{
   // world -> face view, then near clip (the camera's tested view-space clipper)
   let tri:[crate::camera_projection::Vertex;3]=std::array::from_fn(|k|{
    let w=positions[t[k] as usize];let q=mv(&map.views[face],[w[0],w[1],w[2],1.]);
    crate::camera_projection::Vertex{position:[q[0],q[1],q[2]],attributes:vec![]}});
   for t2 in crate::camera_projection::clip(tri,map.near)?{
    let sz=map.size as f64;
    let p:[[f64;3];3]=std::array::from_fn(|k|[(t2[k].position[0]/t2[k].position[2]+1.)*0.5*sz,(t2[k].position[1]/t2[k].position[2]+1.)*0.5*sz,t2[k].position[2]]);
    let area=edge([p[0][0],p[0][1]],[p[1][0],p[1][1]],p[2][0],p[2][1]);
    if !area.is_finite(){return Err("cube shadow triangle area overflow".into());}
    if area==0.{continue;}
    let(a,b,c)=([p[0][0],p[0][1]],[p[1][0],p[1][1]],[p[2][0],p[2][1]]);
    // rasterize both windings: casters are single- or double-sided, depth is winding-agnostic
    let x0=p.iter().map(|q|q[0]).fold(f64::INFINITY,f64::min).floor().clamp(0.,sz) as usize;
    let x1=p.iter().map(|q|q[0]).fold(f64::NEG_INFINITY,f64::max).ceil().clamp(0.,sz) as usize;
    let y0=p.iter().map(|q|q[1]).fold(f64::INFINITY,f64::min).floor().clamp(0.,sz) as usize;
    let y1=p.iter().map(|q|q[1]).fold(f64::NEG_INFINITY,f64::max).ceil().clamp(0.,sz) as usize;
    for y in y0..y1{for x in x0..x1{let(sx,sy)=(x as f64+0.5,y as f64+0.5);
     let(w0,w1,w2)=(edge(b,c,sx,sy)/area,edge(c,a,sx,sy)/area,edge(a,b,sx,sy)/area);
     if (w0<0.||w1<0.||w2<0.)&&(w0>0.||w1>0.||w2>0.){continue;} // outside (either winding)
     let inv_z=w0/p[0][2]+w1/p[1][2]+w2/p[2][2];
     if inv_z<=0.||!inv_z.is_finite(){continue;}
     let z=1.0/inv_z;
     let id=base+y*map.size+x;if z<map.depth[id]{map.depth[id]=z;}}}
   }
  }
 }
 Ok(())}

#[cfg(test)]mod tests{use super::*;
 fn box_aabb()->[[f64;3];2]{[[-1.;3],[1.;3]]}
 #[test]fn light_space_covers_aabb_and_is_deterministic(){
  let(v1,b1)=directional_light_space([0.3,-0.5,1.],box_aabb()).unwrap();
  let(v2,b2)=directional_light_space([0.3,-0.5,1.],box_aabb()).unwrap();
  assert_eq!(v1,v2);assert_eq!(b1,b2);
  let map=DirectionalShadowMap{view:v1,bounds:b1,width:4,height:4,depth:vec![f64::INFINITY;16]};
  for k in 0..8{let p=[if k&1==0{-1.}else{1.},if k&2==0{-1.}else{1.},if k&4==0{-1.}else{1.}];
   let(u,v,d)=shadow_uvz(&map,p).expect("every AABB corner inside the map");
   assert!((0.0..=1.0).contains(&u)&&(0.0..=1.0).contains(&v)&&(0.0..=1.0).contains(&d),"corner {k} -> {u} {v} {d}");}
 }
 #[test]fn bias_contract_zero_front_on_grows_grazing(){
  let(view,bounds)=directional_light_space([0.,0.,1.],box_aabb()).unwrap();
  let map=DirectionalShadowMap{view,bounds,width:64,height:64,depth:vec![f64::INFINITY;64*64]};
  let(no0,db0)=bias(&map,1.0);let(_,db1)=bias(&map,0.5);let(_,db2)=bias(&map,0.1);
  assert!(no0>0.,"normal offset always on");
  assert!(db0>0.&&db1>db0&&db2>db1,"bias grows with grazing: {db0} {db1} {db2}");
  let(_,dbt)=bias(&map,0.0);let(_,dbc)=bias(&map,1e-9);assert_eq!(dbt,dbc,"slope clamped: nl=0 equals nl~0");}
 #[test]fn visibility_hard_and_pcf_penumbra(){
  // light travels +z: smaller world z = nearer the light. Occluder quad at z=0, half the map (x<0).
  let(view,bounds)=directional_light_space([0.,0.,1.],[[-2.,-2.,-1.],[2.,2.,1.]]).unwrap();
  let mut map=DirectionalShadowMap{view,bounds,width:64,height:64,depth:vec![f64::INFINITY;64*64]};
  let pos=vec![[-2.,-2.,0.],[0.,-2.,0.],[0.,2.,0.],[-2.,2.,0.]];
  let idx=vec![[0u32,1,2],[0,2,3]];
  raster_depth(&pos,&idx,&mut map).unwrap();
  let behind=visibility(&map,[-1.,0.,0.5],[0.,0.,-1.],1.0,0);let front=visibility(&map,[-1.,0.,-0.5],[0.,0.,-1.],1.0,0);
  assert_eq!((behind,front),(0.0,1.0),"hard shadow: behind occluder {behind}, in front {front}");
  let behind_g=visibility(&map,[-1.,0.,0.5],[0.,0.,-1.],0.8,0);assert_eq!(behind_g,0.0,"bias must not erase a real shadow at moderate incidence");
  // documented tradeoff: at grazing incidence (nl=0.3) the pinned slope bias CAN erase a
  // shallow shadow on a small map - the acne/peter-panning tradeoff is deliberate.
  assert_eq!(visibility(&map,[-1.,0.,0.5],[0.,0.,-1.],0.3,0),1.0,"grazing bias erasure is the pinned contract (documented)");
  // no self-shadow acne: a receiver ON the occluder plane stays lit under its own depth
  let on=visibility(&map,[-1.,0.,0.],[0.,0.,-1.],1.0,0);assert_eq!(on,1.0,"bias prevents acne on the occluder plane");
  // open half of the map: fully lit
  let open=visibility(&map,[1.,0.,0.5],[0.,0.,-1.],1.0,0);assert_eq!(open,1.0,"no occluder texel -> lit");
  // penumbra at the occluder silhouette (x=0): hard is binary, PCF straddles
  let edge_x=0.5/64.; // half a texel inside the covered half
  let hard=visibility(&map,[-edge_x,0.,0.5],[0.,0.,-1.],1.0,0);
  let soft=visibility(&map,[-edge_x,0.,0.5],[0.,0.,-1.],1.0,1);
  assert_eq!(hard,0.0,"single tap binary at the edge");
  assert!(soft>0.0&&soft<1.0,"PCF penumbra strictly between: {soft}");
 }
 #[test]fn raster_depth_nearest_wins_and_empty_is_infinite(){
  let(view,bounds)=directional_light_space([0.,0.,1.],[[-2.;3],[2.;3]]).unwrap();
  let mut map=DirectionalShadowMap{view,bounds,width:32,height:32,depth:vec![f64::INFINITY;32*32]};
  raster_depth(&[[0.,0.,0.],[0.1,0.,0.],[0.,0.1,0.]],&[[0u32,1,2]],&mut map).unwrap();
  assert!(map.depth.iter().all(|d|*d==f64::INFINITY),"sub-texel triangle may cover nothing; all empty stays infinite");
  // two overlapping quads: nearer (smaller light-z) must win
  let(view,bounds)=directional_light_space([0.,0.,1.],[[-2.;3],[2.;3]]).unwrap();
  let mut map=DirectionalShadowMap{view,bounds,width:32,height:32,depth:vec![f64::INFINITY;32*32]};
  let far=vec![[-1.,-1.,0.5],[1.,-1.,0.5],[1.,1.,0.5],[-1.,1.,0.5]];
  let near=vec![[-1.,-1.,-0.5],[1.,-1.,-0.5],[1.,1.,-0.5],[-1.,1.,-0.5]];
  let idx=vec![[0u32,1,2],[0,2,3]];
  raster_depth(&far,&idx,&mut map).unwrap();
  raster_depth(&near,&idx,&mut map).unwrap();
  let d=*map.depth.iter().filter(|d|**d!=f64::INFINITY).next().expect("quads rasterized");
  let(_,_,expect)=shadow_uvz(&map,[0.,0.,-0.5]).unwrap(); // same transform the rasterizer applies
  assert!((d-expect).abs()<1e-12,"nearest occluder depth {d} vs {expect}");
 }

}


#[cfg(test)]mod cube_tests{use super::*;
 fn scene_box()->[[f64;3];2]{[[-4.;3],[4.;3]]}
 fn cube_map(position:[f64;3],size:usize)->PointCubeShadowMap{let(views,near,far)=point_cube_light_space(position,scene_box(),0.05).unwrap();PointCubeShadowMap{position,views,near,far,size,depth:vec![f64::INFINITY;6*size*size]}}
 fn quad_on_axis(f:[f64;3],dist:f64,half:f64)->(Vec<[f64;3]>,Vec<[u32;3]>){
  // quad perpendicular to f at distance `dist` from origin, half-extent `half`
  let t1=if f[1].abs()<0.9{[0.,1.,0.]}else{[1.,0.,0.]};let r=unit(cross(f,t1)).unwrap();let u=cross(r,f);
  let c=[f[0]*dist,f[1]*dist,f[2]*dist];
  let corner=|s1:f64,s2:f64|{[c[0]+s1*r[0]+s2*u[0],c[1]+s1*r[1]+s2*u[1],c[2]+s1*r[2]+s2*u[2]]};
  (vec![corner(-half,-half),corner(half,-half),corner(half,half),corner(-half,half)],vec![[0u32,1,2],[0,2,3]])}
 #[test]fn face_selection_is_major_axis_and_centers(){
  let map=cube_map([0.,0.,0.],16);
  for i in 0..6{let f=CUBE_FACE_FORWARD[i];
   let(face,u,v,z)=cube_uvz(&map,[f[0]*2.,f[1]*2.,f[2]*2.]);
   assert_eq!(face,i,"direction of face {i} selects itself");
   assert!((u-0.5).abs()<1e-12&&(v-0.5).abs()<1e-12,"face center projects to center: {u} {v}");
   assert!((z-2.).abs()<1e-12,"face z is world distance: {z}");}
  let(_f,u,v,z)=cube_uvz(&map,[1.,1.,1.]);
  assert!((0.0..=1.0).contains(&u)&&(0.0..=1.0).contains(&v)&&z>0.,"diagonal lands inside its major face");}
 #[test]fn light_space_deterministic_and_far_covers_aabb(){
  let p=[0.3,-0.2,0.1];
  let a=point_cube_light_space(p,scene_box(),0.05).unwrap();let b=point_cube_light_space(p,scene_box(),0.05).unwrap();
  assert_eq!(a.0,b.0);assert_eq!((a.1,a.2),(b.1,b.2));
  let map=PointCubeShadowMap{position:p,views:a.0,near:a.1,far:a.2,size:4,depth:vec![f64::INFINITY;6*16]};
  for k in 0..8{let corner=[if k&1==0{-4.}else{4.},if k&2==0{-4.}else{4.},if k&4==0{-4.}else{4.}];
   let(_,_,_,z)=cube_uvz(&map,corner);assert!(z<a.2,"every AABB corner inside far: {z} < {}",a.2);}
  assert!(point_cube_light_space([0.;3],scene_box(),0.).is_err(),"non-positive min_distance is a typed error");
  assert!(point_cube_light_space([f64::NAN;3],scene_box(),0.05).is_err(),"non-finite position is a typed error");}
 #[test]fn occluder_blocks_all_six_directions_and_bias_prevents_acne(){
  for i in 0..6{
   let f=CUBE_FACE_FORWARD[i];
   let(pos,idx)=quad_on_axis(f,1.,1.);
   let mut map=cube_map([0.,0.,0.],64);
   raster_cube_depth(&pos,&idx,&mut map).unwrap();
   let n=[-f[0],-f[1],-f[2]]; // receiver normal faces the light
   let recv=[f[0]*3.,f[1]*3.,f[2]*3.];
   assert_eq!(cube_visibility(&map,recv,n,1.0,0),0.0,"face {i}: behind the occluder is fully shadowed");
   // receiver ON the occluder plane stays lit under its own depth (no self-shadow acne)
   assert_eq!(cube_visibility(&map,[f[0],f[1],f[2]],n,1.0,0),1.0,"face {i}: acne guard on the occluder plane");}
 }
 #[test]fn nearest_wins_across_draws_and_empty_map_is_lit(){
  let mut map=cube_map([0.,0.,0.],32);
  assert_eq!(cube_visibility(&map,[2.,0.,0.],[-1.,0.,0.],1.0,0),1.0,"empty map is lit");
  let(far_pos,idx)=quad_on_axis([1.,0.,0.],2.,0.5);
  raster_cube_depth(&far_pos,&idx,&mut map).unwrap();
  let(near_pos,idx2)=quad_on_axis([1.,0.,0.],1.,0.5);
  raster_cube_depth_into(&near_pos,&idx2,&mut map).unwrap();
  let center=map.depth[0*32*32+16*32+16];
  assert!((center-1.).abs()<1e-9,"nearer caster wins: center texel {center}");
  assert_eq!(cube_visibility(&map,[3.,0.,0.],[-1.,0.,0.],1.0,0),0.0,"both quads occlude");}
 #[test]fn hard_binary_and_pcf_disk_penumbra_at_silhouette(){
  // small quad (half 0.5 at distance 1) covers x/z in [-0.5,0.5] on the +X face;
  // silhouette sits at x/z=0.5 for receivers on the ground line of the quad plane.
  let(pos,idx)=quad_on_axis([1.,0.,0.],1.,0.5);
  let mut map=cube_map([0.,0.,0.],64);
  raster_cube_depth(&pos,&idx,&mut map).unwrap();
  // receiver at distance 3, offset along the quad's r axis so x/z = 0.49 (just inside)
  let f=[1.,0.,0.];let r=unit(cross(f,[0.,1.,0.])).unwrap();
  let recv=[3.,0.,0.];let in_edge=[recv[0]+0.*r[0],0.,0.];
  let _=in_edge;
  let p_in=[3.,1.47,0.]; // r axis for +X face: r = f x [0,1,0] = (0,0,1)... use y instead: offset along y
  let _=p_in;
  // offset along world y: face view y axis for +X face is world y (u = r x f = (0,0,1) x? pinned by basis test above)
  let inside=[3.,1.47,0.]; // x/z_face = 1.47/3 = 0.49 < 0.5: inside the silhouette
  let n=[-1.,0.,0.];
  let hard=cube_visibility(&map,inside,n,1.0,0);
  assert_eq!(hard,0.0,"hard tap inside the silhouette: {hard}");
  let soft=cube_visibility(&map,inside,n,1.0,1);
  assert!(soft>0.0&&soft<1.0,"3x3 PCF straddles the silhouette: {soft}");
  let disk=cube_visibility_disk(&map,inside,n,1.0,4.0,7,11);
  assert!(disk>0.0&&disk<1.0,"disk penumbra strictly between: {disk}");
  let disk_hard=cube_visibility_disk(&map,inside,n,1.0,0.0,7,11);
  assert_eq!(disk_hard,hard,"disk radius 0 is the hard tap");
  // fully inside and fully outside stay extreme
  assert_eq!(cube_visibility(&map,[3.,0.,0.],n,1.0,0),0.0,"deep inside the shadow");
  assert_eq!(cube_visibility(&map,[3.,-2.4,0.],n,1.0,0),1.0,"outside the silhouette is lit");}
 #[test]fn raster_validation_is_typed(){
  let mut bad=cube_map([0.,0.,0.],0);
  assert_eq!(raster_cube_depth(&[[0.;3],[1.,0.,0.],[0.,1.,0.]],&[[0u32,1,2]],&mut bad).err().unwrap(),"cube shadow map 1..4096 per face axis");
  let mut bad2=cube_map([0.,0.,0.],8);bad2.depth=vec![0.;3];
  assert_eq!(raster_cube_depth(&[[0.;3],[1.,0.,0.],[0.,1.,0.]],&[[0u32,1,2]],&mut bad2).err().unwrap(),"cube shadow depth buffer size mismatch");}
}
