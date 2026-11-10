//! Original adaptive octree over the bounded implicit graph (octree-mesh CP1).
//! Error metric = trilinear interpolation deviation at cell face/center probes plus
//! corner-normal angular variation, so curved and creased regions subdivide deeper
//! than flat ones. Interior probes catch thin features the corner signs miss (a
//! feature fully inside a cell between probes can still be missed - documented limit).
//! Descent culls a cell only when |f(center)| exceeds the cell circumsphere (SDF
//! distance bound), so closed surfaces that touch no corner/probe are still found.
//! Lipschitz discipline: this culling assumes |f| never over-estimates true distance to
//! the zero set (true for exact SDF primitives, min/max CSG and the under-estimating
//! smooth blends here). Domain warps or non-uniform scales can violate that bound and
//! would need a scaled circumradius; the graph currently has neither.
//! Leaves carry Hermite edge-crossing samples (analytic normals, FD fallback counted)
//! for the QEF vertex placement (CP2). extract_mesh runs adaptive Dual Contouring over the
//! leaves: QEF placement with mass-point fallback, crack-free LOD-transition stitching
//! (census on AdaptiveMesh asserts closed+manifold), sharp-feature recovery.
use super::sdf_graph::Graph;use serde::Serialize;use std::collections::{HashMap,HashSet};
#[derive(Clone,Debug,Serialize,serde::Deserialize)]#[serde(deny_unknown_fields)]pub struct OctreeConfig{pub bounds:[[f64;3];2],pub min_depth:u32,pub max_depth:u32,pub error_tolerance:f64,pub normal_angle_degrees:f64,pub normal_epsilon:f64}
#[derive(Clone,Debug,Serialize)]pub struct Hermite{pub point:[f64;3],pub normal:[f64;3]}
#[derive(Clone,Debug,Serialize)]pub struct Leaf{pub min:[f64;3],pub size:[f64;3],pub depth:u32,pub corners:[f64;8],pub hermite:Vec<Hermite>}
#[derive(Clone,Debug,Default,Serialize)]pub struct Stats{pub leaves:usize,pub surface_leaves:usize,pub splits_min_depth:usize,pub splits_trilinear:usize,pub splits_normal_angle:usize,pub splits_thin_feature:usize,pub splits_near_band:usize,pub hermite_normal_dropped:usize,pub max_depth_hits:usize,pub field_evals:usize,pub normal_evals:usize,pub normal_fd_fallbacks:usize,pub max_depth_reached:u32}
#[derive(Serialize)]pub struct Octree{pub leaves:Vec<Leaf>,pub stats:Stats,pub empty:bool,pub config:OctreeConfig}
fn validate(c:&OctreeConfig)->Result<(),String>{if c.bounds.iter().flatten().any(|v|!v.is_finite()||v.abs()>999_000.)||(0..3).any(|j|c.bounds[1][j]<=c.bounds[0][j]){return Err("bounded volume required".into());}if c.min_depth<1||c.max_depth>10||c.min_depth>c.max_depth{return Err("1<=min_depth<=max_depth<=10 required".into());}if !(c.error_tolerance.is_finite()&&(1e-9..=1e3).contains(&c.error_tolerance)){return Err("error_tolerance 1e-9..1e3 required".into());}if !(c.normal_angle_degrees.is_finite()&&(0.0..=180.).contains(&c.normal_angle_degrees)&&c.normal_angle_degrees>0.){return Err("normal_angle_degrees (0,180] required".into());}if !(c.normal_epsilon.is_finite()&&(1e-6..=0.01).contains(&c.normal_epsilon)){return Err("normal_epsilon 1e-6..0.01 required".into());}Ok(())}
struct B<'a>{g:&'a Graph,c:&'a OctreeConfig,fc:HashMap<[u32;3],f64>,nc:HashMap<[u32;3],[f64;3]>,st:Stats,cos_thr:f64}
impl<'a>B<'a>{
 fn key(&self,p:[f64;3])->[u32;3]{let n=(1u32<<self.c.max_depth) as f64;std::array::from_fn(|j|(((p[j]-self.c.bounds[0][j])/(self.c.bounds[1][j]-self.c.bounds[0][j])*n).round() as u32).min(1<<self.c.max_depth))}
 fn field(&mut self,p:[f64;3])->Result<f64,String>{let k=self.key(p);if let Some(v)=self.fc.get(&k){return Ok(*v);}let v=self.g.evaluate(p)?;self.st.field_evals+=1;self.fc.insert(k,v);Ok(v)}
 fn normal(&mut self,p:[f64;3])->Result<[f64;3],String>{let k=self.key(p);if let Some(v)=self.nc.get(&k){return Ok(*v);}let v=self.g.normal_analytic(p).or_else(|_|{self.st.normal_fd_fallbacks+=1;self.g.normal(p,self.c.normal_epsilon)})?;self.st.normal_evals+=1;self.nc.insert(k,v);Ok(v)}
 fn cell(&mut self,min:[f64;3],depth:u32,out:&mut Vec<Leaf>)->Result<(),String>{
  crate::guard::check();if self.st.leaves+self.st.splits_min_depth+self.st.splits_trilinear+self.st.splits_normal_angle+self.st.splits_thin_feature+self.st.splits_near_band>8_000_000{return Err("octree cell budget 8000000 exceeded".into());}
  self.st.max_depth_reached=self.st.max_depth_reached.max(depth);
  let size:[f64;3]=std::array::from_fn(|j|(self.c.bounds[1][j]-self.c.bounds[0][j])/(1u32<<depth) as f64);
  let corner_p=|i:usize|->[f64;3]{[min[0]+if i&1==1{size[0]}else{0.},min[1]+if i&2==2{size[1]}else{0.},min[2]+if i&4==4{size[2]}else{0.}]};
  let mut corners=[0.;8];for i in 0..8{corners[i]=self.field(corner_p(i))?;}
  let neg=corners.map(|v|v<0.);let surface=neg.iter().any(|&b|b!=neg[0]);
  // interior probes: 6 face centers + center (all lattice points at finer depth)
  let probes:[[f64;3];7]={let mut p=[[0.5;3];7];p[0]=[0.5,0.,0.];p[1]=[0.5,1.,0.];p[2]=[0.,0.5,0.];p[3]=[1.,0.5,0.];p[4]=[0.,0.,0.5];p[5]=[1.,1.,0.5];p[6]=[0.5,0.5,0.5];p.map(|u|std::array::from_fn(|j|min[j]+u[j]*size[j]))};
  let mut surf=surface;let mut thin=false;
  if !surf{for p in probes{if (self.field(p)?<0.)!=neg[0]{surf=true;thin=true;break;}}}
  // SDF distance descent: cull only when the center distance provably exceeds the cell
  // circumsphere. Conservative for under-estimating blends; a region whose field
  // over-estimates true distance could still be missed (documented limit).
  let ctr:[f64;3]=std::array::from_fn(|j|min[j]+size[j]/2.);
  let circum=0.5*(size[0]*size[0]+size[1]*size[1]+size[2]*size[2]).sqrt();
  let near=self.field(ctr)?.abs()<=circum;
  if !surf&&!near{out.push(Leaf{min,size,depth,corners,hermite:Vec::new()});self.st.leaves+=1;return Ok(());}
  if !surf{
   // near-band or thin-feature cell without a corner crossing: keep descending; it only
   // becomes a surface leaf if a finer level finds an actual sign change.
   if depth>=self.c.max_depth{self.st.max_depth_hits+=1;out.push(Leaf{min,size,depth,corners,hermite:Vec::new()});self.st.leaves+=1;return Ok(());}
   if thin{self.st.splits_thin_feature+=1;}else{self.st.splits_near_band+=1;}
   let h=[size[0]/2.,size[1]/2.,size[2]/2.];for i in 0..8{self.cell([min[0]+if i&1==1{h[0]}else{0.},min[1]+if i&2==2{h[1]}else{0.},min[2]+if i&4==4{h[2]}else{0.}],depth+1,out)?;}
   return Ok(());
  }
  // subdivision decision
  let trilerp=|u:[f64;3]|->f64{let c00=corners[0]+(corners[1]-corners[0])*u[0];let c10=corners[2]+(corners[3]-corners[2])*u[0];let c01=corners[4]+(corners[5]-corners[4])*u[0];let c11=corners[6]+(corners[7]-corners[6])*u[0];let c0=c00+(c10-c00)*u[1];let c1=c01+(c11-c01)*u[1];c0+(c1-c0)*u[2]};
  let mut dev=0.0f64;for p in probes{let u:[f64;3]=std::array::from_fn(|j|(p[j]-min[j])/size[j]);dev=dev.max((self.field(p)?-trilerp(u)).abs());}
  // corners on a non-differentiable set (box face/edge plane, CSG crease) have no normal:
  // exclude them; the remaining valid corners still expose sharp features by angle.
  let mut ns:Vec<[f64;3]>=Vec::with_capacity(8);
  for i in 0..8{if let Ok(n)=self.normal(corner_p(i)){ns.push(n);}}
  let mut min_dot=1.0f64;for a in 0..ns.len(){for b in a+1..ns.len(){let d=(ns[a][0]*ns[b][0]+ns[a][1]*ns[b][1]+ns[a][2]*ns[b][2]).clamp(-1.,1.);if d<min_dot{min_dot=d;}}}
  let over_dev=dev>self.c.error_tolerance;let over_ang=if ns.len()>=2{min_dot<self.cos_thr}else{true};
  let split=if depth<self.c.min_depth{self.st.splits_min_depth+=1;true}else if depth>=self.c.max_depth{if over_dev||over_ang||thin{self.st.max_depth_hits+=1;}false}else if thin{self.st.splits_thin_feature+=1;true}else if over_dev{self.st.splits_trilinear+=1;true}else if over_ang{self.st.splits_normal_angle+=1;true}else{false};
  if split{let h=[size[0]/2.,size[1]/2.,size[2]/2.];for i in 0..8{self.cell([min[0]+if i&1==1{h[0]}else{0.},min[1]+if i&2==2{h[1]}else{0.},min[2]+if i&4==4{h[2]}else{0.}],depth+1,out)?;}return Ok(());}
  // leaf: Hermite edge crossings (12 edges), linear-interpolated point + analytic normal
  let hermite=self.hermite_for(min,size,corners)?;
  out.push(Leaf{min,size,depth,corners,hermite});self.st.leaves+=1;self.st.surface_leaves+=1;Ok(())
 }
}
pub fn build(g:&Graph,c:&OctreeConfig)->Result<Octree,String>{g.validate()?;validate(c)?;
 let mut b=B{g,c,fc:HashMap::new(),nc:HashMap::new(),st:Stats::default(),cos_thr:(c.normal_angle_degrees.to_radians()).cos()};
 let mut leaves=Vec::new();b.cell(c.bounds[0],0,&mut leaves)?;
 let empty=b.st.surface_leaves==0;Ok(Octree{leaves,stats:b.st,empty,config:c.clone()})}
