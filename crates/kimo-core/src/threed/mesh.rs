use serde::{Serialize,Deserialize};
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]
pub struct Mesh {pub positions:Vec<[f64;3]>,pub indices:Vec<[u32;3]>}
impl Mesh {
 pub fn validate(&self)->Result<(),String>{
  if self.positions.is_empty()||self.positions.len()>1_000_000||self.indices.is_empty()||self.indices.len()>1_000_000{return Err("mesh requires1..1000000 positions and triangles".into());}
  if self.positions.iter().flatten().any(|v|!v.is_finite()||v.abs()>1e12){return Err("mesh positions finite and abs<=1e12 required".into());}
  if self.indices.iter().flatten().any(|i|*i as usize>=self.positions.len()){return Err("mesh index outside position buffer".into());}Ok(())
 }
 pub fn bounds(&self)->Result<[[f64;3];2],String>{self.validate()?;let mut lo=[f64::INFINITY;3];let mut hi=[f64::NEG_INFINITY;3];for p in &self.positions{for j in 0..3{lo[j]=lo[j].min(p[j]);hi[j]=hi[j].max(p[j]);}}Ok([lo,hi])}
}
pub type Matrix=[[f64;4];4];
pub const IDENTITY:Matrix=[[1.,0.,0.,0.],[0.,1.,0.,0.],[0.,0.,1.,0.],[0.,0.,0.,1.]];
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]
pub struct Node {pub parent:Option<usize>,pub local:Matrix,pub mesh:Option<usize>}
fn multiply(a:Matrix,b:Matrix)->Result<Matrix,String>{let mut o=[[0.;4];4];for i in 0..4{for j in 0..4{for k in 0..4{o[i][j]+=a[i][k]*b[k][j];}if !o[i][j].is_finite(){return Err("world matrix overflow".into());}}}Ok(o)}
pub fn world_matrices(nodes:&[Node],mesh_count:usize)->Result<Vec<Matrix>,String>{
 if nodes.len()>256{return Err("max256 scene nodes".into());}
 for n in nodes{if n.local.iter().flatten().any(|v|!v.is_finite()||v.abs()>1e12)||n.local[3]!=[0.,0.,0.,1.]{return Err("finite bounded affine4x4 required".into());}if n.mesh.map(|i|i>=mesh_count).unwrap_or(false){return Err("node mesh missing".into());}}
 fn visit(i:usize,n:&[Node],state:&mut[u8],cache:&mut[Matrix])->Result<Matrix,String>{if state[i]==1{return Err("scene parent cycle".into());}if state[i]==2{return Ok(cache[i]);}state[i]=1;let m=if let Some(p)=n[i].parent{if p>=n.len(){return Err("scene parent missing".into());}multiply(visit(p,n,state,cache)?,n[i].local)?}else{n[i].local};cache[i]=m;state[i]=2;Ok(m)}
 let mut state=vec![0;nodes.len()];let mut out=vec![IDENTITY;nodes.len()];for i in 0..nodes.len(){visit(i,nodes,&mut state,&mut out)?;}Ok(out)
}
pub fn transform_mesh(mesh:&Mesh,m:Matrix)->Result<Mesh,String>{mesh.validate()?;if m[3]!=[0.,0.,0.,1.]||m.iter().flatten().any(|v|!v.is_finite()){return Err("finite affine transform required".into());}let mut out=mesh.clone();for p in &mut out.positions{let q=*p;for i in 0..3{p[i]=m[i][0]*q[0]+m[i][1]*q[1]+m[i][2]*q[2]+m[i][3];}}out.validate()?;Ok(out)}

