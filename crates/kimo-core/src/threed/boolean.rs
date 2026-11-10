//! Watertight mesh booleans via vendored Manifold 3.5.3 (Apache-2.0, vendor/manifold-3.5.3;
//! Sun fdlibm notice retained) through the narrow C ABI in manifold_shim.cpp. No copied tables
//! or constants: volume oracles in the tests are analytic (axis-aligned boxes) plus the
//! inclusion-exclusion identity. Manifold's kernel is f32 internally with exact predicates;
//! inputs are welded by exact position first and must be closed 2-manifold shells.
use super::mesh::Mesh;
use super::subdivide::{edge_stats,weld};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum BoolOp{Union,Difference,Intersection}

pub struct BooleanResult{pub mesh:Mesh,pub empty:bool,pub volume:f64,pub vertices:usize,pub triangles:usize,pub interior_edges_two_manifold:bool,pub boundary_edges:usize,pub euler_characteristic:i64,pub min_triangle_area:f64}

unsafe extern "C"{fn kimo_manifold_boolean(a_pos:*const f64,a_nv:usize,a_tri:*const u32,a_nt:usize,b_pos:*const f64,b_nv:usize,b_tri:*const u32,b_nt:usize,op:i32,out_pos:*mut *mut f64,out_nv:*mut usize,out_tri:*mut *mut u32,out_nt:*mut usize,out_volume:*mut f64,err:*mut i8,err_len:usize)->i32;fn kimo_manifold_free(p:*mut f64,t:*mut u32);}

fn signed_volume(pos:&[[f64;3]],tris:&[[u32;3]])->f64{let mut v=0.0;for t in tris{let[a,b,c]=t.map(|i|pos[i as usize]);v+=(a[0]*(b[1]*c[2]-b[2]*c[1])-a[1]*(b[0]*c[2]-b[2]*c[0])+a[2]*(b[0]*c[1]-b[1]*c[0]))/6.0;}v}

/// Boolean of two closed 2-manifold shells. `Empty` is a result, not an error (disjoint
/// intersection, identical difference). Output is re-validated: 2-manifold edges, outward CCW
/// winding (positive signed volume), no zero-area slivers at a scale-aware threshold.
pub fn boolean(a:&Mesh,b:&Mesh,op:BoolOp)->Result<BooleanResult,String>{
 let(pa,ta)=weld(a)?;let(pb,tb)=weld(b)?;
 let(ea,ba,ma)=edge_stats(&ta);if !ma||ba!=0{return Err(format!("input A must be a closed 2-manifold shell (boundary edges {ba}, manifold {ma})"));}
 let(eb,bb,mb)=edge_stats(&tb);if !mb||bb!=0{return Err(format!("input B must be a closed 2-manifold shell (boundary edges {bb}, manifold {mb})"));}
 let _=(ea,eb);
 let mut lo=[f64::INFINITY;3];let mut hi=[f64::NEG_INFINITY;3];for p in pa.iter().chain(pb.iter()){for j in 0..3{lo[j]=lo[j].min(p[j]);hi[j]=hi[j].max(p[j]);}}
 let extent=(hi[0]-lo[0]).max(hi[1]-lo[1]).max(hi[2]-lo[2]).max(1e-30);
 let code_op=match op{BoolOp::Union=>0,BoolOp::Difference=>1,BoolOp::Intersection=>2};
 let(mut op_pos,mut op_tri)=(std::ptr::null_mut::<f64>(),std::ptr::null_mut::<u32>());
 let(mut onv,mut ont,mut vol)=(0usize,0usize,0f64);let mut ebuf=[0i8;256];
 let code=unsafe{kimo_manifold_boolean(pa.as_ptr() as *const f64,pa.len(),ta.as_ptr() as *const u32,ta.len(),pb.as_ptr() as *const f64,pb.len(),tb.as_ptr() as *const u32,tb.len(),code_op,&mut op_pos,&mut onv,&mut op_tri,&mut ont,&mut vol,ebuf.as_mut_ptr(),ebuf.len())};
 if code!=0{let msg=ebuf.iter().take_while(|&&c|c!=0).map(|&c|c as u8 as char).collect::<String>();return Err(format!("manifold shim error {code}: {msg}"));}
 if onv==0{return Ok(BooleanResult{mesh:Mesh{positions:Vec::new(),indices:Vec::new()},empty:true,volume:0.0,vertices:0,triangles:0,interior_edges_two_manifold:true,boundary_edges:0,euler_characteristic:0,min_triangle_area:0.0});}
 let positions:Vec<[f64;3]>=unsafe{std::slice::from_raw_parts(op_pos,onv*3)}.chunks_exact(3).map(|c|[c[0],c[1],c[2]]).collect();
 let indices:Vec<[u32;3]>=unsafe{std::slice::from_raw_parts(op_tri,ont*3)}.chunks_exact(3).map(|c|[c[0],c[1],c[2]]).collect();
 unsafe{kimo_manifold_free(op_pos,op_tri)};
 let mesh=Mesh{positions,indices};mesh.validate()?;
 let(_,boundary,two)=edge_stats(&mesh.indices);
 if !two||boundary!=0{return Err(format!("boolean output not closed 2-manifold (boundary {boundary})"));}
 let sv=signed_volume(&mesh.positions,&mesh.indices);
 if sv<=0.0{return Err(format!("boolean output winding not outward CCW (signed volume {sv})"));}
 let mut min_area=f64::INFINITY;
 for t in &mesh.indices{let[a,b,c]=t.map(|i|mesh.positions[i as usize]);let ab=[b[0]-a[0],b[1]-a[1],b[2]-a[2]];let ac=[c[0]-a[0],c[1]-a[1],c[2]-a[2]];let n=[ab[1]*ac[2]-ab[2]*ac[1],ab[2]*ac[0]-ab[0]*ac[2],ab[0]*ac[1]-ab[1]*ac[0]];let area=0.5*(n[0]*n[0]+n[1]*n[1]+n[2]*n[2]).sqrt();min_area=min_area.min(area);}
 let sliver=extent*extent*1e-14;
 if min_area<=sliver{return Err(format!("zero-area sliver in boolean output (min area {min_area:e} <= {sliver:e} at extent {extent})"));}
 let euler=mesh.positions.len() as i64-edge_stats(&mesh.indices).0 as i64+mesh.indices.len() as i64;
 Ok(BooleanResult{mesh,empty:false,volume:vol,vertices:onv,triangles:ont,interior_edges_two_manifold:two,boundary_edges:boundary,euler_characteristic:euler,min_triangle_area:min_area})
}