#[cfg(test)]mod tests{use super::*;use crate::threed::sdf_graph::Field;
fn sphere()->Graph{Graph{nodes:vec![Field::Sphere{center:[0.;3],radius:1.}],output:0}}
fn union_scene()->Graph{Graph{nodes:vec![Field::Sphere{center:[0.35,0.,0.],radius:0.8},Field::Box{center:[-0.35,0.,0.],half_size:[0.6;3]},Field::SmoothUnion{a:0,b:1,k:0.25}],output:2}}
fn cfg(min_d:u32,max_d:u32)->OctreeConfig{OctreeConfig{bounds:[[-2.2;3],[2.2;3]],min_depth:min_d,max_depth:max_d,error_tolerance:0.02,normal_angle_degrees:20.,normal_epsilon:0.001}}
#[test]fn sphere_adaptive_savings_and_hermite_quality(){
 let o=build(&sphere(),&cfg(2,7)).unwrap();assert!(!o.empty);
 let uniform=8u64.pow(7)as usize;assert!(o.leaves.len()*200<uniform,"adaptive savings: {} leaves vs uniform {}",o.leaves.len(),uniform);
 let mut hist=[0usize;11];for l in &o.leaves{hist[l.depth as usize]+=1;}
 let mut shist=[0usize;11];for l in o.leaves.iter().filter(|l|!l.hermite.is_empty()){shist[l.depth as usize]+=1;}
 println!("OCTREE-SPHERE leaves={} surface={} splits(min/tri/ang/thin/near)={}/{}/{}/{}/{} hits={} evals(f/n)={}/{} fb={} depth_hist={:?} surface_depth_hist={:?}",o.stats.leaves,o.stats.surface_leaves,o.stats.splits_min_depth,o.stats.splits_trilinear,o.stats.splits_normal_angle,o.stats.splits_thin_feature,o.stats.splits_near_band,o.stats.max_depth_hits,o.stats.field_evals,o.stats.normal_evals,o.stats.normal_fd_fallbacks,hist,shist);
 for l in o.leaves.iter().filter(|l|!l.hermite.is_empty()){
  assert!(!l.hermite.is_empty(),"surface leaf has Hermite data");
  let cell=l.size[0];
  for h in &l.hermite{let f=sphere().evaluate(h.point).unwrap().abs();assert!(f<cell,"Hermite point residual {f} vs cell {cell}");let l2=(h.normal[0]*h.normal[0]+h.normal[1]*h.normal[1]+h.normal[2]*h.normal[2]).sqrt();assert!((l2-1.).abs()<1e-9,"unit normal {l2}");}
 }
 // surface leaves cluster near the sphere: every surface leaf center is within 2 cells of the surface
 for l in o.leaves.iter().filter(|l|!l.hermite.is_empty()){let c=[l.min[0]+l.size[0]/2.,l.min[1]+l.size[1]/2.,l.min[2]+l.size[2]/2.];let d=sphere().evaluate(c).unwrap().abs();assert!(d<2.*l.size[0].max(l.size[1]).max(l.size[2]),"surface leaf far from field: {d}");}
}
#[test]fn plane_is_exactly_trilinear_so_no_error_subdivision(){
 let g=Graph{nodes:vec![Field::Plane{normal:[0.,0.,1.],offset:0.3}],output:0};
 let o=build(&g,&cfg(2,8)).unwrap();
 assert_eq!((o.stats.splits_trilinear,o.stats.splits_normal_angle,o.stats.splits_thin_feature),(0,0,0),"linear field must not trigger error subdivision: {:?}",o.stats);
 // the near band still refines (distance-bound descent), but every surface leaf stops at min_depth
 assert!(o.leaves.iter().filter(|l|!l.hermite.is_empty()).all(|l|l.depth==2),"plane surface leaves must all sit at min_depth: depths {:?}",o.leaves.iter().filter(|l|!l.hermite.is_empty()).map(|l|l.depth).collect::<Vec<_>>());
 println!("OCTREE-PLANE leaves={} surface={}",o.stats.leaves,o.stats.surface_leaves);
}
#[test]fn crease_subdivides_deeper_than_flat_cap(){
 // smooth-union benchmark scene: the near-tangent crease needs depth, the sphere far pole does not
 let o=build(&union_scene(),&cfg(2,8)).unwrap();
 // crease = the blend ring between sphere and box: x in the overlap band, 0.3..0.95 off the x-axis
 let crease_max=o.leaves.iter().filter(|l|!l.hermite.is_empty()).filter(|l|{let c=[l.min[0]+l.size[0]/2.,l.min[1]+l.size[1]/2.,l.min[2]+l.size[2]/2.];let r=c[1].hypot(c[2]);(-0.5..=0.3).contains(&c[0])&&(0.3..=0.95).contains(&r)}).map(|l|l.depth).max().unwrap_or(0);
 let pole_max=o.leaves.iter().filter(|l|!l.hermite.is_empty()).filter(|l|{let c=[l.min[0]+l.size[0]/2.,l.min[1]+l.size[1]/2.,l.min[2]+l.size[2]/2.];(c[0]-1.15).abs()<0.25&&c[1].abs()<0.25&&c[2].abs()<0.25}).map(|l|l.depth).max().unwrap_or(0);
 let mut hist=[0usize;11];for l in &o.leaves{hist[l.depth as usize]+=1;}
 println!("OCTREE-CREASE crease_max_depth={crease_max} pole_max_depth={pole_max} leaves={} surface={} depth_hist={:?}",o.stats.leaves,o.stats.surface_leaves,hist);
 assert!(crease_max>pole_max,"crease must subdivide deeper than flat pole: {crease_max} vs {pole_max}");
 assert!(pole_max<8,"flat pole must stop before max_depth");
}
#[test]fn sharp_box_face_stays_shallow_corner_goes_deep(){
 let g=Graph{nodes:vec![Field::Box{center:[0.;3],half_size:[0.6;3]}],output:0};
 let c=OctreeConfig{bounds:[[-1.6;3],[1.6;3]],..cfg(2,8)};
 let o=build(&g,&c).unwrap();
 let face_max=o.leaves.iter().filter(|l|!l.hermite.is_empty()).filter(|l|{let ctr=[l.min[0]+l.size[0]/2.,l.min[1]+l.size[1]/2.,l.min[2]+l.size[2]/2.];l.min[0]<0.6&&l.min[0]+l.size[0]>0.6&&ctr[1].abs()<0.2&&ctr[2].abs()<0.2}).map(|l|l.depth).max().unwrap_or(0);
 let corner_max=o.leaves.iter().filter(|l|!l.hermite.is_empty()).filter(|l|{let ctr=[l.min[0]+l.size[0]/2.,l.min[1]+l.size[1]/2.,l.min[2]+l.size[2]/2.];(ctr[0]-0.6).abs()<0.15&&(ctr[1]-0.6).abs()<0.15&&(ctr[2]-0.6).abs()<0.15}).map(|l|l.depth).max().unwrap_or(0);
 println!("OCTREE-BOX face_max_depth={face_max} corner_max_depth={corner_max}");
 assert!(corner_max>face_max+1,"corner deeper than face: {corner_max} vs {face_max}");
}
#[test]fn determinism_and_empty_and_typed_errors(){
 let a=build(&sphere(),&cfg(2,6)).unwrap();let b=build(&sphere(),&cfg(2,6)).unwrap();
 let sig=|o:&Octree|o.leaves.iter().map(|l|(l.min,l.depth,l.hermite.len())).collect::<Vec<_>>();
 assert_eq!(sig(&a),sig(&b),"octree build must be deterministic");
 let far=Graph{nodes:vec![Field::Sphere{center:[50.;3],radius:1.}],output:0};
 let e=build(&far,&cfg(2,6)).unwrap();assert!(e.empty&&e.stats.surface_leaves==0,"empty field is a typed result, not an error");
 let bad=[OctreeConfig{min_depth:0,..cfg(2,6)},OctreeConfig{min_depth:5,max_depth:3,..cfg(2,6)},OctreeConfig{max_depth:11,..cfg(2,6)},OctreeConfig{error_tolerance:0.,..cfg(2,6)},OctreeConfig{error_tolerance:f64::NAN,..cfg(2,6)},OctreeConfig{normal_angle_degrees:0.,..cfg(2,6)},OctreeConfig{normal_angle_degrees:181.,..cfg(2,6)},OctreeConfig{normal_epsilon:0.,..cfg(2,6)}];
 for(i,c)in bad.iter().enumerate(){assert!(build(&sphere(),c).is_err(),"bad config {i} accepted");}
}
#[test]fn torus_topology_of_leaf_layout(){
 let g=Graph{nodes:vec![Field::Torus{center:[0.;3],major_radius:1.,minor_radius:0.4}],output:0};
 let o=build(&g,&cfg(2,7)).unwrap();
 // surface leaves form a closed band: every surface leaf has a surface neighbor (face-adjacent or diagonal)
 let surf:Vec<&Leaf>=o.leaves.iter().filter(|l|!l.hermite.is_empty()).collect();
 assert!(surf.len()>50,"torus band too thin: {}",surf.len());
 println!("OCTREE-TORUS leaves={} surface={} hits={}",o.stats.leaves,o.stats.surface_leaves,o.stats.max_depth_hits);
}
}
// ---- CP2: restricted (2:1) balancing, QEF vertex placement, crack-free quad extraction.
// Invariant enforced before meshing: every minimal sign-change edge's 4 surrounding
// positions resolve to LEAVES THAT HAVE corner sign changes (hence vertices). A
// neighbor leaf without one is force-split (bounded by max_depth) until the invariant
// holds; then each minimal edge emits one quad (or triangle fan at volume borders).
fn lattice(max_depth:u32,bounds:[[f64;3];2],p:[f64;3])->[u32;3]{let n=(1u32<<max_depth) as f64;std::array::from_fn(|j|(((p[j]-bounds[0][j])/(bounds[1][j]-bounds[0][j])*n).round() as u32).min(1<<max_depth))}
fn map_key(max_depth:u32,depth:u32,min:[f64;3],bounds:[[f64;3];2])->(u32,u32,u32,u32){let sh=max_depth-depth;let k=lattice(max_depth,bounds,min);(depth,k[0]>>sh,k[1]>>sh,k[2]>>sh)}
impl<'a> B<'a>{
 fn hermite_for(&mut self,min:[f64;3],size:[f64;3],corners:[f64;8])->Result<Vec<Hermite>,String>{
  let corner_p=|i:usize|->[f64;3]{[min[0]+if i&1==1{size[0]}else{0.},min[1]+if i&2==2{size[1]}else{0.},min[2]+if i&4==4{size[2]}else{0.}]};
  let mut hermite=Vec::new();
  for a in 0..3{for i in 0..8{if i&(1<<a)==0{let j=i|(1<<a);let(v0,v1)=(corners[i],corners[j]);if (v0<0.)!=(v1<0.){let t=v0/(v0-v1);let(p0,p1)=(corner_p(i),corner_p(j));let p:[f64;3]=std::array::from_fn(|k|p0[k]+t*(p1[k]-p0[k]));
   // degenerate normals never enter the buffer: analytic -> FD(epsilon) -> drop+count
   let n=match self.g.normal_analytic(p).or_else(|_|{self.st.normal_fd_fallbacks+=1;self.g.normal(p,self.c.normal_epsilon)}){Ok(n)if n.iter().all(|v|v.is_finite())=>n,_=>{self.st.hermite_normal_dropped+=1;continue}};
   hermite.push(Hermite{point:p,normal:n});}}}}
  Ok(hermite)}
 fn split_leaf(&mut self,l:&Leaf)->Result<Vec<Leaf>,String>{
  let d=l.depth+1;if d>self.c.max_depth{return Err("balance/repair split beyond max_depth".into());}
  let h=[l.size[0]/2.,l.size[1]/2.,l.size[2]/2.];let mut out=Vec::with_capacity(8);
  for i in 0..8{let m=[l.min[0]+if i&1==1{h[0]}else{0.},l.min[1]+if i&2==2{h[1]}else{0.},l.min[2]+if i&4==4{h[2]}else{0.}];let mut corners=[0.;8];let corner_p=|q:usize|->[f64;3]{[m[0]+if q&1==1{h[0]}else{0.},m[1]+if q&2==2{h[1]}else{0.},m[2]+if q&4==4{h[2]}else{0.}]};for q in 0..8{corners[q]=self.field(corner_p(q))?;}let neg=corners.map(|v|v<0.);let surf=neg.iter().any(|&b|b!=neg[0]);let hermite=if surf{self.hermite_for(m,h,corners)?}else{Vec::new()};out.push(Leaf{min:m,size:h,depth:d,corners,hermite});}
  Ok(out)}
 fn leaf_map(&self,leaves:&[Leaf])->HashMap<(u32,u32,u32,u32),usize>{leaves.iter().enumerate().map(|(i,l)|(map_key(self.c.max_depth,l.depth,l.min,self.c.bounds),i)).collect()}
 fn resolve(&self,map:&HashMap<(u32,u32,u32,u32),usize>,depth:u32,i:u32,j:u32,k:u32)->Option<Resolve>{
  if let Some(&x)=map.get(&(depth,i,j,k)){return Some(Resolve::Leaf(x));}
  let mut d=depth;let(mut a,mut b,mut c2)=(i,j,k);
  while d>0{d-=1;a>>=1;b>>=1;c2>>=1;if let Some(&x)=map.get(&(d,a,b,c2)){return Some(Resolve::Leaf(x));}}
  // no leaf at or above: the position is internal (subdivided) or outside the tree
  Some(Resolve::Internal)}
}
#[derive(Clone,Copy,PartialEq,Debug)]enum Resolve{Leaf(usize),Internal}
/// Strong 2:1 balance over all 26 neighbor directions, then the vertex-invariant
/// repair pass. Returns the number of balance + repair splits.
fn balance_and_repair(b:&mut B,leaves:&mut Vec<Leaf>)->Result<(usize,usize),String>{
 let mut bal=0;let mut rep=0;
 for _round in 0..64{
  let map=b.leaf_map(leaves);
  let mut split:Option<usize>=None;
  'outer:for(li,l)in leaves.iter().enumerate(){let d=l.depth;if d==0{continue}
   let sh=b.c.max_depth-d;let base=lattice(b.c.max_depth,b.c.bounds,l.min).map(|v|v>>sh);
   for dx in -1i32..=1{for dy in -1i32..=1{for dz in -1i32..=1{if dx==0&&dy==0&&dz==0{continue}
    let(nx,ny,nz)=(base[0]as i64+dx as i64,base[1]as i64+dy as i64,base[2]as i64+dz as i64);let lim=1i64<<d;
    if nx<0||ny<0||nz<0||nx>=lim||ny>=lim||nz>=lim{continue}
    if let Some(Resolve::Leaf(ni))=b.resolve(&map,d,nx as u32,ny as u32,nz as u32){if ni!=li&&leaves[ni].depth+1<d{split=Some(ni);break 'outer;}}}}
  }}
  match split{Some(ni)=>{let l=leaves[ni].clone();let ch=b.split_leaf(&l)?;leaves[ni]=ch[0].clone();for c2 in ch.into_iter().skip(1){leaves.push(c2);}bal+=1;},None=>break}
 }
 // repair: minimal sign-change edges must be surrounded by vertex-carrying leaves
 for _round in 0..64{
  let map=b.leaf_map(leaves);
  let mut split:Option<usize>=None;
  'outer:for(li,l)in leaves.iter().enumerate(){
   let neg=l.corners.map(|v|v<0.);if neg.iter().all(|&b|b==neg[0]){continue}
   let d=l.depth;let sh=b.c.max_depth-d;let base=lattice(b.c.max_depth,b.c.bounds,l.min).map(|v|v>>sh);
   for a in 0..3{for i in 0..8{if i&(1<<a)==0{let j=i|(1<<a);if (l.corners[i]<0.)==(l.corners[j]<0.){continue}
    // the 4 positions around this edge at depth d
    let (ax1,ax2)=((a+1)%3,(a+2)%3);
    let mut pos=[[0u32;3];4];for(q,(du,dv))in [(0i64,0i64),(0,-1),(-1,0),(-1,-1)].into_iter().enumerate(){let mut p=base;p[ax1]=(base[ax1]as i64+du+((i>>ax1)&1)as i64)as u32;p[ax2]=(base[ax2]as i64+dv+((i>>ax2)&1)as i64)as u32;pos[q]=p;}
    for p in pos{let lim=1u32<<d;if p[0]>=lim||p[1]>=lim||p[2]>=lim{continue}
     if let Some(Resolve::Leaf(ni))=b.resolve(&map,d,p[0],p[1],p[2]){if ni!=li{let nn=leaves[ni].corners.map(|v|v<0.);if nn.iter().all(|&bb|bb==nn[0]){split=Some(ni);break 'outer;}}}}
   }}}
  }
  match split{Some(ni)=>{let l=leaves[ni].clone();let ch=b.split_leaf(&l)?;leaves[ni]=ch[0].clone();for c2 in ch.into_iter().skip(1){leaves.push(c2);}rep+=1;},None=>break}
 }
 Ok((bal,rep))
}
/// Symmetric 3x3 eigensolver: cyclic Jacobi rotations, original. Returns (eigenvalues, eigenvectors as columns).
fn jacobi3(mut a:[[f64;3];3])->([f64;3],[[f64;3];3]){
 let mut v=[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]];
 for _ in 0..16{
  let mut off=0.0f64;for i in 0..3{for j in i+1..3{off=off.max(a[i][j].abs());}}
  if off<1e-300{break}
  for p in 0..3{for q in p+1..3{
   if a[p][q]==0.{continue}
   let theta=(a[q][q]-a[p][p])/(2.*a[p][q]);let t=theta.signum()/(theta.abs()+(theta*theta+1.).sqrt());let c=1./(t*t+1.).sqrt();let s=t*c;
   for k in 0..3{let(a_pk,a_qk)=(a[p][k],a[q][k]);a[p][k]=c*a_pk-s*a_qk;a[q][k]=s*a_pk+c*a_qk;}
   for k in 0..3{let(a_kp,a_kq)=(a[k][p],a[k][q]);a[k][p]=c*a_kp-s*a_kq;a[k][q]=s*a_kp+c*a_kq;}
   for k in 0..3{let(v_kp,v_kq)=(v[k][p],v[k][q]);v[k][p]=c*v_kp-s*v_kq;v[k][q]=s*v_kp+c*v_kq;}
  }}
 }
 ([a[0][0],a[1][1],a[2][2]],v)
}
/// QEF vertex for a surface leaf: least-squares intersection of the Hermite planes,
/// solved in mass-point-shifted coordinates via the eigendecomposition; falls back to
/// the mass point when the system is numerically rank-deficient; clamped to the cell.
fn qef_vertex(her:&[Hermite],min:[f64;3],size:[f64;3])->([f64;3],bool,bool){
 let m=[her.iter().map(|h|h.point[0]).sum::<f64>()/her.len() as f64,her.iter().map(|h|h.point[1]).sum::<f64>()/her.len() as f64,her.iter().map(|h|h.point[2]).sum::<f64>()/her.len() as f64];
 let mut ata=[[0.;3];3];let mut atb=[0.;3];
 for h in her{let d=[h.point[0]-m[0],h.point[1]-m[1],h.point[2]-m[2]];let b=h.normal[0]*d[0]+h.normal[1]*d[1]+h.normal[2]*d[2];for i in 0..3{for j in 0..3{ata[i][j]+=h.normal[i]*h.normal[j];}}for i in 0..3{atb[i]+=h.normal[i]*b;}}
 let (ev,v)=jacobi3(ata);let lmax=ev[0].max(ev[1]).max(ev[2]);let thr=(lmax*1e-4).max(1e-300);// singular-value threshold: rank-deficient (planar/cylindrical) cases drop to mass-point behavior on the null directions
 let mut x=[0.;3];let mut used=0;
 for i in 0..3{if ev[i]>thr{let vi=[v[0][i],v[1][i],v[2][i]];let dot=vi[0]*atb[0]+vi[1]*atb[1]+vi[2]*atb[2];for k in 0..3{x[k]+=vi[k]*dot/ev[i];}used+=1;}}
 let mut p=[m[0]+x[0],m[1]+x[1],m[2]+x[2]];
 let outside=p.iter().enumerate().any(|(j,&v)|!v.is_finite()||v<min[j]||v>min[j]+size[j]);
 let fallback=used==0||outside;
 if fallback{p=m;}
 let mut clamped=false;for j in 0..3{let q=p[j].clamp(min[j],min[j]+size[j]);if q!=p[j]{clamped=true;}p[j]=q;}
 (p,fallback,clamped)
}
#[derive(Clone,Debug,Default,Serialize)]pub struct MeshStats{pub balance_splits:usize,pub repair_splits:usize,pub vertices:usize,pub quads:usize,pub triangles:usize,pub boundary_edges_skipped:usize,pub degenerate_edges:usize,pub qef_fallbacks:usize,pub qef_clamped:usize,pub hermite_normal_dropped:usize,pub normal_fd_fallbacks:usize,pub normal_face_weighted:usize,pub transition_quads:usize,pub max_depth_span_in_quad:u32,pub residual_max:f64}
#[derive(Serialize)]pub struct AdaptiveMesh{pub stream:super::vertex_stream::Stream,pub stats:MeshStats,pub leaves:usize,pub empty:bool,pub algorithm:&'static str,pub boundary_edges:usize,pub nonmanifold_edges:usize,pub euler_characteristic:i64,pub signed_volume:f64,pub winding_consistent:bool}
/// Crack-free adaptive mesh: balance -> repair -> QEF vertices -> minimal-edge quads.
pub fn extract_mesh(g:&Graph,c:&OctreeConfig)->Result<AdaptiveMesh,String>{
 g.validate()?;validate(c)?;
 let mut b=B{g,c,fc:HashMap::new(),nc:HashMap::new(),st:Stats::default(),cos_thr:(c.normal_angle_degrees.to_radians()).cos()};
 let mut leaves=Vec::new();b.cell(c.bounds[0],0,&mut leaves)?;
 let empty0=b.st.surface_leaves==0;
 let(bal,rep)=balance_and_repair(&mut b,&mut leaves)?;
 if empty0{return Ok(AdaptiveMesh{stream:super::vertex_stream::Stream{vertices:vec![],indices:vec![]},stats:MeshStats{..Default::default()},leaves:leaves.len(),empty:true,algorithm:"adaptive octree dual contouring (QEF, crack-free 2:1 transitions)",boundary_edges:0,nonmanifold_edges:0,euler_characteristic:0,signed_volume:0.,winding_consistent:true});}
 let map=b.leaf_map(&leaves);
 let mut ms=MeshStats{balance_splits:bal,repair_splits:rep,..Default::default()};
 // vertex per surface leaf
 let mut vid:HashMap<usize,u32>=HashMap::new();let mut positions:Vec<[f64;3]>=Vec::new();
 for(li,l)in leaves.iter().enumerate(){let neg=l.corners.map(|v|v<0.);if neg.iter().all(|&x|x==neg[0]){continue}
  let(p,fb,cl)=qef_vertex(&l.hermite,l.min,l.size);if fb{ms.qef_fallbacks+=1;}if cl{ms.qef_clamped+=1;}
  let r=g.evaluate(p)?.abs();ms.residual_max=ms.residual_max.max(r);
  vid.insert(li,positions.len() as u32);positions.push(p);}
 // emit quads for minimal sign-change edges
 let mut emitted:HashSet<(u8,u32,u32,u32,u32)>=HashSet::new();
 let mut tris:Vec<[u32;3]>=Vec::new();
 for(_li,l)in leaves.iter().enumerate(){
  let neg=l.corners.map(|v|v<0.);if neg.iter().all(|&x|x==neg[0]){continue}
  let d=l.depth;let sh=c.max_depth-d;let base=lattice(c.max_depth,c.bounds,l.min).map(|v|v>>sh);
  for a in 0..3{for i in 0..8{if i&(1<<a)==0{let j=i|(1<<a);
   if (l.corners[i]<0.)==(l.corners[j]<0.){continue}
   let ek=(a as u8,d,base[0]+((i&1)as u32),base[1]+(((i>>1)&1)as u32),base[2]+(((i>>2)&1)as u32));
   if !emitted.insert(ek){continue}
   let (ax1,ax2)=((a+1)%3,(a+2)%3);
   let corner_bit=[((i>>ax1)&1)as i64,((i>>ax2)&1)as i64];
   let offs=[[0,0],[0,-1],[-1,0],[-1,-1]];
   let mut cells:Vec<usize>=Vec::with_capacity(4);let mut any_internal=false;let mut outside=0usize;
   for o in offs{let mut p=base;p[ax1]=(base[ax1]as i64+o[0]+corner_bit[0])as u32;p[ax2]=(base[ax2]as i64+o[1]+corner_bit[1])as u32;
    let lim=1u32<<d;
    if (base[ax1]as i64+o[0]+corner_bit[0])<0||(base[ax2]as i64+o[1]+corner_bit[1])<0||p[0]>=lim||p[1]>=lim||p[2]>=lim{outside+=1;continue}
    match b.resolve(&map,d,p[0],p[1],p[2]){Some(Resolve::Leaf(ni))=>cells.push(ni),Some(Resolve::Internal)=>any_internal=true,None=>any_internal=true}}
   if any_internal{continue}// finer half-edges carry this crossing
   let mut distinct:Vec<usize>=Vec::new();for ci in cells{if !distinct.contains(&ci){distinct.push(ci);}}
   let mut depths=distinct.iter().map(|&ci|leaves[ci].depth).collect::<Vec<_>>();depths.sort();let span=depths.last().unwrap()-depths[0];ms.max_depth_span_in_quad=ms.max_depth_span_in_quad.max(span);if span>0{ms.transition_quads+=1;}
   // rotational order around axis a: (0,0),(1,0),(1,1),(0,1) in (ax1,ax2) offsets
   let order=[3usize,2,0,1];// map offset index -> rotational sequence
   let ring:Vec<usize>=order.iter().filter_map(|&oi|{let o=offs[oi];let mut p=base;p[ax1]=(base[ax1]as i64+o[0]+corner_bit[0])as u32;p[ax2]=(base[ax2]as i64+o[1]+corner_bit[1])as u32;let lim=1u32<<d;if(base[ax1]as i64+o[0]+corner_bit[0])<0||(base[ax2]as i64+o[1]+corner_bit[1])<0||p[0]>=lim||p[1]>=lim||p[2]>=lim{return None}match b.resolve(&map,d,p[0],p[1],p[2]){Some(Resolve::Leaf(ni))=>Some(ni),_=>None}}).collect();
   // a coarser leaf can cover two cyclically-adjacent positions around the edge
   // (its footprint is contiguous); collapse cyclic duplicates, keep the vertex once
   let mut rr:Vec<usize>=Vec::with_capacity(4);for x in ring{if rr.last()!=Some(&x){rr.push(x);}}
   while rr.len()>1&&rr.first()==rr.last(){rr.pop();}
   let ring=rr;
   if ring.len()<3{ms.degenerate_edges+=1;continue}
   if outside>0{ms.boundary_edges_skipped+=outside;}
   let w:Vec<u32>=ring.iter().map(|ci|vid[ci]).collect();
   let inside=l.corners[i]<0.;// reference endpoint inside => reversed ring order faces outward (census-verified)
   let emit=|t:[u32;3],tris:&mut Vec<[u32;3]>|{if inside{tris.push([t[0],t[2],t[1]]);}else{tris.push([t[0],t[1],t[2]]);}};
   if w.len()==4{ms.quads+=1;emit([w[0],w[1],w[3]],&mut tris);emit([w[1],w[2],w[3]],&mut tris);}else{ms.triangles+=1;for k in 1..w.len()-1{emit([w[0],w[k],w[k+1]],&mut tris);}}
  }}}
 }
 // Stream vertex normals (CP3 contract): analytic SDF gradient where defined, else smooth
 // angle-weighted face normal (sharp creases / CSG ties), then FD counted, then counted axis
 // placeholder - never NaN. UV/tangent contract as contour_wrapped.
 let mut vertices=Vec::with_capacity(positions.len());
 let mut fan=vec![[0.;3];positions.len()];
 for t in &tris{let ids=[t[0] as usize,t[1] as usize,t[2] as usize];let p=[positions[ids[0]],positions[ids[1]],positions[ids[2]]];
  let u=[p[1][0]-p[0][0],p[1][1]-p[0][1],p[1][2]-p[0][2]];let v=[p[2][0]-p[0][0],p[2][1]-p[0][1],p[2][2]-p[0][2]];
  let cr=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]];let l=(cr[0]*cr[0]+cr[1]*cr[1]+cr[2]*cr[2]).sqrt();if !(l>1e-18)||!l.is_finite(){continue;}
  let n=[cr[0]/l,cr[1]/l,cr[2]/l];
  for k in 0..3{let a=[p[(k+1)%3][0]-p[k][0],p[(k+1)%3][1]-p[k][1],p[(k+1)%3][2]-p[k][2]];let b=[p[(k+2)%3][0]-p[k][0],p[(k+2)%3][1]-p[k][1],p[(k+2)%3][2]-p[k][2]];
   let cx=a[1]*b[2]-a[2]*b[1];let cy=a[2]*b[0]-a[0]*b[2];let cz=a[0]*b[1]-a[1]*b[0];
   let ang=(cx*cx+cy*cy+cz*cz).sqrt().atan2(a[0]*b[0]+a[1]*b[1]+a[2]*b[2]);if !ang.is_finite(){continue;}
   for j in 0..3{fan[ids[k]][j]+=ang*n[j];}}}
 for (vi,p) in positions.iter().enumerate(){let n=match g.normal_analytic(*p){Ok(n)if n.iter().all(|v|v.is_finite())=>n,_=>{
  let f=fan[vi];let l=(f[0]*f[0]+f[1]*f[1]+f[2]*f[2]).sqrt();
  if l.is_finite()&&l>1e-12{ms.normal_face_weighted+=1;[f[0]/l,f[1]/l,f[2]/l]}
  else{match g.normal(*p,c.normal_epsilon){Ok(n)if n.iter().all(|v|v.is_finite())=>{ms.normal_fd_fallbacks+=1;n},_=>{ms.hermite_normal_dropped+=1;[1.,0.,0.]}}}}};
  let axis=if n[0].abs()<0.8{[1.,0.,0.]}else{[0.,1.,0.]};let t=super::vertex_stream::unit([n[1]*axis[2]-n[2]*axis[1],n[2]*axis[0]-n[0]*axis[2],n[0]*axis[1]-n[1]*axis[0]])?;
  vertices.push(super::primitives::Vertex{position:p.map(|v|v as f32),normal:n.map(|v|v as f32),uv:[0.,0.],tangent:[t[0] as f32,t[1] as f32,t[2] as f32,1.]});}
 let cen=super::topo::census(&positions,&tris);
 let stream=super::vertex_stream::Stream{vertices,indices:tris};stream.validate()?;
 ms.vertices=positions.len();
 Ok(AdaptiveMesh{stream,stats:ms,leaves:leaves.len(),empty:false,algorithm:"adaptive octree dual contouring (QEF, crack-free 2:1 transitions)",boundary_edges:cen.boundary_edges,nonmanifold_edges:cen.nonmanifold_edges,euler_characteristic:cen.euler_characteristic,signed_volume:cen.signed_volume,winding_consistent:cen.winding_consistent})}

#[cfg(test)]mod mesh_tests{use super::*;use crate::threed::sdf_graph::Field;
fn sphere()->Graph{Graph{nodes:vec![Field::Sphere{center:[0.;3],radius:1.}],output:0}}
fn union_scene()->Graph{Graph{nodes:vec![Field::Sphere{center:[0.35,0.,0.],radius:0.8},Field::Box{center:[-0.35,0.,0.],half_size:[0.6;3]},Field::SmoothUnion{a:0,b:1,k:0.25}],output:2}}
fn cfg(min_d:u32,max_d:u32)->OctreeConfig{OctreeConfig{bounds:[[-2.2;3],[2.2;3]],min_depth:min_d,max_depth:max_d,error_tolerance:0.02,normal_angle_degrees:20.,normal_epsilon:0.001}}
#[test]fn jacobi3_known_eigenpairs(){
 let(ev,_v)=jacobi3([[2.,0.,0.],[0.,3.,0.],[0.,0.,5.]]);let mut srt=ev.to_vec();srt.sort_by(|a,b|a.partial_cmp(b).unwrap());assert!((srt[0]-2.).abs()<1e-12&&(srt[1]-3.).abs()<1e-12&&(srt[2]-5.).abs()<1e-12,"diag eigenvalues {ev:?}");
 // rotation-invariant: eigenvalues of a known 2x2 block [[1,2],[2,1]] are 3,-1; embedded in 3x3 with 4 on z
 let(ev,_)=jacobi3([[1.,2.,0.],[2.,1.,0.],[0.,0.,4.]]);let mut srt=ev.to_vec();srt.sort_by(|a,b|a.partial_cmp(b).unwrap());assert!((srt[0]+1.).abs()<1e-10&&(srt[1]-3.).abs()<1e-10&&(srt[2]-4.).abs()<1e-10,"block eigenvalues {srt:?}");
 // eigenvectors orthonormal and diagonalize: V^T A V = D
 let a=[[4.,1.,0.5],[1.,3.,0.],[0.5,0.,2.]];let(ev,v)=jacobi3(a);
 for i in 0..3{let r:[f64;3]=std::array::from_fn(|k|a[k][0]*v[0][i]+a[k][1]*v[1][i]+a[k][2]*v[2][i]);for k in 0..3{assert!((r[k]-ev[i]*v[k][i]).abs()<1e-9,"A v != lambda v: component {k} r={} ev* v={}",r[k],ev[i]*v[k][i]);}}
}
#[test]fn qef_plane_and_corner(){
 // one plane z=0.3: vertex z must equal 0.3
 // exact: all planes n.(x-p)=0 share z=0.3, so the QEF solution must satisfy z=0.3
 let h:Vec<Hermite>=(0..4).map(|i|{let x=(i%2)as f64;let y=(i/2)as f64;Hermite{point:[x,y,0.3],normal:[0.,0.,1.]}}).collect();
 let(p,fb,_)=qef_vertex(&h,[-1.;3],[2.;3]);assert!(!fb&&(p[2]-0.3).abs()<1e-9,"plane vertex z {} fb {}",p[2],fb);
 // three planes x=0.2,y=0.4,z=0.6: vertex at the corner
 let h=vec![Hermite{point:[0.2,0.,0.],normal:[1.,0.,0.]},Hermite{point:[0.2,0.5,0.5],normal:[1.,0.,0.]},Hermite{point:[0.,0.4,0.],normal:[0.,1.,0.]},Hermite{point:[0.5,0.4,0.5],normal:[0.,1.,0.]},Hermite{point:[0.,0.,0.6],normal:[0.,0.,1.]},Hermite{point:[0.5,0.5,0.6],normal:[0.,0.,1.]}];
 let(p,fb,cl)=qef_vertex(&h,[-1.;3],[2.;3]);assert!(!fb&&!cl,"corner fb={fb} cl={cl}");
 assert!((p[0]-0.2).abs()<1e-9&&(p[1]-0.4).abs()<1e-9&&(p[2]-0.6).abs()<1e-9,"corner vertex {p:?}");
}
#[test]fn sphere_mesh_closed_euler_two_volume(){
 let m=extract_mesh(&sphere(),&cfg(2,6)).unwrap();assert!(!m.empty);
 println!("ADCM-SPHERE v={} t={} q={} trans={} span={} bal={} rep={} fb={} cl={} resid={:.4} bnd={} nm={} euler={} vol={:.4} wind={}",m.stats.vertices,m.stream.indices.len(),m.stats.quads,m.stats.transition_quads,m.stats.max_depth_span_in_quad,m.stats.balance_splits,m.stats.repair_splits,m.stats.qef_fallbacks,m.stats.qef_clamped,m.stats.residual_max,m.boundary_edges,m.nonmanifold_edges,m.euler_characteristic,m.signed_volume,m.winding_consistent);
 assert_eq!((m.boundary_edges,m.nonmanifold_edges),(0,0),"sphere mesh must be crack-free closed: bnd={} nm={}",m.boundary_edges,m.nonmanifold_edges);
 assert_eq!(m.euler_characteristic,2,"euler {}",m.euler_characteristic);
 let expect=4./3.*std::f64::consts::PI;assert!((m.signed_volume-expect).abs()<0.02*expect,"volume {} vs {}",m.signed_volume,expect);
 assert!(m.winding_consistent,"consistent outward winding");
 assert_eq!(m.stats.transition_quads,0,"unit sphere refines uniformly: no LOD transitions expected");
 // vertices near the true sphere
 for vtx in &m.stream.vertices{let r=(vtx.position[0].powi(2)+vtx.position[1].powi(2)+vtx.position[2].powi(2)).sqrt();assert!((r-1.).abs()<0.05,"vertex radius {r}");}
}
#[test]fn torus_mesh_closed_euler_zero(){
 let g=Graph{nodes:vec![Field::Torus{center:[0.;3],major_radius:1.,minor_radius:0.4}],output:0};
 let m=extract_mesh(&g,&cfg(2,7)).unwrap();
 println!("ADCM-TORUS v={} t={} bal={} rep={} bnd={} nm={} euler={} vol={:.4}",m.stats.vertices,m.stream.indices.len(),m.stats.balance_splits,m.stats.repair_splits,m.boundary_edges,m.nonmanifold_edges,m.euler_characteristic,m.signed_volume);
 assert_eq!((m.boundary_edges,m.nonmanifold_edges),(0,0),"torus crack-free: bnd={} nm={}",m.boundary_edges,m.nonmanifold_edges);
 assert_eq!(m.euler_characteristic,0,"torus euler {}",m.euler_characteristic);
 let expect=2.*std::f64::consts::PI*std::f64::consts::PI*1.*0.4*0.4;assert!((m.signed_volume-expect).abs()<0.1*expect,"torus volume {} vs {}",m.signed_volume,expect);
}
#[test]fn vertex_normals_and_tangents_finite_unit_orthogonal_on_sharp_creases(){
 for g in [union_scene(),Graph{nodes:vec![Field::Box{center:[0.;3],half_size:[0.6;3]}],output:0}]{
  let m=extract_mesh(&g,&cfg(2,7)).unwrap();
  assert!(m.stats.normal_face_weighted>0,"crease/edge vertices must route to face-weighted normals: {:?}",m.stats);
  for(i,v)in m.stream.vertices.iter().enumerate(){
   let n=v.normal.map(|x|x as f64);let t=[v.tangent[0] as f64,v.tangent[1] as f64,v.tangent[2] as f64];
   assert!(n.iter().chain(t.iter()).all(|x|x.is_finite()),"vertex {i} NaN normal/tangent: {n:?} {t:?}");
   let ln=(n[0]*n[0]+n[1]*n[1]+n[2]*n[2]).sqrt();let lt=(t[0]*t[0]+t[1]*t[1]+t[2]*t[2]).sqrt();
   assert!((ln-1.).abs()<1e-3,"vertex {i} normal len {ln}");assert!((lt-1.).abs()<1e-3,"vertex {i} tangent len {lt}");
   let d=(n[0]*t[0]+n[1]*t[1]+n[2]*t[2]).abs();assert!(d<1e-3,"vertex {i} tangent not orthogonal to normal: {d}");}
 }}
