use kimo_core::camera_projection::{Camera,Vertex,clip};
fn cam()->Camera{Camera{eye:[0.0,0.0,-10.0],pan:0.0,tilt:0.0,roll:0.0,sensor_height:24.0,focal_length:50.0,near:1.0}}
fn v(x:f64,y:f64,z:f64,a:f64)->Vertex{Vertex{position:[x,y,z],attributes:vec![a]}}
#[test]fn focal_changes_only_projection_not_view_pose(){let a=cam();let b=Camera{focal_length:100.0,..a};assert_eq!(a.view([2.0,3.0,0.0]).unwrap(),b.view([2.0,3.0,0.0]).unwrap());let p=a.project([2.0,3.0,0.0],1920,1080).unwrap().unwrap();let q=b.project([2.0,3.0,0.0],1920,1080).unwrap().unwrap();assert!((q[0]-960.0-2.0*(p[0]-960.0)).abs()<1e-9);assert_eq!(a.eye,b.eye);}
#[test]fn near_inside_outside_and_crossings(){for(z,count)in [([2.0,2.0,2.0],1),([0.0,0.0,0.0],0),([2.0,0.0,0.0],1),([0.0,2.0,2.0],2)]{let out=clip([v(-1.0,0.0,z[0],0.0),v(1.0,0.0,z[1],1.0),v(0.0,1.0,z[2],2.0)],1.0).unwrap();assert_eq!(out.len(),count);assert!(out.iter().flatten().all(|p|p.position[2]>=1.0));}}
#[test]fn attributes_interpolate_at_crossing(){let out=clip([v(0.0,0.0,0.0,0.0),v(2.0,0.0,2.0,2.0),v(0.0,2.0,2.0,4.0)],1.0).unwrap();let p=out.iter().flatten().find(|p|p.position==[1.0,0.0,1.0]).unwrap();assert_eq!(p.attributes,[1.0]);}
#[test]fn invalid_rejected_and_onplane_deduplicated(){assert!(clip([v(0.0,0.0,0.0,0.0),v(1.0,0.0,2.0,0.0),v(0.0,1.0,2.0,0.0)],0.0).is_err());assert!(Camera{near:f64::NAN,..cam()}.project([0.0;3],10,10).is_err());let t=[v(0.0,0.0,1.0,0.0),v(1.0,0.0,2.0,1.0),v(0.0,1.0,0.0,2.0)];assert_eq!(clip(t,1.0).unwrap().len(),1);}
#[test]fn all_axes_rotation_and_eye_translation_finite(){for a in [-180.0,-90.0,0.0,90.0,180.0]{let c=Camera{pan:a,tilt:a*0.3,roll:a*0.2,..cam()};assert!(c.view([1.0,2.0,3.0]).unwrap().iter().all(|x|x.is_finite()));}}

