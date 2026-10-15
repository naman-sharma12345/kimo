//! SDF contour frame fixture (CP2 render half of double verification): the CP1
//! torus+capsule smooth-union and a sharp box polygonized via DualContouring+MinimiseQEF
//! (analytic Hermite normals), then rasterized through lit_raster (ACES PBR) for visual
//! sliver/flipped-normal inspection. Deterministic FNV-1a hash at 320x180; env-gated
//! 640x360 PNG dumps (KIMO_SDF_FRAMES). Topology census asserted closed+manifold.
#![cfg(feature="surface-contour")]
use kimo_core::threed::{sdf_graph::{Field,Graph},contour_wrapped::{self,Config},lit_raster,pbr::Material,lighting_stage::Light};
use kimo_core::camera_projection::Camera;

fn fnv(bytes:&[u8])->u64{let mut h=0xcbf29ce484222325u64;for b in bytes{h^=*b as u64;h=h.wrapping_mul(0x100000001b3);}h}

fn union_scene()->Graph{Graph{nodes:vec![
 Field::Torus{center:[0.,-0.15,0.],major_radius:1.05,minor_radius:0.42},
 Field::Capsule{a:[-0.85,-0.95,0.15],b:[0.85,0.95,-0.15],radius:0.34},
 Field::SmoothUnion{a:0,b:1,k:0.35}],output:2}}

fn camera(eye_scale:f64)->Camera{Camera{eye:[2.4*eye_scale,1.9*eye_scale,-3.4*eye_scale],pan:324.79,tilt:24.55,roll:0.,sensor_height:24.,focal_length:35.,near:0.1}}

fn contour_frame(g:&Graph,cfg:&Config,eye_scale:f64,w:usize,h:usize)->(kimo_core::effects::Frame,usize,contour_wrapped::Contour){
 let m=contour_wrapped::polygonize(g,cfg).unwrap();
 let material=Material{albedo:[0.74,0.68,0.6],metallic:0.,roughness:0.55};
 let lights=[Light::Directional{travel_direction:[-0.3,-0.5,1.],radiance:[2.5;3]}];
 let out=lit_raster::render(&m.stream,&camera(eye_scale),w,h,&material,&lights).unwrap();
 (out.frame,out.covered,m)
}

fn hash_of(f:&kimo_core::effects::Frame)->u64{let mut raw=Vec::with_capacity(f.d.len()*4);for v in &f.d{raw.extend_from_slice(&v.to_le_bytes());}fnv(&raw)}

fn dump(dir:&str,name:&str,g:&Graph,cfg:&Config,eye_scale:f64){
 let(frame,covered,m)=contour_frame(g,cfg,eye_scale,640,360);
 assert!(m.topology_verified,"dump scene must verify");
 let rgba=kimo_core::render::to_rgba8_tpdf(&frame);
 let mut ch=std::process::Command::new("ffmpeg").args(["-v","error","-y","-f","rawvideo","-pix_fmt","rgba","-s","640x360","-i","-","-frames:v","1",&format!("{dir}/{name}")]).stdin(std::process::Stdio::piped()).spawn().unwrap();
 std::io::Write::write_all(&mut ch.stdin.take().unwrap(),&rgba).unwrap();assert!(ch.wait().unwrap().success());
 println!("CONTOUR-FRAME {name} 640x360 covered={covered} dumped");
}

