use super::mesh::Mesh;
use serde::{Serialize,Deserialize};
#[derive(Clone,Debug,Serialize,Deserialize)]pub struct Ray {pub origin:[f64;3],pub direction:[f64;3],pub t_min:f64,pub t_max:f64}
#[derive(Clone,Debug,Serialize)]pub struct Hit {pub triangle:usize,pub t:f64,pub barycentric:[f64;3]}
#[derive(Clone,Debug,Serialize)]pub struct BvhNode {pub bounds:[[f64;3];2],pub children:Option<[usize;2]>,pub triangles:Vec<usize>}
#[derive(Clone,Debug,Serialize)]pub struct Bvh {pub nodes:Vec<BvhNode>}
fn sub(a:[f64;3],b:[f64;3])->[f64;3]{std::array::from_fn(|i|a[i]-b[i])}
fn dot(a:[f64;3],b:[f64;3])->f64{a.iter().zip(b).map(|(a,b)|a*b).sum()}
fn cross(a:[f64;3],b:[f64;3])->[f64;3]{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
fn triangle(mesh:&Mesh,id:usize,r:&Ray)->Option<Hit>{let idx=mesh.indices[id];let a=mesh.positions[idx[0] as usize];let e1=sub(mesh.positions[idx[1] as usize],a);let e2=sub(mesh.positions[idx[2] as usize],a);let p=cross(r.direction,e2);let det=dot(e1,p);if det.abs()<1e-15{return None;}let q=sub(r.origin,a);let u=dot(q,p)/det;if !(0.0..=1.0).contains(&u){return None;}let v=dot(r.direction,cross(q,e1))/det;if v<0.0||u+v>1.0{return None;}let t=dot(e2,cross(q,e1))/det;if !t.is_finite()||t<r.t_min||t>r.t_max{return None;}Some(Hit{triangle:id,t,barycentric:[1.0-u-v,u,v]})}
fn slab(bounds:[[f64;3];2],r:&Ray,limit:f64)->bool{let mut near=r.t_min;let mut far=r.t_max.min(limit);for j in 0..3{if r.direction[j]==0.0{if r.origin[j]<bounds[0][j]||r.origin[j]>bounds[1][j]{return false;}continue;}let a=(bounds[0][j]-r.origin[j])/r.direction[j];let b=(bounds[1][j]-r.origin[j])/r.direction[j];near=near.max(a.min(b));far=far.min(a.max(b));if near>far{return false;}}true}
fn valid_ray(r:&Ray)->Result<(),String>{if r.origin.iter().chain(r.direction.iter()).chain([r.t_min,r.t_max].iter()).any(|v|!v.is_finite()||v.abs()>1e12)||r.direction.iter().all(|v|*v==0.0)||r.t_min<0.0||r.t_max<r.t_min{return Err("finite bounded nonzero ray and0<=t_min<=t_max required".into());}Ok(())}
impl Bvh {
 pub fn build(mesh:&Mesh)->Result<Self,String>{mesh.validate()?;let mut out=Self{nodes:Vec::new()};
  fn build(out:&mut Bvh,m:&Mesh,mut ids:Vec<usize>)->usize{crate::guard::check();let mut b=[[f64::INFINITY;3],[f64::NEG_INFINITY;3]];for id in &ids{for i in m.indices[*id]{let p=m.positions[i as usize];for j in 0..3{b[0][j]=b[0][j].min(p[j]);b[1][j]=b[1][j].max(p[j]);}}}let index=out.nodes.len();out.nodes.push(BvhNode{bounds:b,children:None,triangles:Vec::new()});if ids.len()<=4{out.nodes[index].triangles=ids;return index;}let axis=(0..3).max_by(|a,c|(b[1][*a]-b[0][*a]).total_cmp(&(b[1][*c]-b[0][*c]))).unwrap();let center=|id:usize|m.indices[id].iter().map(|i|m.positions[*i as usize][axis]).sum::<f64>()/3.0;ids.sort_by(|a,b|center(*a).total_cmp(&center(*b)).then(a.cmp(b)));let other=ids.split_off(ids.len()/2);let a=build(out,m,ids);let b=build(out,m,other);out.nodes[index].children=Some([a,b]);index}
  build(&mut out,mesh,(0..mesh.indices.len()).collect());Ok(out)
 }
 pub fn raycast(&self,mesh:&Mesh,r:&Ray)->Result<Option<Hit>,String>{mesh.validate()?;valid_ray(r)?;let mut best:Option<Hit>=None;let mut stack=vec![0usize];while let Some(i)=stack.pop(){crate::guard::check();let n=self.nodes.get(i).ok_or("invalid BVH node")?;if !slab(n.bounds,r,best.as_ref().map(|h|h.t).unwrap_or(r.t_max)){continue;}if let Some(children)=n.children{stack.extend(children);}for id in &n.triangles{if *id>=mesh.indices.len(){return Err("BVH mesh mismatch".into());}if let Some(h)=triangle(mesh,*id,r){if best.as_ref().map(|b|h.t<b.t||h.t==b.t&&h.triangle<b.triangle).unwrap_or(true){best=Some(h);}}}}Ok(best)}
}
pub fn brute_force(mesh:&Mesh,r:&Ray)->Result<Option<Hit>,String>{mesh.validate()?;valid_ray(r)?;let mut best:Option<Hit>=None;for i in 0..mesh.indices.len(){if let Some(h)=triangle(mesh,i,r){if best.as_ref().map(|b|h.t<b.t).unwrap_or(true){best=Some(h);}}}Ok(best)}
/// Plane convention dot(n,p)+d>=0 is inside. Boundary boxes retained.
pub fn intersects_frustum(bounds:[[f64;3];2],planes:[[f64;4];6])->Result<bool,String>{if bounds.iter().flatten().chain(planes.iter().flatten()).any(|v|!v.is_finite())||(0..3).any(|i|bounds[0][i]>bounds[1][i]){return Err("finite ordered bounds and planes required".into());}for p in planes{if p[..3].iter().all(|v|*v==0.0){return Err("frustum plane normal nonzero required".into());}let far:[f64;3]=std::array::from_fn(|j|if p[j]>=0.0{bounds[1][j]}else{bounds[0][j]});if p[0]*far[0]+p[1]*far[1]+p[2]*far[2]+p[3]<0.0{return Ok(false);}}Ok(true)}


#[cfg(test)]
mod tests{
 use super::*;
 use crate::threed::primitives::{generate,Primitive};
 fn close(a:f64,b:f64)->bool{(a-b).abs()<=1e-12*b.abs().max(1.0)}
 // MT_* are an independent Python transcription of the standard Moller-Trumbore
 // ray-triangle intersection (same algebra as `triangle` above), pinned at 1e-12.
 pub const MT_TRI:[[f64;3];3]=[[0.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00],[2.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00],[0.00000000000000000e+00,2.00000000000000000e+00,0.00000000000000000e+00]];
 pub const MT_RAYS:[([f64;3],[f64;3]);6]=[([5.00000000000000000e-01,5.00000000000000000e-01,1.00000000000000000e+00],[0.00000000000000000e+00,0.00000000000000000e+00,-1.00000000000000000e+00]),([5.00000000000000000e-01,5.00000000000000000e-01,-1.00000000000000000e+00],[0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000000e+00]),([1.50000000000000000e+00,4.00000000000000022e-01,2.00000000000000000e+00],[0.00000000000000000e+00,0.00000000000000000e+00,-2.00000000000000000e+00]),([3.00000000000000000e+00,5.00000000000000000e-01,1.00000000000000000e+00],[0.00000000000000000e+00,0.00000000000000000e+00,-1.00000000000000000e+00]),([5.00000000000000000e-01,5.00000000000000000e-01,1.00000000000000000e+00],[1.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00]),([5.00000000000000000e-01,5.00000000000000000e-01,1.00000000000000000e+00],[0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000000e+00])];
 pub const MT_EXPECTED:[Option<(f64,[f64;3])>;6]=[
 Some((1.00000000000000000e+00,[5.00000000000000000e-01,2.50000000000000000e-01,2.50000000000000000e-01])),
 Some((1.00000000000000000e+00,[5.00000000000000000e-01,2.50000000000000000e-01,2.50000000000000000e-01])),
 Some((1.00000000000000000e+00,[4.99999999999999889e-02,7.50000000000000000e-01,2.00000000000000011e-01])),
 None,
 None,
 None,
 ];
 fn unit_triangle_mesh()->Mesh{Mesh{positions:MT_TRI.to_vec(),indices:vec![[0,1,2]]}}
 struct Xor(u64);
 impl Xor{fn next(&mut self)->u64{self.0^=self.0<<13;self.0^=self.0>>7;self.0^=self.0<<17;self.0}fn f(&mut self)->f64{(self.next()>>11)as f64/(1u64<<53)as f64}}

 #[test]fn moller_trumbore_matches_python_oracle(){let m=unit_triangle_mesh();for (ri,(o,d)) in MT_RAYS.iter().enumerate(){let r=Ray{origin:*o,direction:*d,t_min:0.,t_max:1e12};let got=brute_force(&m,&r).unwrap();match(got,MT_EXPECTED[ri]){(None,None)=>{},(Some(h),Some((t,b)))=>{assert!(close(h.t,t),"ray {ri} t {} want {t}",h.t);for j in 0..3{assert!(close(h.barycentric[j],b[j]),"ray {ri} bary[{j}]");}},(g,e)=>panic!("ray {ri}: got {g:?} want {e:?}")}}}
 #[test]fn bvh_raycast_parity_with_brute_force_and_hit_point_consistent(){let g=generate(&Primitive::Sphere{radius:1.,longitude_slices:16,latitude_slices:8}).unwrap();let m=g.mesh();let bvh=Bvh::build(&m).unwrap();let mut rng=Xor(0x1234_5678_9abc_def1);
  for i in 0..300{let o=[(rng.f()-0.5)*6.,(rng.f()-0.5)*6.,(rng.f()-0.5)*6.];let d=[rng.f()-0.5,rng.f()-0.5,rng.f()-0.5];let r=Ray{origin:o,direction:d,t_min:0.,t_max:1e12};
   let a=bvh.raycast(&m,&r).unwrap();let b=brute_force(&m,&r).unwrap();match(&a,&b){(None,None)=>{},(Some(x),Some(y))=>{assert!((x.t-y.t).abs()<1e-9,"ray {i}: bvh t {} vs brute {}",x.t,y.t);
    let idx=m.indices[x.triangle];let p:Vec<f64>=(0..3).map(|j|(0..3).map(|k|x.barycentric[k]*m.positions[idx[k]as usize][j]).sum()).collect();for j in 0..3{assert!((p[j]-(o[j]+x.t*d[j])).abs()<1e-9,"ray {i}: barycentric hit point equals origin+t*dir axis {j}");}},_=>panic!("ray {i}: bvh {a:?} vs brute {b:?}")}}}
 #[test]fn bvh_structure_leaf_bounds_and_coverage(){let g=generate(&Primitive::Sphere{radius:1.,longitude_slices:16,latitude_slices:8}).unwrap();let m=g.mesh();let bvh=Bvh::build(&m).unwrap();
  let mut seen=vec![0usize;m.indices.len()];for n in &bvh.nodes{for id in &n.triangles{seen[*id]+=1;assert!(n.triangles.len()<=4,"leaf holds at most 4 triangles");for i in m.indices[*id]{for j in 0..3{let p=m.positions[i as usize][j];assert!(p>=n.bounds[0][j]-1e-12&&p<=n.bounds[1][j]+1e-12,"node bounds contain its triangles");}}}}
  assert!(seen.iter().all(|c|*c==1),"every triangle in exactly one leaf");
  let mb=m.bounds().unwrap();for j in 0..3{assert!(close(bvh.nodes[0].bounds[0][j],mb[0][j])&&close(bvh.nodes[0].bounds[1][j],mb[1][j]),"root bounds equal mesh bounds axis {j}");}}
 #[test]fn valid_ray_typed_errors(){let m=unit_triangle_mesh();let ray=|o,d,t0,t1|Ray{origin:o,direction:d,t_min:t0,t_max:t1};
  assert_eq!(brute_force(&m,&ray([0.;3],[0.;3],0.,1.)).unwrap_err(),"finite bounded nonzero ray and0<=t_min<=t_max required");
  assert_eq!(brute_force(&m,&ray([0.;3],[0.,0.,1.],2.,1.)).unwrap_err(),"finite bounded nonzero ray and0<=t_min<=t_max required");
  assert_eq!(brute_force(&m,&ray([0.;3],[0.,0.,1.],-1.,1.)).unwrap_err(),"finite bounded nonzero ray and0<=t_min<=t_max required");
  assert_eq!(brute_force(&m,&ray([0.,f64::NAN,0.],[0.,0.,1.],0.,1.)).unwrap_err(),"finite bounded nonzero ray and0<=t_min<=t_max required");}
 #[test]fn frustum_planes_box_cases_and_typed_errors(){let p=[[-1.,0.,0.,1.],[1.,0.,0.,1.],[0.,-1.,0.,1.],[0.,1.,0.,1.],[0.,0.,-1.,1.],[0.,0.,1.,1.]];
  assert!(intersects_frustum([[-0.5,-0.5,-0.5],[0.5,0.5,0.5]],p).unwrap(),"box fully inside");
  assert!(intersects_frustum([[1.,1.,1.],[2.,2.,2.]],p).unwrap(),"corner-touching boundary box retained");
  assert!(!intersects_frustum([[1.0001,-0.5,-0.5],[2.,0.5,0.5]],p).unwrap(),"box beyond +x face rejected");
  assert!(!intersects_frustum([[-2.,-0.5,-0.5],[-1.0001,0.5,0.5]],p).unwrap(),"box beyond -x face rejected");
  assert!(!intersects_frustum([[-0.5,1.0001,-0.5],[0.5,2.,0.5]],p).unwrap(),"box beyond +y face rejected");
  assert!(!intersects_frustum([[-0.5,-0.5,1.0001],[0.5,0.5,2.]],p).unwrap(),"box beyond +z face rejected");
  let mut bad=p;bad[0]=[0.,0.,0.,1.];assert_eq!(intersects_frustum([[0.;3],[1.;3]],bad).unwrap_err(),"frustum plane normal nonzero required");
  assert_eq!(intersects_frustum([[1.;3],[0.;3]],p).unwrap_err(),"finite ordered bounds and planes required");
  let mut nan=[[0.;3],[1.;3]];nan[0][1]=f64::NAN;assert_eq!(intersects_frustum(nan,p).unwrap_err(),"finite ordered bounds and planes required");}
}