#[cfg(test)]mod tests{use super::*;
 fn cube(side:f64,at:[f64;3])->Mesh{let h=side/2.0;let p=[[-h,-h,-h],[h,-h,-h],[h,h,-h],[-h,h,-h],[-h,-h,h],[h,-h,h],[h,h,h],[-h,h,h]];let t=[[0,2,1],[0,3,2],[4,5,6],[4,6,7],[0,1,5],[0,5,4],[1,2,6],[1,6,5],[2,3,7],[2,7,6],[3,0,4],[3,4,7]];Mesh{positions:p.iter().map(|p|[p[0]+at[0],p[1]+at[1],p[2]+at[2]]).collect(),indices:t.iter().map(|t|*t).collect()}}
 fn approx(a:f64,b:f64,tol:f64,what:&str){assert!((a-b).abs()<=tol,"{what}: {a} vs {b} (tol {tol})");}
 #[test]fn disjoint_case(){let a=cube(2.0,[0.;3]);let b=cube(2.0,[10.,0.,0.]);let u=boolean(&a,&b,BoolOp::Union).unwrap();approx(u.volume,16.0,1e-5,"disjoint union volume");assert_eq!(u.vertices,16,"disjoint union keeps both shells' verts");let i=boolean(&a,&b,BoolOp::Intersection).unwrap();assert!(i.empty,"disjoint intersection is a clean empty result");let d=boolean(&a,&b,BoolOp::Difference).unwrap();approx(d.volume,8.0,1e-5,"disjoint difference volume");assert_eq!(d.triangles,12);}
 #[test]fn identical_case(){let a=cube(2.0,[0.;3]);let b=cube(2.0,[0.;3]);let d=boolean(&a,&b,BoolOp::Difference).unwrap();assert!(d.empty,"identical difference empty");let i=boolean(&a,&b,BoolOp::Intersection).unwrap();approx(i.volume,8.0,1e-5,"identical intersection volume");let u=boolean(&a,&b,BoolOp::Union).unwrap();approx(u.volume,8.0,1e-5,"identical union volume");}
 #[test]fn contained_both_ways(){let big=cube(4.0,[0.;3]);let small=cube(2.0,[0.;3]);for(a,b,tag)in[(&big,&small,"big-small"),(&small,&big,"small-big")]{let u=boolean(a,b,BoolOp::Union).unwrap();approx(u.volume,64.0,1e-4,&format!("{tag} union = outer"));let i=boolean(a,b,BoolOp::Intersection).unwrap();approx(i.volume,8.0,1e-4,&format!("{tag} intersection = inner"));}let d=boolean(&big,&small,BoolOp::Difference).unwrap();approx(d.volume,56.0,1e-4,"hollow shell volume");assert_eq!(d.euler_characteristic,4,"shell with cavity: chi = 2 shells * 2");let d2=boolean(&small,&big,BoolOp::Difference).unwrap();assert!(d2.empty,"contained difference empty");}
 #[test]fn touching_shared_face(){let a=cube(2.0,[0.;3]);let b=cube(2.0,[2.,0.,0.]);let u=boolean(&a,&b,BoolOp::Union).unwrap();approx(u.volume,16.0,1e-4,"face-touching union volume (no duplicate coincident faces)");assert_eq!(u.euler_characteristic,2,"merged single shell, chi=2");}
 #[test]fn edge_and_point_contacts_rejected_or_valid(){let a=cube(2.0,[0.;3]);for(at,tag)in[([2.,2.,0.],"shared-edge"),([2.,2.,2.],"single-point tangent")]{let b=cube(2.0,at);match boolean(&a,&b,BoolOp::Union){Ok(r)=>{assert!(r.interior_edges_two_manifold,"{tag}: union emitted but must be 2-manifold (no pinch)");assert!(r.boundary_edges==0);},Err(e)=>{assert!(e.contains("manifold")||e.contains("Manifold")||e.contains("pinch")||e.contains("manifold shim"),"{tag}: rejection must name the manifoldness problem, got: {e}");}}}}
 #[test]fn coplanar_overlap(){let a=cube(2.0,[0.;3]);let b=cube(2.0,[1.,0.,0.]);let u=boolean(&a,&b,BoolOp::Union).unwrap();approx(u.volume,12.0,1e-4,"coplanar overlap union");let i=boolean(&a,&b,BoolOp::Intersection).unwrap();approx(i.volume,4.0,1e-4,"coplanar overlap intersection");let d=boolean(&a,&b,BoolOp::Difference).unwrap();approx(d.volume,4.0,1e-4,"coplanar overlap difference");}
 #[test]fn inclusion_exclusion_overlap(){let a=cube(2.0,[0.;3]);let b=cube(2.0,[1.,0.5,0.25]);let u=boolean(&a,&b,BoolOp::Union).unwrap();let i=boolean(&a,&b,BoolOp::Intersection).unwrap();approx(u.volume,8.0+8.0-i.volume,1e-4,"V(u)=V(a)+V(b)-V(i)");}
 #[test]fn non_watertight_input_rejected(){let mut a=cube(2.0,[0.;3]);a.indices.pop();let b=cube(2.0,[1.,0.,0.]);let r=boolean(&a,&b,BoolOp::Union);assert!(r.is_err(),"open shell rejected before calling the kernel");}
 #[test]fn cylinder_through_cube_genus_and_volumes(){let cyl=crate::threed::primitives::generate(&crate::threed::primitives::Primitive::Lathe{profile:vec![[0.7,-2.0],[0.7,2.0]],segments:32,caps:true}).unwrap().mesh();let c=cube(2.0,[0.;3]);let d=boolean(&c,&cyl,BoolOp::Difference).unwrap();assert_eq!(d.euler_characteristic,0,"cube with through-hole: genus 1, chi=0");let v_cyl=0.7*0.7*std::f64::consts::PI*2.0;approx(d.volume,8.0-v_cyl,0.05,"difference volume vs analytic cylinder");let i=boolean(&c,&cyl,BoolOp::Intersection).unwrap();approx(i.volume,v_cyl,0.05,"intersection volume vs analytic cylinder");let u=boolean(&c,&cyl,BoolOp::Union).unwrap();approx(u.volume,8.0+4.0*0.7*0.7*std::f64::consts::PI-i.volume,0.1,"union via inclusion-exclusion");}
 #[test]fn output_quality_guards_on_all_ok_results(){let a=cube(2.0,[0.;3]);let b=cube(2.0,[1.,0.,0.]);for op in [BoolOp::Union,BoolOp::Difference,BoolOp::Intersection]{let r=boolean(&a,&b,op).unwrap();assert!(r.interior_edges_two_manifold&&r.boundary_edges==0,"{op:?} closed 2-manifold");assert!(r.min_triangle_area>1e-10,"{op:?} no slivers: min area {}",r.min_triangle_area);}}
 #[test]fn render_boolean_verification_frames(){let Ok(dir)=std::env::var("KIMO_PM_FRAMES")else{return};use crate::threed::perspective::{render_mesh,Shading};use crate::camera_projection::Camera;
  let normals=|m:&Mesh|->Vec<[f64;3]>{let mut n=vec![[0.;3];m.positions.len()];for t in &m.indices{let[a,b,c]=t.map(|i|m.positions[i as usize]);let ab=[b[0]-a[0],b[1]-a[1],b[2]-a[2]];let ac=[c[0]-a[0],c[1]-a[1],c[2]-a[2]];let f=[ab[1]*ac[2]-ab[2]*ac[1],ab[2]*ac[0]-ab[0]*ac[2],ab[0]*ac[1]-ab[1]*ac[0]];for &i in t{for j in 0..3{n[i as usize][j]+=f[j];}}}for v in &mut n{let l=(v[0]*v[0]+v[1]*v[1]+v[2]*v[2]).sqrt();if l>0.{for j in 0..3{v[j]/=l;}}}n};
  let fit=|pos:&[[f64;3]]|->(Camera,[f64;2]){let mut lo=[f64::INFINITY;3];let mut hi=[f64::NEG_INFINITY;3];for p in pos{for j in 0..3{lo[j]=lo[j].min(p[j]);hi[j]=hi[j].max(p[j]);}}let c=[(lo[0]+hi[0])/2.,(lo[1]+hi[1])/2.,(lo[2]+hi[2])/2.];let r=pos.iter().map(|p|(p[0]-c[0]).hypot(p[1]-c[1]).hypot(p[2]-c[2])).fold(0f64,f64::max);let mut cam=Camera{eye:[0.;3],pan:324.79,tilt:24.55,roll:0.,sensor_height:24.,focal_length:35.,near:0.1};let row3=[cam.view([1.,0.,0.]).unwrap()[2],cam.view([0.,1.,0.]).unwrap()[2],cam.view([0.,0.,1.]).unwrap()[2]];let d=r*1.18/(12f64/35.).atan().sin();cam.eye=[c[0]-d*row3[0],c[1]-d*row3[1],c[2]-d*row3[2]];(cam,[(d-2.*r).max(0.2),d+2.*r])};
  // scene 1: cylinder through sphere; scene 2: cube minus an angled torus
  let sph=crate::threed::primitives::generate(&crate::threed::primitives::Primitive::Sphere{radius:1.4,longitude_slices:32,latitude_slices:24}).unwrap().mesh();
  let cyl=crate::threed::primitives::generate(&crate::threed::primitives::Primitive::Lathe{profile:vec![[0.8,-2.0],[0.8,2.0]],segments:32,caps:true}).unwrap().mesh();
  let torus=crate::threed::primitives::generate(&crate::threed::primitives::Primitive::Torus{major_radius:0.8,minor_radius:0.35,radial_segments:48,tubular_segments:24}).unwrap().mesh();
  let(rot,s,c)=(35f64.to_radians(),35f64.to_radians().sin(),35f64.to_radians().cos());let _=rot;
  let angled=Mesh{positions:torus.positions.iter().map(|p|[p[0],c*p[1]-s*p[2],s*p[1]+c*p[2]]).collect(),indices:torus.indices.clone()};
  let cb=cube(2.2,[0.;3]);
  for(name,m)in[("boole-union",&boolean(&sph,&cyl,BoolOp::Union).unwrap().mesh),("boole-difference",&boolean(&cb,&angled,BoolOp::Difference).unwrap().mesh),("boole-intersection",&boolean(&sph,&cyl,BoolOp::Intersection).unwrap().mesh)]{
   let n=normals(m);let(cam,dr)=fit(&m.positions);let o=render_mesh(m,&n,&cam,1920,1080,Shading::Normal,dr).unwrap();assert!(o.covered>20_000,"{name} covered={}",o.covered);crate::render::write_image(&o.frame,&format!("{dir}/pm-{name}-normal.png")).unwrap();}}
}