#[test]fn contour_union_frame_topology_and_hash(){
 let cfg=Config{bounds:[[-2.2;3],[2.2;3]],resolution:48,normal_epsilon:0.001};
 let(frame,covered,m)=contour_frame(&union_scene(),&cfg,2.0,320,180);
 println!("CONTOUR-FRAME union 320x180 covered={} hash={:016x} tris={} euler={} vol={:.3} residual={:.4} fb={} slivers={}",covered,hash_of(&frame),m.stream.indices.len(),m.euler_characteristic,m.signed_volume,m.sampled_field_residual_max,m.normal_fallbacks,m.sliver_triangles);
 // capsule threads the torus hole: genus-2 union, so Euler = 2-2g = -2 (measured, not assumed).
 assert!(m.topology_verified&&m.euler_characteristic==-2,"union contour census: v={} e={} b={} nm={} wind={} vol={}",m.topology_verified,m.euler_characteristic,m.boundary_edges,m.nonmanifold_edges,m.winding_consistent,m.signed_volume);
 assert_eq!((m.normal_fallbacks,m.sliver_triangles,m.clamped_to_volume),(0,0,0),"union telemetry");
 assert!(covered>2_500,"covered {covered}");
 const EXPECT:u64=0x4adbc2e2de1e7895;
 if EXPECT!=0{assert_eq!(hash_of(&frame),EXPECT,"contour union frame hash drift");}
 if let Ok(dir)=std::env::var("KIMO_SDF_FRAMES"){dump(&dir,"sdf-cp2-contour-union.png",&union_scene(),&cfg,2.0);}
}

#[test]fn contour_box_frame_sharp_normals_and_hash(){
 let g=Graph{nodes:vec![Field::Box{center:[0.;3],half_size:[0.6;3]}],output:0};
 let cfg=Config{bounds:[[-1.6;3],[1.6;3]],resolution:32,normal_epsilon:0.001};
 let(frame,covered,m)=contour_frame(&g,&cfg,1.0,320,180);
 println!("CONTOUR-FRAME box 320x180 covered={} hash={:016x} tris={} euler={} vol={:.3} fb={} slivers={}",covered,hash_of(&frame),m.stream.indices.len(),m.euler_characteristic,m.signed_volume,m.normal_fallbacks,m.sliver_triangles);
 assert!(m.topology_verified&&m.euler_characteristic==2,"box contour census: v={} e={} b={} nm={} wind={} vol={}",m.topology_verified,m.euler_characteristic,m.boundary_edges,m.nonmanifold_edges,m.winding_consistent,m.signed_volume);
 assert!(covered>2_000,"covered {covered}");
 const EXPECT:u64=0x7cfe3701651f4948;
 if EXPECT!=0{assert_eq!(hash_of(&frame),EXPECT,"contour box frame hash drift");}
 if let Ok(dir)=std::env::var("KIMO_SDF_FRAMES"){dump(&dir,"sdf-cp2-contour-box.png",&g,&cfg,1.0);}
}