// == camera-v2 supplementary oracle rows ==
// View transform equals the transpose of the explicit R_y(pan)R_x(tilt)R_z(roll) composition
// applied to (world - eye) - independent matrix formulation of the inline quaternion-free code.
#[test]fn view_matches_explicit_matrix_transpose(){
 fn matmul(a:[[f64;3];3],b:[[f64;3];3])->[[f64;3];3]{let mut o=[[0.0;3];3];for i in 0..3{for j in 0..3{for k in 0..3{o[i][j]+=a[i][k]*b[k][j];}}}o}
 let poses=[[10.0,5.0,2.0],[-37.5,11.0,-25.0],[179.0,-89.0,44.0],[0.0,0.0,0.0]];
 for [pan,tilt,roll] in poses{
  let c=Camera{pan,tilt,roll,..cam()};
  let (sp,cp)=pan.to_radians().sin_cos();let (st,ct)=tilt.to_radians().sin_cos();let (sr,cr)=roll.to_radians().sin_cos();
  let ry=[[cp,0.0,sp],[0.0,1.0,0.0],[-sp,0.0,cp]];
  let rx=[[1.0,0.0,0.0],[0.0,ct,-st],[0.0,st,ct]];
  let rz=[[cr,-sr,0.0],[sr,cr,0.0],[0.0,0.0,1.0]];
  let m=matmul(ry,matmul(rx,rz)); // world->camera is M^T applied after eye subtraction
  for w in [[2.0,3.0,0.0],[-4.0,1.5,7.0],[0.1,-0.2,30.0]]{
   let q=c.view(w).unwrap();let d=[w[0]-c.eye[0],w[1]-c.eye[1],w[2]-c.eye[2]];
   for i in 0..3{let e=m[0][i]*d[0]+m[1][i]*d[1]+m[2][i]*d[2];assert!((q[i]-e).abs()<1e-9,"pose {pan}/{tilt}/{roll} point {w:?} axis {i}: {} vs {e}",q[i]);}
  }
 }
}
// Projection is the exact pinhole tangent law: pixel offset from center == f * tan(angle off axis).
#[test]fn project_matches_tangent_law(){
 let c=cam();let f=1080.0*(c.focal_length/c.sensor_height);
 for [x,z] in [[1.0,4.0],[3.0,9.0],[0.25,2.0]]{
  let w=[c.eye[0]+x,c.eye[1],c.eye[2]+z];
  let p=c.project(w,1920,1080).unwrap().unwrap();
  let angle=(x/z).atan();let expect=960.0+f*angle.tan();
  assert!((p[0]-expect).abs()<1e-6,"tangent law x={x} z={z}: {} vs {expect}",p[0]);
  assert!((p[1]-540.0).abs()<1e-9);assert!((p[2]-z).abs()<1e-9);
 }
 let fov=c.vertical_fov().unwrap();let e=2.0*(c.sensor_height/(2.0*c.focal_length)).atan().to_degrees();
 assert!((fov-e).abs()<1e-12);
}
// Dolly (eye translation) changes view depth and pixel scale; focal length never changes view - separation oracle.
#[test]fn dolly_vs_focal_separation(){
 let a=cam();let w=[2.0,3.0,0.0];
 let dollied=Camera{eye:[0.0,0.0,-9.0],..a};
 assert_ne!(a.view(w).unwrap(),dollied.view(w).unwrap());
 let p1=a.project(w,1920,1080).unwrap().unwrap();let p2=dollied.project(w,1920,1080).unwrap().unwrap();
 assert!(p2[0]-960.0>p1[0]-960.0,"nearer eye must magnify offset");
 let zoomed=Camera{focal_length:100.0,..a};
 assert_eq!(a.view(w).unwrap(),zoomed.view(w).unwrap());
}
// Clipped output vertices are convex combinations of the input triangle (barycentric containment).
#[test]fn clip_output_inside_input_triangle(){
 let tri=[v(-1.0,0.0,0.5,0.0),v(1.0,0.0,3.0,1.0),v(0.0,1.0,1.5,2.0)];
 let out=clip(tri.clone(),1.0).unwrap();assert!(!out.is_empty());
 for t in &out{for p in t{
  // solve barycentric coordinates in the plane of the input triangle (3x3 least squares via z-free xy+1 rows)
  let m=[[tri[0].position[0],tri[1].position[0],tri[2].position[0]],[tri[0].position[1],tri[1].position[1],tri[2].position[1]],[1.0,1.0,1.0]];
  let rhs=[p.position[0],p.position[1],1.0];
  // Cramer
  let det=m[0][0]*(m[1][1]*m[2][2]-m[1][2]*m[2][1])-m[0][1]*(m[1][0]*m[2][2]-m[1][2]*m[2][0])+m[0][2]*(m[1][0]*m[2][1]-m[1][1]*m[2][0]);
  let mut lam=[0.0;3];
  for c in 0..3{let mut mm=m;mm[0][c]=rhs[0];mm[1][c]=rhs[1];mm[2][c]=rhs[2];
   lam[c]=mm[0][0]*(mm[1][1]*mm[2][2]-mm[1][2]*mm[2][1])-mm[0][1]*(mm[1][0]*mm[2][2]-mm[1][2]*mm[2][0])+mm[0][2]*(mm[1][0]*mm[2][1]-mm[1][1]*mm[2][0]);lam[c]/=det;}
  assert!(lam.iter().all(|l|*l>-1e-9&&*l<1.0+1e-9),"vertex {p:?} outside input triangle: bary {lam:?}");
  assert!(p.position[2]>=1.0-1e-12);
 }}
}
// Degenerate directions: extreme but finite poses stay finite through project; zero-size viewport rejected.
#[test]fn extreme_pose_and_viewport_validation(){
 let c=Camera{eye:[1e6,-1e6,1e6],pan:1e-300,tilt:-1e-300,roll:360.0,sensor_height:1e-6,focal_length:1e6,near:1e-9};
 let r=c.project([0.0,0.0,0.0],1920,1080);assert!(r.is_ok());if let Ok(Some(p))=r{assert!(p.iter().all(|v|v.is_finite()));}
 assert!(cam().project([0.0,0.0,5.0],0,1080).is_err());
 assert!(Camera{sensor_height:0.0,..cam()}.vertical_fov().is_err());
}

