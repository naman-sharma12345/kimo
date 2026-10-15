//! Hero pipeline fixture: lathe vase -> Catmull-Clark -> star extrusion -> boolean cut.
//! End-to-end through pub APIs only. Structural assertions always run at 320x180 with
//! FNV-1a frame hashes; 1080p PNG dumps are gated behind KIMO_PM_FRAMES.
//! Normals use a crease-angle split (smooth groups, 40deg threshold) so the boolean
//! seam does not smear shading across the ~90deg crease (census: median 95deg).
use kimo_core::threed::{boolean::{boolean,BoolOp},mesh::Mesh,perspective::{render_mesh,Shading},subdivide::catmull_clark,vertex_stream::Stream,lit_raster,lighting_stage::Light,pbr::Material,primitives::{generate,Primitive,Vertex}};
use kimo_core::camera_projection::Camera;

fn sub(a:[f64;3],b:[f64;3])->[f64;3]{[a[0]-b[0],a[1]-b[1],a[2]-b[2]]}
fn dot(a:[f64;3],b:[f64;3])->f64{a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
fn cross(a:[f64;3],b:[f64;3])->[f64;3]{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
fn vlen(a:[f64;3])->f64{dot(a,a).sqrt()}
fn face_n(m:&Mesh,t:[u32;3])->([f64;3],f64){let[a,b,c]=t.map(|i|m.positions[i as usize]);let n=cross(sub(b,a),sub(c,a));let l=vlen(n);if l>0.{([n[0]/l,n[1]/l,n[2]/l],l/2.)}else{([0.;3],0.)}}
fn fnv(bytes:&[u8])->u64{let mut h=0xcbf29ce484222325u64;for b in bytes{h^=*b as u64;h=h.wrapping_mul(0x100000001b3);}h}
fn frame_hash(f:&kimo_core::effects::Frame)->u64{let mut raw=Vec::with_capacity(f.d.len()*4);for v in &f.d{raw.extend_from_slice(&v.to_le_bytes());}fnv(&raw)}

/// Smooth normals with a crease-angle split: each corner averages only incident face
/// normals within `thresh_deg` of its own face normal; vertices on creases are
/// duplicated per smoothing group. Returns (expanded mesh, per-vertex normals).
fn split_crease(m:&Mesh,thresh_deg:f64)->(Mesh,Vec<[f64;3]>){
 let fns:Vec<[f64;3]>=m.indices.iter().map(|t|face_n(m,*t).0).collect();
 let cos_t=thresh_deg.to_radians().cos();
 let mut inc:Vec<Vec<usize>>=vec![Vec::new();m.positions.len()];
 for (fi,t) in m.indices.iter().enumerate(){for &i in t{inc[i as usize].push(fi);}}
 let mut positions=Vec::new();let mut normals=Vec::new();
 let mut groups:Vec<Vec<([f64;3],u32)>>=vec![Vec::new();m.positions.len()];
 let mut indices=Vec::with_capacity(m.indices.len());
 for (fi,t) in m.indices.iter().enumerate(){
  let mut tri=[0u32;3];
  for (k,&vi) in t.iter().enumerate(){
   let vi=vi as usize;let fnor=fns[fi];
   let mut acc=[0.;3];
   for &fj in &inc[vi]{let g=fns[fj];let w=1.0;if dot(fnor,g)>=cos_t{for j in 0..3{acc[j]+=g[j]*w;}}}
   let l=vlen(acc);let n=if l>0.{[acc[0]/l,acc[1]/l,acc[2]/l]}else{fnor};
   let existing=groups[vi].iter().find(|(g,_)|dot(*g,n)>1.0-1e-12).map(|&(_,idx)|idx);
   let idx=match existing{Some(i)=>i,None=>{let i=positions.len() as u32;positions.push(m.positions[vi]);normals.push(n);groups[vi].push((n,i));i}};
   tri[k]=idx;
  }
  indices.push(tri);
 }
 (Mesh{positions,indices},normals)
}

fn smooth_normals(m:&Mesh)->Vec<[f64;3]>{let mut n=vec![[0.;3];m.positions.len()];for t in &m.indices{let(f,a)=face_n(m,*t);for &i in t{for j in 0..3{n[i as usize][j]+=f[j]*a;}}}for v in &mut n{let l=vlen(*v);if l>0.{for j in 0..3{v[j]/=l;}}}n}
fn stream_of(m:&Mesh,n:&[[f64;3]])->Stream{let vs=m.positions.iter().enumerate().map(|(i,p)|{let nrm=n[i];let a=if nrm[1].abs()<0.9{[0.,1.,0.]}else{[1.,0.,0.]};let cr=cross(a,nrm);let l=vlen(cr);Vertex{position:p.map(|v|v as f32),normal:nrm.map(|v|v as f32),uv:[0.,0.],tangent:[(cr[0]/l)as f32,(cr[1]/l)as f32,(cr[2]/l)as f32,1.]}}).collect();Stream{vertices:vs,indices:m.indices.clone()}}
fn fit(pos:&[[f64;3]],pan:f64,tilt:f64)->(Camera,[f64;2]){let mut lo=[f64::INFINITY;3];let mut hi=[f64::NEG_INFINITY;3];for p in pos{for j in 0..3{lo[j]=lo[j].min(p[j]);hi[j]=hi[j].max(p[j]);}}let c=[(lo[0]+hi[0])/2.,(lo[1]+hi[1])/2.,(lo[2]+hi[2])/2.];let r=pos.iter().map(|p|(p[0]-c[0]).hypot(p[1]-c[1]).hypot(p[2]-c[2])).fold(0f64,f64::max);let mut cam=Camera{eye:[0.;3],pan,tilt,roll:0.,sensor_height:24.,focal_length:35.,near:0.1};let row3=[cam.view([1.,0.,0.]).unwrap()[2],cam.view([0.,1.,0.]).unwrap()[2],cam.view([0.,0.,1.]).unwrap()[2]];let d=r*1.18/(12f64/35.).atan().sin();cam.eye=[c[0]-d*row3[0],c[1]-d*row3[1],c[2]-d*row3[2]];(cam,[(d-2.*r).max(0.2),d+2.*r])}
fn save(frame:&kimo_core::effects::Frame,w:usize,h:usize,path:&str){let rgba=kimo_core::render::to_rgba8(frame);let mut ch=std::process::Command::new("ffmpeg").args(["-v","error","-y","-f","rawvideo","-pix_fmt","rgba","-s",&format!("{w}x{h}"),"-i","-","-frames:v","1",path]).stdin(std::process::Stdio::piped()).spawn().unwrap();std::io::Write::write_all(&mut ch.stdin.take().unwrap(),&rgba).unwrap();assert!(ch.wait().unwrap().success());}

struct Scene{vase:Mesh,nv:Vec<[f64;3]>,cut:Mesh,nc:Vec<[f64;3]>,volume:f64,min_area:f64,euler:i64,two_manifold:bool,worst_normal_deg:f64}
fn scene()->Scene{
 let vase0=generate(&Primitive::Lathe{profile:vec![[0.30,-1.0],[0.55,-0.75],[0.42,-0.35],[0.50,0.1],[0.62,0.55],[0.34,0.95],[0.30,1.05]],segments:32,caps:true}).unwrap().mesh();
 let vase=catmull_clark(&vase0,3,&[],10.0).unwrap();
 let star:Vec<[f64;2]>=(0..12).map(|i|{let r=if i%2==0{0.20}else{0.09};let a=i as f64*std::f64::consts::PI/6.0;[r*a.cos(),r*a.sin()]}).collect();
 let starpath=generate(&Primitive::ExtrudePath{profile:star,path:[[-2.4,0.25,0.0],[-0.8,0.95,0.7],[0.8,-0.55,-0.7],[2.4,0.1,0.0]],stations:64,caps:true}).unwrap().mesh();
 let cut=boolean(&vase.mesh,&starpath,BoolOp::Difference).unwrap();
 let nv=smooth_normals(&vase.mesh);
 let (cut_split,nc)=split_crease(&cut.mesh,40.0);
 // crease quality: worst angle between a split vertex normal and its own incident face normals
 let fns2:Vec<[f64;3]>=cut_split.indices.iter().map(|t|face_n(&cut_split,*t).0).collect();
 let mut worst=0f64;
 for (fi,t) in cut_split.indices.iter().enumerate(){for &i in t{let c=dot(nc[i as usize],fns2[fi]).clamp(-1.,1.);worst=worst.max(c.acos().to_degrees());}}
 println!("STAGE lathe32: {}v {}t | subdiv(3): {}v {}t | star: {}v {}t | cut: {}v {}t (split {}v) volume={:.4} minarea={:.3e}",vase0.positions.len(),vase0.indices.len(),vase.final_vertices,vase.final_triangles,starpath.positions.len(),starpath.indices.len(),cut.vertices,cut.triangles,cut_split.positions.len(),cut.volume,cut.min_triangle_area);
 Scene{vase:vase.mesh,nv,cut:cut_split,nc,volume:cut.volume,min_area:cut.min_triangle_area,euler:cut.euler_characteristic,two_manifold:cut.interior_edges_two_manifold,worst_normal_deg:worst}
}

#[test]fn hero_pipeline(){
 let s=scene();
 assert!(s.two_manifold&&s.euler==0&&s.volume>1.0&&s.min_area>1e-12,"structural: volume={} min_area={} euler={} 2manifold={}",s.volume,s.min_area,s.euler,s.two_manifold);
 println!("CREASE-SPLIT worst vertex-normal vs face-normal angle: {:.1}deg (unsplit census: p90 93.7, max 143.5)",s.worst_normal_deg);
 let(cam,dr)=fit(&s.cut.positions,324.79,18.0);
 let mat=Material{albedo:[0.72,0.70,0.66],metallic:0.0,roughness:0.42};
 let lights=[Light::Directional{travel_direction:[0.45,-1.0,0.55],radiance:[4.4,4.1,3.7]},Light::Directional{travel_direction:[-0.55,-0.25,-1.0],radiance:[1.7,1.8,2.1]}];
 let o1=render_mesh(&s.vase,&s.nv,&cam,320,180,Shading::Normal,dr).unwrap();
 let o2=render_mesh(&s.cut,&s.nc,&cam,320,180,Shading::Normal,dr).unwrap();
 let o3=lit_raster::render(&stream_of(&s.cut,&s.nc),&cam,320,180,&mat,&lights).unwrap();
 println!("HASH vase-normal={:016x} cut-normal={:016x} cut-pbr={:016x} covered={}/{}/{}",frame_hash(&o1.frame),frame_hash(&o2.frame),frame_hash(&o3.frame),o1.covered,o2.covered,o3.covered);
 assert!(o1.covered>5_000&&o2.covered>5_000&&o3.covered>5_000);
 const EXPECT:[u64;3]=[0x7da76338efc4bbd9,0xdcc5c22afef17484,0xe20ee8c1403efa08];
 if EXPECT!=[0,0,0]{assert_eq!([frame_hash(&o1.frame),frame_hash(&o2.frame),frame_hash(&o3.frame)],EXPECT,"hero frame hash drift");}
 if let Ok(dir)=std::env::var("KIMO_PM_FRAMES"){
  let o1=render_mesh(&s.vase,&s.nv,&cam,1920,1080,Shading::Normal,dr).unwrap();save(&o1.frame,1920,1080,&format!("{dir}/pm-hero-vase-subdiv-normal.png"));
  let o2=render_mesh(&s.cut,&s.nc,&cam,1920,1080,Shading::Normal,dr).unwrap();save(&o2.frame,1920,1080,&format!("{dir}/pm-hero-cut-normal.png"));
  let o3=lit_raster::render(&stream_of(&s.cut,&s.nc),&cam,1920,1080,&mat,&lights).unwrap();save(&o3.frame,1920,1080,&format!("{dir}/pm-hero-cut-pbr.png"));
  println!("FRAME 1080p covered={}/{}/{} clipped={}",o1.covered,o2.covered,o3.covered,o3.clipped_triangles);
 }
}