// ---- CP3: named benchmark pinning TODAY's near-tangent blend crumple for the octree unit.
// These are known-bad artifact measurements, NOT correctness claims: when the octree
// adaptive DC unit lands, the >120deg fold counts should drop sharply and this test
// should be re-pinned then (and only then).
#[test]fn near_tangent_blend_benchmark_pins_current_artifact_for_octree(){
 fn stats(res:u32)->(usize,usize,usize,usize,f64,f64,usize,bool){
  let cfg=Config{bounds:[[-2.2;3],[2.2;3]],resolution:res,normal_epsilon:0.001};
  let m=contour_wrapped::polygonize(&union_scene(),&cfg).unwrap();
  let p:Vec<[f64;3]>=m.stream.vertices.iter().map(|v|v.position.map(|x|x as f64)).collect();
  let f=|t:[u32;3]|->[f64;3]{let[a,b,c]=t.map(|i|p[i as usize]);let ab=[b[0]-a[0],b[1]-a[1],b[2]-a[2]];let ac=[c[0]-a[0],c[1]-a[1],c[2]-a[2]];let n=[ab[1]*ac[2]-ab[2]*ac[1],ab[2]*ac[0]-ab[0]*ac[2],ab[0]*ac[1]-ab[1]*ac[0]];let l=(n[0]*n[0]+n[1]*n[1]+n[2]*n[2]).sqrt();if l>1e-300{[n[0]/l,n[1]/l,n[2]/l]}else{[0.,0.,0.]}};
  let fn_:Vec<_>=m.stream.indices.iter().map(|t|f(*t)).collect();
  let mut min_a=f64::INFINITY;
  for t in &m.stream.indices{let[a,b,c]=t.map(|i|p[i as usize]);let ab=[b[0]-a[0],b[1]-a[1],b[2]-a[2]];let ac=[c[0]-a[0],c[1]-a[1],c[2]-a[2]];let n=[ab[1]*ac[2]-ab[2]*ac[1],ab[2]*ac[0]-ab[0]*ac[2],ab[0]*ac[1]-ab[1]*ac[0]];let a2=0.5*(n[0]*n[0]+n[1]*n[1]+n[2]*n[2]).sqrt();if a2<min_a{min_a=a2;}}
  use std::collections::HashMap;
  let mut et:HashMap<(u32,u32),Vec<usize>>=HashMap::new();
  for(ti,t)in m.stream.indices.iter().enumerate(){for k in 0..3{let(a,b)=(t[k],t[(k+1)%3]);et.entry((a.min(b),a.max(b))).or_default().push(ti);}}
  let mut o60=0;let mut o120=0;
  for(_,ts)in &et{if ts.len()==2{let d=fn_[ts[0]][0]*fn_[ts[1]][0]+fn_[ts[0]][1]*fn_[ts[1]][1]+fn_[ts[0]][2]*fn_[ts[1]][2];let a=d.clamp(-1.,1.).acos().to_degrees();if a>60.{o60+=1;}if a>120.{o120+=1;}}}
  (m.stream.indices.len(),o60,o120,m.normal_fallbacks,m.sampled_field_residual_max,min_a,m.sliver_triangles,m.topology_verified)
 }
 let(t48,o60_48,o120_48,fb48,r48,ma48,sl48,v48)=stats(48);
 let(t64,o60_64,o120_64,fb64,r64,ma64,sl64,v64)=stats(64);
 println!("BENCHMARK near-tangent blend: res48 tris={t48} folds60={o60_48} folds120={o120_48} fb={fb48} resid={r48:.4} min_area={ma48:.3e}; res64 tris={t64} folds60={o60_64} folds120={o120_64} fb={fb64} resid={r64:.4} min_area={ma64:.3e}");
 assert_eq!((t48,o60_48,o120_48,fb48,sl48,v48),(7612,543,346,0,0,true),"res48 artifact benchmark drifted - re-pin ONLY with an octree-unit note");
 assert_eq!((t64,o60_64,o120_64,fb64,sl64,v64),(13608,482,288,2,0,true),"res64 artifact benchmark drifted - re-pin ONLY with an octree-unit note");
 assert!((0.059..0.061).contains(&r48)&&(0.021..0.023).contains(&r64),"residual benchmark drift");
 assert!((2.2e-8..2.3e-8).contains(&ma48)&&(2.2e-11..2.3e-11).contains(&ma64),"min-area benchmark drift");
}