#[cfg(test)]
mod tests{
 use super::*;
 fn tri()->Mesh{Mesh{positions:vec![[1.,-2.,3.],[-4.,5.,-6.],[7.,8.,9.]],indices:vec![[0,1,2]]}}
 #[test]fn bounds_exact_on_known_positions(){let m=tri();let[lo,hi]=m.bounds().unwrap();assert_eq!(lo,[-4.,-2.,-6.]);assert_eq!(hi,[7.,8.,9.]);}
 #[test]fn validate_typed_errors(){let empty_p=Mesh{positions:vec![],indices:vec![[0,0,0]]};assert_eq!(empty_p.validate().unwrap_err(),"mesh requires1..1000000 positions and triangles");
  let empty_i=Mesh{positions:vec![[0.;3]],indices:vec![]};assert_eq!(empty_i.validate().unwrap_err(),"mesh requires1..1000000 positions and triangles");
  let mut nan=tri();nan.positions[1][2]=f64::NAN;assert_eq!(nan.validate().unwrap_err(),"mesh positions finite and abs<=1e12 required");
  let mut huge=tri();huge.positions[0][0]=1e12+1.;assert_eq!(huge.validate().unwrap_err(),"mesh positions finite and abs<=1e12 required");
  let mut oob=tri();oob.indices[0][2]=3;assert_eq!(oob.validate().unwrap_err(),"mesh index outside position buffer");
  let mut edge=tri();edge.positions[2][1]=1e12;edge.positions[2][2]=-1e12;assert!(edge.validate().is_ok(),"abs == 1e12 boundary accepted");}
 const MAT_AC:[[f64;4];4]=[[0.,-0.5,0.,5.],[2.,0.,0.,-2.],[0.,0.,2.,7.],[0.,0.,0.,1.]];
 // MAT_*/XFORM_* are independent Python (stdlib) 4x4 products and point transforms, pinned at 1e-12.
 pub const MAT_A:[[f64;4];4]=[[0.00000000000000000e+00,-1.00000000000000000e+00,0.00000000000000000e+00,5.00000000000000000e+00],[1.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00,-2.00000000000000000e+00],[0.00000000000000000e+00,0.00000000000000000e+00,2.00000000000000000e+00,1.00000000000000000e+00],[0.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000000e+00]];
 pub const MAT_B:[[f64;4];4]=[[1.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00,5.00000000000000000e-01],[0.00000000000000000e+00,0.00000000000000000e+00,-1.00000000000000000e+00,2.50000000000000000e-01],[0.00000000000000000e+00,1.00000000000000000e+00,0.00000000000000000e+00,-7.50000000000000000e-01],[0.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000000e+00]];
 pub const MAT_C:[[f64;4];4]=[[2.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00],[0.00000000000000000e+00,5.00000000000000000e-01,0.00000000000000000e+00,0.00000000000000000e+00],[0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000000e+00,3.00000000000000000e+00],[0.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000000e+00]];
 pub const MAT_AB:[[f64;4];4]=[[0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000000e+00,4.75000000000000000e+00],[1.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00,-1.50000000000000000e+00],[0.00000000000000000e+00,2.00000000000000000e+00,0.00000000000000000e+00,-5.00000000000000000e-01],[0.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000000e+00]];
 pub const MAT_ABC:[[f64;4];4]=[[0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000000e+00,7.75000000000000000e+00],[2.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00,-1.50000000000000000e+00],[0.00000000000000000e+00,1.00000000000000000e+00,0.00000000000000000e+00,-5.00000000000000000e-01],[0.00000000000000000e+00,0.00000000000000000e+00,0.00000000000000000e+00,1.00000000000000000e+00]];
 pub const XFORM_POINTS:[[f64;3];2]=[[1.00000000000000000e+00,2.00000000000000000e+00,3.00000000000000000e+00],[-5.00000000000000000e-01,2.50000000000000000e-01,4.00000000000000000e+00]];
 pub const XFORM_EXPECTED:[[f64;3];2]=[[1.07500000000000000e+01,5.00000000000000000e-01,1.50000000000000000e+00],[1.17500000000000000e+01,-2.50000000000000000e+00,-2.50000000000000000e-01]];
 fn close(a:f64,b:f64)->bool{(a-b).abs()<=1e-12*b.abs().max(1.0)}
 fn closem(a:Matrix,b:Matrix)->bool{(0..4).all(|i|(0..4).all(|j|close(a[i][j],b[i][j])))}
 fn node(local:Matrix,parent:Option<usize>,mesh:Option<usize>)->Node{Node{parent,local,mesh}}
 fn mesh3(p:[[f64;3];3])->Mesh{Mesh{positions:p.to_vec(),indices:vec![[0,1,2]]}}

 #[test]fn world_matrices_match_python_oracle_chain(){let nodes=vec![node(MAT_A,None,None),node(MAT_B,Some(0),None),node(MAT_C,Some(1),None)];
  let w=world_matrices(&nodes,0).unwrap();assert!(closem(w[0],MAT_A),"root local is world");assert!(closem(w[1],MAT_AB),"child = parent*local");assert!(closem(w[2],MAT_ABC),"grandchild composes full chain");}
 #[test]fn world_matrices_shared_parent_and_child_listed_first(){let nodes=vec![node(MAT_B,Some(2),None),node(MAT_C,Some(2),None),node(MAT_A,None,None)];
  let w=world_matrices(&nodes,0).unwrap();assert!(closem(w[2],MAT_A));assert!(closem(w[0],MAT_AB),"sibling 1 composes with shared root regardless of listing order");assert!(closem(w[1],MAT_AC),"sibling 2 composes with shared root regardless of listing order");}
 #[test]fn world_matrices_typed_errors(){let big=vec![node(IDENTITY,None,None);257];assert_eq!(world_matrices(&big,0).unwrap_err(),"max256 scene nodes");
  let cyc=vec![node(IDENTITY,Some(1),None),node(IDENTITY,Some(0),None)];assert_eq!(world_matrices(&cyc,0).unwrap_err(),"scene parent cycle");
  let missing=vec![node(IDENTITY,Some(7),None)];assert_eq!(world_matrices(&missing,0).unwrap_err(),"scene parent missing");
  let nomesh=vec![node(IDENTITY,None,Some(0))];assert_eq!(world_matrices(&nomesh,0).unwrap_err(),"node mesh missing");
  let mut bad=IDENTITY;bad[3]=[0.,0.,0.,2.];assert_eq!(world_matrices(&[node(bad,None,None)],0).unwrap_err(),"finite bounded affine4x4 required");
  let mut nan=IDENTITY;nan[0][0]=f64::NAN;assert_eq!(world_matrices(&[node(nan,None,None)],0).unwrap_err(),"finite bounded affine4x4 required");
  let mut huge=IDENTITY;huge[0][3]=1e12+1.;assert_eq!(world_matrices(&[node(huge,None,None)],0).unwrap_err(),"finite bounded affine4x4 required");}
 #[test]fn transform_mesh_matches_oracle_and_validates(){let m=mesh3([XFORM_POINTS[0],XFORM_POINTS[1],[0.,0.,0.]]);let out=transform_mesh(&m,MAT_ABC).unwrap();
  for i in 0..2{for j in 0..3{assert!(close(out.positions[i][j],XFORM_EXPECTED[i][j]),"point {i} axis {j}");}}
  for j in 0..3{assert!(close(out.positions[2][j],[7.75,-1.5,-0.5][j]),"origin maps to translation column");}
  let mut bad=MAT_ABC;bad[3]=[0.,0.,0.,0.];assert_eq!(transform_mesh(&m,bad).unwrap_err(),"finite affine transform required");
  let mut nan=MAT_ABC;nan[1][2]=f64::NAN;assert_eq!(transform_mesh(&m,nan).unwrap_err(),"finite affine transform required");}
}