// == camera-v2 harness visualizer (raw math, no adapter) ==
// Wireframe ground grid + axis gizmo through Camera::view/project; manual near-plane segment
// clipping in camera space; clipped endpoints re-projected via an origin-aligned Camera so the
// library projection path is exercised for every plotted point. 2x supersampled DDA lines.
#[test]#[ignore="1080p camera pose render gate"]
fn render_1080p_camera_poses(){
 use std::{io::Write,path::PathBuf};
 let dir=PathBuf::from(std::env::var("KIMO_CAMERA_FRAMES").expect("set output directory"));std::fs::create_dir_all(&dir).unwrap();
 let (w,h)=(1920usize,1080usize);let (sw,sh)=(w*2,h*2);
 // scene: ground grid y=0, x in -5..=5 step 1, z in 0..=10 step 1; axis gizmo at origin len 2
 let mut segs:Vec<([f64;3],[f64;3],[u8;3])>=Vec::new();
 for x in -5..=5{segs.push(([x as f64,0.0,0.0],[x as f64,0.0,10.0],[150,150,150]));}
 for z in 0..=10{segs.push(([-5.0,0.0,z as f64],[5.0,0.0,z as f64],[150,150,150]));}
 segs.push(([0.0,0.0,0.0],[3.0,0.0,0.0],[255,70,70]));   // +X red
 segs.push(([0.0,0.0,0.0],[0.0,3.0,0.0],[70,255,70]));   // +Y green
 segs.push(([0.0,0.0,0.0],[0.0,0.0,3.0],[80,80,255]));   // +Z blue
 // scene camera above the grid looking down (image convention: +y projects downward)
 let base=Camera{eye:[0.0,4.0,-8.0],pan:0.0,tilt:30.0,roll:0.0,sensor_height:24.0,focal_length:50.0,near:1.0};
 let poses:Vec<(&str,Camera)>=vec![
  ("default",base.clone()),
  ("pan30",Camera{pan:30.0,..base}),
  ("tilt10",Camera{tilt:10.0,..base}),
  ("roll15",Camera{roll:15.0,..base}),
  ("dolly-in",Camera{eye:[0.0,4.0,-4.0],..base}),
  ("focal-zoom",Camera{focal_length:100.0,..base}),
  ("clip-edge",Camera{near:10.5,..base}),
 ];
 for (name,c) in poses{
 let origin_cam=Camera{eye:[0.0;3],pan:0.0,tilt:0.0,roll:0.0,focal_length:c.focal_length,sensor_height:c.sensor_height,near:c.near}; // per-pose projection optics, zero pose (explicit fields: no move of c)
  let mut img=vec![[0u8;3];sw*sh];
  for &(p0,p1,col) in &segs{
   let (mut a,mut b)=(c.view(p0).unwrap(),c.view(p1).unwrap());
   if a[2]<c.near&&b[2]<c.near{continue;}
   if a[2]<c.near||b[2]<c.near{ // clip segment at the near plane in camera space
    let (in_a,in_b)=if a[2]<c.near{(b,a)}else{(a,b)};
    let t=(c.near-in_a[2])/(in_b[2]-in_a[2]);
    let cut=[in_a[0]+(in_b[0]-in_a[0])*t,in_a[1]+(in_b[1]-in_a[1])*t,c.near];
    a=in_a;b=cut;
   }
   let pa=origin_cam.project(a,w as u32,h as u32).unwrap().unwrap();
   let pb=origin_cam.project(b,w as u32,h as u32).unwrap().unwrap();
   // DDA at 2x
   let (mut x0,mut y0,mut x1,mut y1)=(pa[0]*2.0,pa[1]*2.0,pb[0]*2.0,pb[1]*2.0);
   if x0>x1{std::mem::swap(&mut x0,&mut x1);std::mem::swap(&mut y0,&mut y1);}
   let steps=((x1-x0).abs().max((y1-y0).abs()) as usize).max(1);
   for i in 0..=steps{let t=i as f64/steps as f64;let px=(x0+(x1-x0)*t)as usize;let py=(y0+(y1-y0)*t)as usize;
    for dy in 0..2{for dx in 0..2{let (qx,qy)=(px+dx,py+dy);if qx<sw&&qy<sh{let cur=img[qy*sw+qx];
     img[qy*sw+qx]=if cur!=[0,0,0]&&cur!=col{[cur[0]/2+col[0]/2,cur[1]/2+col[1]/2,cur[2]/2+col[2]/2]}else{col};}}}} // blend on overlap so collinear gizmo axes both stay visible
  }
  let mut file=std::fs::File::create(dir.join(format!("pose-{name}.ppm"))).unwrap();write!(file,"P6\n{w} {h}\n255\n").unwrap();
  let mut rgb=Vec::with_capacity(w*h*3);
  for y in 0..h{for x in 0..w{let mut acc=[0u32;3];for dy in 0..2{for dx in 0..2{let p=img[(y*2+dy)*sw+x*2+dx];for c in 0..3{acc[c]+=p[c]as u32;}}}for c in 0..3{rgb.push((acc[c]/4)as u8);}}}
  file.write_all(&rgb).unwrap();
 }
}
