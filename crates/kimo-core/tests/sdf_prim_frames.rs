//! SDF primitive frame fixture (CP1 double verification, render half): sphere-traced
//! capsule+torus smooth-union shaded with ANALYTIC gradient normals (lambert key).
//! Deterministic FNV-1a hash at 320x180; env-gated 640x360 PNG dump (KIMO_SDF_FRAMES).
//! No mesh, no contouring (that is CP2) - this traces the raw field directly.
use kimo_core::threed::sdf_graph::{Field,Graph};

fn sub(a:[f64;3],b:[f64;3])->[f64;3]{[a[0]-b[0],a[1]-b[1],a[2]-b[2]]}
fn unit(v:[f64;3])->[f64;3]{let l=(v[0]*v[0]+v[1]*v[1]+v[2]*v[2]).sqrt();[v[0]/l,v[1]/l,v[2]/l]}
fn cross(a:[f64;3],b:[f64;3])->[f64;3]{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
fn fnv(bytes:&[u8])->u64{let mut h=0xcbf29ce484222325u64;for b in bytes{h^=*b as u64;h=h.wrapping_mul(0x100000001b3);}h}

fn scene()->Graph{Graph{nodes:vec![
 Field::Torus{center:[0.,-0.15,0.],major_radius:1.05,minor_radius:0.42},
 Field::Capsule{a:[-0.85,-0.95,0.15],b:[0.85,0.95,-0.15],radius:0.34},
 Field::SmoothUnion{a:0,b:1,k:0.35}],output:2}}

fn trace(g:&Graph,eye:[f64;3],dir:[f64;3])->Option<f64>{let mut t=0.05;for _ in 0..400{let p=[eye[0]+t*dir[0],eye[1]+t*dir[1],eye[2]+t*dir[2]];let d=g.sample(p).unwrap();if d<2e-4*t.max(1.){return Some(t)}let step=(d*0.9).max(1e-4);t+=step;if t>24.{return None}}None}

fn render(w:usize,h:usize)->(kimo_core::effects::Frame,usize){
 let g=scene();let eye=[3.1,2.3,3.1];let fwd=unit([-3.1,-2.0,-3.1]);let right=unit(cross(fwd,[0.,1.,0.]));let up=cross(right,fwd);
 let key=unit([0.5,1.0,0.65]);let mut frame=kimo_core::effects::Frame::try_new(w,h).unwrap();let mut covered=0;
 for y in 0..h{for x in 0..w{
  let u=(x as f64+0.5)/w as f64-0.5;let v=0.5-(y as f64+0.5)/h as f64;let f=1.35;let aspect=w as f64/h as f64;
  let dir=unit([fwd[0]*f+right[0]*u*aspect+up[0]*v,fwd[1]*f+right[1]*u*aspect+up[1]*v,fwd[2]*f+right[2]*u*aspect+up[2]*v]);
  let id=(y*w+x)*4;
  if let Some(t)=trace(&g,eye,dir){covered+=1;let p=[eye[0]+t*dir[0],eye[1]+t*dir[1],eye[2]+t*dir[2]];
   // analytic normal; typed-error points (none expected on this C1 field) fall back to central FD
   let n=g.normal_analytic(p).unwrap_or_else(|_|{let e=1e-5;let mut nf=[0.;3];for j in 0..3{let mut a=p;let mut b=p;a[j]+=e;b[j]-=e;nf[j]=g.sample(a).unwrap()-g.sample(b).unwrap();}unit(nf)});
   let nl=(n[0]*key[0]+n[1]*key[1]+n[2]*key[2]).max(0.);
   let alb=[0.74,0.68,0.6];let c=0.06+0.94*nl;
   for j in 0..3{frame.d[id+j]=(alb[j]*c)as f32;}frame.d[id+3]=1.;
  }else{for j in 0..3{frame.d[id+j]=0.02;}frame.d[id+3]=1.;}
 }}
 (frame,covered)
}

#[test]fn sdf_prim_sphere_trace_analytic_normal_frame(){
 let(frame,covered)=render(320,180);
 let mut raw=Vec::with_capacity(frame.d.len()*4);for v in &frame.d{raw.extend_from_slice(&v.to_le_bytes());}
 let hash=fnv(&raw);
 println!("SDF-FRAME 320x180 covered={} hash={:016x}",covered,hash);
 assert!(covered>10_000,"covered {covered}");
 const EXPECT:u64=0x1508f2ee71f76e29;
 if EXPECT!=0{assert_eq!(hash,EXPECT,"sdf frame hash drift");}
 if let Ok(dir)=std::env::var("KIMO_SDF_FRAMES"){
  let(frame,covered)=render(640,360);
  let rgba=kimo_core::render::to_rgba8(&frame);
  let mut ch=std::process::Command::new("ffmpeg").args(["-v","error","-y","-f","rawvideo","-pix_fmt","rgba","-s","640x360","-i","-","-frames:v","1",&format!("{dir}/sdf-cp1-union-analytic.png")]).stdin(std::process::Stdio::piped()).spawn().unwrap();
  std::io::Write::write_all(&mut ch.stdin.take().unwrap(),&rgba).unwrap();assert!(ch.wait().unwrap().success());
  println!("SDF-FRAME 640x360 covered={} dumped",covered);
 }
}