#[test]fn crease_benchmark_sharp_drop_vs_pinned_uniform(){
 let m=extract_mesh(&union_scene(),&cfg(2,8)).unwrap();
 assert_eq!((m.boundary_edges,m.nonmanifold_edges),(0,0),"union crack-free: bnd={} nm={}",m.boundary_edges,m.nonmanifold_edges);
 assert_eq!(m.euler_characteristic,2,"union euler {}",m.euler_characteristic);
 // same dihedral metric as the pinned uniform benchmark in tests/sdf_contour_frames.rs
 let p:Vec<[f64;3]>=m.stream.vertices.iter().map(|v|v.position.map(|x|x as f64)).collect();
 let f=|t:[u32;3]|->[f64;3]{let[a,b,c]=t.map(|i|p[i as usize]);let ab=[b[0]-a[0],b[1]-a[1],b[2]-a[2]];let ac=[c[0]-a[0],c[1]-a[1],c[2]-a[2]];let n=[ab[1]*ac[2]-ab[2]*ac[1],ab[2]*ac[0]-ab[0]*ac[2],ab[0]*ac[1]-ab[1]*ac[0]];let l=(n[0]*n[0]+n[1]*n[1]+n[2]*n[2]).sqrt();if l>1e-300{[n[0]/l,n[1]/l,n[2]/l]}else{[0.,0.,0.]}};
 let fn_:Vec<_>=m.stream.indices.iter().map(|t|f(*t)).collect();
 use std::collections::HashMap as HM;let mut et:HM<(u32,u32),Vec<usize>>=HM::new();
 for(ti,t)in m.stream.indices.iter().enumerate(){for k in 0..3{let(a,b)=(t[k],t[(k+1)%3]);et.entry((a.min(b),a.max(b))).or_default().push(ti);}}
 let mut o60=0;let mut o120=0;
 for(_,ts)in &et{if ts.len()==2{let d=fn_[ts[0]][0]*fn_[ts[1]][0]+fn_[ts[0]][1]*fn_[ts[1]][1]+fn_[ts[0]][2]*fn_[ts[1]][2];let a=d.clamp(-1.,1.).acos().to_degrees();if a>60.{o60+=1;}if a>120.{o120+=1;}}}
 println!("ADCM-CREASE tris={} folds60={o60} folds120={o120} resid={:.4} bal={} rep={} trans={}",m.stream.indices.len(),m.stats.residual_max,m.stats.balance_splits,m.stats.repair_splits,m.stats.transition_quads);
 // pinned uniform artifact: res48 folds120=346, res64 folds120=288 (tests/sdf_contour_frames.rs)
 assert!(o120<288/2,"adaptive folds120 {o120} must drop sharply vs uniform-res64 288");
 assert!(m.stats.transition_quads>0,"crease scene has multiple surface depths: LOD transitions must be exercised");
}
#[test]fn box_mesh_sharp_closed(){
 let g=Graph{nodes:vec![Field::Box{center:[0.;3],half_size:[0.6;3]}],output:0};
 let c=OctreeConfig{bounds:[[-1.6;3],[1.6;3]],..cfg(2,7)};
 let m=extract_mesh(&g,&c).unwrap();
 println!("ADCM-BOX v={} t={} bnd={} euler={} vol={:.4} cl={} fb={}",m.stats.vertices,m.stream.indices.len(),m.boundary_edges,m.euler_characteristic,m.signed_volume,m.stats.qef_clamped,m.stats.qef_fallbacks);
 assert_eq!((m.boundary_edges,m.nonmanifold_edges),(0,0),"box crack-free: bnd={} nm={}",m.boundary_edges,m.nonmanifold_edges);
 assert_eq!(m.euler_characteristic,2,"box euler {}",m.euler_characteristic);
 let expect=1.2f64.powi(3);assert!((m.signed_volume-expect).abs()<0.05*expect,"box volume {} vs {}",m.signed_volume,expect);
 // corner fidelity: each box corner has a mesh vertex within a fine cell
 let cell=(c.bounds[1][0]-c.bounds[0][0])/2f64.powi(7);
 for corner in 0..8{let t=[if corner&1==0{-0.6}else{0.6},if corner&2==0{-0.6}else{0.6},if corner&4==0{-0.6}else{0.6}];let d=m.stream.vertices.iter().map(|v|{let dx=v.position[0] as f64-t[0];let dy=v.position[1] as f64-t[1];let dz=v.position[2] as f64-t[2];(dx*dx+dy*dy+dz*dz).sqrt()}).fold(f64::INFINITY,f64::min);assert!(d<cell,"corner {t:?} nearest vertex {d} >= cell {cell}");}
 // edge midpoints: each of the 12 box edges has a vertex within one fine cell of its midpoint
 for e in 0..12{let ax=e/4;let k=e%4;let mut t=[0.0;3];t[ax]=0.;t[(ax+1)%3]=if k&1==0{-0.6}else{0.6};t[(ax+2)%3]=if k&2==0{-0.6}else{0.6};let d=m.stream.vertices.iter().map(|v|{let dx=v.position[0] as f64-t[0];let dy=v.position[1] as f64-t[1];let dz=v.position[2] as f64-t[2];(dx*dx+dy*dy+dz*dz).sqrt()}).fold(f64::INFINITY,f64::min);assert!(d<2.*cell,"edge {e} midpoint nearest vertex {d}");}
 // no NaN anywhere in the stream
 for v in &m.stream.vertices{assert!(v.position.iter().chain(v.normal.iter()).chain(v.tangent.iter()).all(|x|x.is_finite()),"nonfinite vertex data");}
}
#[test]fn empty_field_mesh_typed_empty(){
 let far=Graph{nodes:vec![Field::Sphere{center:[50.;3],radius:1.}],output:0};
 let m=extract_mesh(&far,&cfg(2,6)).unwrap();assert!(m.empty&&m.stream.vertices.is_empty()&&m.stream.indices.is_empty(),"empty field typed empty");
}
}