// ---- CP3: render verification of the contour mesh against the sphere-traced field truth.
// Same camera both sides (view() Jacobian gives the exact world-from-view rotation, no
// convention guessing). Mesh normals come from render_normal (wgpu when a real adapter
// exists, else explicit CPU fallback with reported status); field-truth normals are
// sphere-traced analytic gradients. Compared as world-normal maps + silhouettes.
fn traced_normal_map(g:&Graph,cam:&Camera,w:usize,h:usize)->(Vec<[f64;3]>,Vec<bool>){
 let eye=cam.eye;
 let col=|d:[f64;3]|{let a=cam.view([eye[0]+d[0],eye[1]+d[1],eye[2]+d[2]]).unwrap();let b=cam.view(eye).unwrap();[a[0]-b[0],a[1]-b[1],a[2]-b[2]]};
 let c0=col([1.,0.,0.]);let c1=col([0.,1.,0.]);let c2=col([0.,0.,1.]);
 for(c,other)in[(&c0,&c1),(&c0,&c2),(&c1,&c2)]{let d=c[0]*other[0]+c[1]*other[1]+c[2]*other[2];assert!(d.abs()<1e-9,"view basis not orthogonal");}
 for c in[&c0,&c1,&c2]{let l=(c[0]*c[0]+c[1]*c[1]+c[2]*c[2]).sqrt();assert!((l-1.).abs()<1e-9,"view basis not unit");}
 // world dir = M^T * view dir (M orthogonal; columns are c0,c1,c2)
 let world=|v:[f64;3]|{[c0[0]*v[0]+c0[1]*v[1]+c0[2]*v[2],c1[0]*v[0]+c1[1]*v[1]+c1[2]*v[2],c2[0]*v[0]+c2[1]*v[1]+c2[2]*v[2]]};
 let fpx=h as f64*(cam.focal_length/cam.sensor_height);
 let mut normals=vec![[0.;3];w*h];let mut cov=vec![false;w*h];
 for y in 0..h{for x in 0..w{
  let vd=[((x as f64+0.5)-w as f64*0.5)/fpx,((y as f64+0.5)-h as f64*0.5)/fpx,1.];
  let wd=world(vd);let l=(wd[0]*wd[0]+wd[1]*wd[1]+wd[2]*wd[2]).sqrt();let dir=[wd[0]/l,wd[1]/l,wd[2]/l];
  let mut t=0.05;let mut hit=None;
  for _ in 0..400{let p=[eye[0]+t*dir[0],eye[1]+t*dir[1],eye[2]+t*dir[2]];let d=g.sample(p).unwrap();if d<2e-4*t.max(1.){hit=Some(p);break;}let step=(d*0.9).max(1e-4);t+=step;if t>24.{break;}}
  if let Some(p)=hit{
   let n=g.normal_analytic(p).unwrap_or_else(|_|{let e=1e-5;let mut nf=[0.;3];for j in 0..3{let mut a=p;let mut b=p;a[j]+=e;b[j]-=e;nf[j]=g.sample(a).unwrap()-g.sample(b).unwrap();}let l=(nf[0]*nf[0]+nf[1]*nf[1]+nf[2]*nf[2]).sqrt();[nf[0]/l,nf[1]/l,nf[2]/l]});
   normals[y*w+x]=n;cov[y*w+x]=true;
  }
 }}
 (normals,cov)
}
#[test]fn contour_render_matches_sphere_traced_field_truth(){
 let cfg=Config{bounds:[[-2.2;3],[2.2;3]],resolution:64,normal_epsilon:0.001};
 let m=contour_wrapped::polygonize(&union_scene(),&cfg).unwrap();
 let cam=camera(2.0);
 let(w,h)=(320usize,180usize);
 let(mf,status)=kimo_core::threed::offscreen_wgpu::render_normal(&m.stream,&cam,w,h,false,true).unwrap();
 println!("CONTOUR-VS-TRACER backend={} fallback={:?}",status.actual_backend,status.fallback_reason);
 let(tn,tcov)=traced_normal_map(&union_scene(),&cam,w,h);
 let mut inter=0usize;let mut union=0usize;let mut degs=Vec::new();
 for i in 0..w*h{
  let mc=mf.d[i*4+3]>0.5;let tc=tcov[i];
  if mc||tc{union+=1;}
  if mc&&tc{inter+=1;let mn=[mf.d[i*4]as f64*2.-1.,mf.d[i*4+1]as f64*2.-1.,mf.d[i*4+2]as f64*2.-1.];let d=mn[0]*tn[i][0]+mn[1]*tn[i][1]+mn[2]*tn[i][2];degs.push(d.clamp(-1.,1.).acos().to_degrees());}
 }
 degs.sort_by(|a,b|a.partial_cmp(b).unwrap());
 let med=degs[degs.len()/2];let p95=degs[(degs.len()as f64*0.95)as usize];let p99=degs[(degs.len()as f64*0.99)as usize];
 let iou=inter as f64/union as f64;
 println!("CONTOUR-VS-TRACER 320x180 iou={:.4} n={} median={:.2}deg p95={:.2}deg p99={:.2}deg",iou,degs.len(),med,p95,p99);
 assert!(iou>0.985,"silhouette IoU {iou}"); // measured 0.9905
 assert!(med<1.0,"median normal disagreement {med}deg"); // measured 0.40
 assert!(p95<6.0,"p95 normal disagreement {p95}deg"); // measured 3.38; tail is crease+silhouette
}
