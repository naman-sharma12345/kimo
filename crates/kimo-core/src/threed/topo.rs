//! Welded-topology census for extracted meshes (exact-bit weld). Used by the
//! contour wrapper: closed/manifold/winding/volume verification plus sliver count.
use std::collections::HashMap;
pub struct Census{pub welded_vertices:usize,pub boundary_edges:usize,pub nonmanifold_edges:usize,pub winding_consistent:bool,pub euler_characteristic:i64,pub signed_volume:f64,pub min_triangle_area:f64,pub sliver_triangles:usize}
pub fn census(positions:&[[f64;3]],indices:&[[u32;3]])->Census{
 let mut weld:HashMap<[u64;3],u32>=HashMap::new();let mut ids=Vec::with_capacity(positions.len());
 for p in positions{let k=[p[0].to_bits(),p[1].to_bits(),p[2].to_bits()];let n=weld.len() as u32;ids.push(*weld.entry(k).or_insert(n));}
 let nv=weld.len();
 let mut edge_dirs:HashMap<(u32,u32),Vec<(u32,u32)>>=HashMap::new();
 for t in indices{let t=[ids[t[0] as usize],ids[t[1] as usize],ids[t[2] as usize]];for k in 0..3{let(a,b)=(t[k],t[(k+1)%3]);edge_dirs.entry((a.min(b),a.max(b))).or_default().push((a,b));}}
 let mut boundary=0;let mut nonmanifold=0;let mut consistent=true;
 for(_,v)in &edge_dirs{if v.len()==1{boundary+=1;}else if v.len()!=2{nonmanifold+=1;}else if v[0]==v[1]||v[0].0!=v[1].1{consistent=false;}}
 let euler=nv as i64-edge_dirs.len() as i64+indices.len() as i64;
 let mut vol=0.0;let mut min_a=f64::INFINITY;
 let mut lo=[f64::INFINITY;3];let mut hi=[f64::NEG_INFINITY;3];for p in positions{for j in 0..3{lo[j]=lo[j].min(p[j]);hi[j]=hi[j].max(p[j]);}}
 let extent=(hi[0]-lo[0]).max(hi[1]-lo[1]).max(hi[2]-lo[2]);
 for t in indices{let[a,b,c]=t.map(|i|positions[i as usize]);vol+=(a[0]*(b[1]*c[2]-b[2]*c[1])-a[1]*(b[0]*c[2]-b[2]*c[0])+a[2]*(b[0]*c[1]-b[1]*c[0]))/6.0;
  let ab=[b[0]-a[0],b[1]-a[1],b[2]-a[2]];let ac=[c[0]-a[0],c[1]-a[1],c[2]-a[2]];let n=[ab[1]*ac[2]-ab[2]*ac[1],ab[2]*ac[0]-ab[0]*ac[2],ab[0]*ac[1]-ab[1]*ac[0]];let area=0.5*(n[0]*n[0]+n[1]*n[1]+n[2]*n[2]).sqrt();if area<min_a{min_a=area;}}
 let sliver_floor=extent*extent*1e-14;
 let slivers=if indices.is_empty(){0}else{indices.iter().filter(|t|{let[a,b,c]=t.map(|i|positions[i as usize]);let ab=[b[0]-a[0],b[1]-a[1],b[2]-a[2]];let ac=[c[0]-a[0],c[1]-a[1],c[2]-a[2]];let n=[ab[1]*ac[2]-ab[2]*ac[1],ab[2]*ac[0]-ab[0]*ac[2],ab[0]*ac[1]-ab[1]*ac[0]];0.5*(n[0]*n[0]+n[1]*n[1]+n[2]*n[2]).sqrt()<=sliver_floor}).count()};
 if indices.is_empty(){min_a=0.;}
 Census{welded_vertices:nv,boundary_edges:boundary,nonmanifold_edges:nonmanifold,winding_consistent:consistent,euler_characteristic:euler,signed_volume:vol,min_triangle_area:min_a,sliver_triangles:slivers}
}
#[cfg(test)]mod tests{use super::*;#[test]fn closed_tetra_verifies(){let s=3f64.sqrt();let p=vec![[1.,1.,1.],[1.,-1.,-1.],[-1.,1.,-1.],[-1.,-1.,1.]];let i=vec![[0,1,2],[0,3,1],[0,2,3],[1,3,2]];let c=census(&p,&i);assert_eq!((c.boundary_edges,c.nonmanifold_edges,c.euler_characteristic),(0,0,2));assert!(c.winding_consistent&&c.signed_volume>0.,"volume {}",c.signed_volume);let _=s;}
#[test]fn crack_and_flip_detected(){let p=vec![[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.,0.,0.],[1.,0.,0.],[0.,1.,0.000001]];let i=vec![[0,1,2],[3,5,4]];let c=census(&p,&i);assert_eq!(c.boundary_edges,4,"open pair: {}",c.boundary_edges);assert!(c.winding_consistent);let p2=vec![[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]];let i2=vec![[0,1,2],[0,2,3],[0,3,1],[1,3,2]];let c2=census(&p2,&i2);assert!(c2.winding_consistent,"consistent tet {}",c2.signed_volume);let i3=vec![[0,1,2],[0,2,3],[0,3,1],[1,2,3]];assert!(!census(&p2,&i3).winding_consistent,"flipped last tri must be caught");}}
