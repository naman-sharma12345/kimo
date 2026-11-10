//! Original bounded implicit graph, min/max and polynomial smooth CSG.
use serde::{Serialize,Deserialize};
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]pub enum Field{Sphere{center:[f64;3],radius:f64},Box{center:[f64;3],half_size:[f64;3]},Plane{normal:[f64;3],offset:f64},Cylinder{center:[f64;3],radius:f64,half_height:f64},Capsule{a:[f64;3],b:[f64;3],radius:f64},Torus{center:[f64;3],major_radius:f64,minor_radius:f64},Union{a:usize,b:usize},Intersect{a:usize,b:usize},Subtract{a:usize,b:usize},SmoothUnion{a:usize,b:usize,k:f64},SmoothIntersect{a:usize,b:usize,k:f64},SmoothSubtract{a:usize,b:usize,k:f64}}
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]pub struct Graph{pub nodes:Vec<Field>,pub output:usize}
fn finite(x:f64)->bool{x.is_finite()&&x.abs()<=1e6}
impl Graph{
 pub fn validate(&self)->Result<(),String>{if self.nodes.is_empty()||self.nodes.len()>128||self.output>=self.nodes.len(){return Err("SDF graph1..128 nodes and valid output required".into());}for(i,n)in self.nodes.iter().enumerate(){match n{Field::Sphere{center,radius}=>{if center.iter().any(|v|!finite(*v))||!finite(*radius)||*radius<=0.{return Err("finite sphere positive radius".into());}},Field::Box{center,half_size}=>{if center.iter().any(|v|!finite(*v))||half_size.iter().any(|v|!finite(*v)||*v<=0.){return Err("finite box positive half sizes".into());}},Field::Plane{normal,offset}=>{if !finite(*offset)||normal.iter().any(|v|!finite(*v)){return Err("bounded plane".into());}super::vertex_stream::unit(*normal)?;},Field::Cylinder{center,radius,half_height}=>{if center.iter().any(|v|!finite(*v))||!finite(*radius)||*radius<=0.||!finite(*half_height)||*half_height<=0.{return Err("finite cylinder positive radius/half_height".into());}},Field::Capsule{a,b,radius}=>{if a.iter().chain(b.iter()).any(|v|!finite(*v))||!finite(*radius)||*radius<=0.{return Err("finite capsule positive radius".into());}let d:f64=a.iter().zip(b.iter()).map(|(x,y)|(x-y)*(x-y)).sum();if d<=1e-18{return Err("capsule segment endpoints must differ".into());}},Field::Torus{center,major_radius,minor_radius}=>{if center.iter().any(|v|!finite(*v))||!finite(*major_radius)||*major_radius<=0.||!finite(*minor_radius)||*minor_radius<=0.{return Err("finite torus positive major/minor radius".into());}},Field::Union{a,b}|Field::Intersect{a,b}|Field::Subtract{a,b}=>{if *a>=i||*b>=i{return Err("CSG inputs must reference earlier nodes".into());}},Field::SmoothUnion{a,b,k}|Field::SmoothIntersect{a,b,k}|Field::SmoothSubtract{a,b,k}=>{if *a>=i||*b>=i||!finite(*k)||*k<=0.{return Err("smooth CSG earlier inputs and k>0 required".into());}}}}Ok(())}
 pub fn sample(&self,p:[f64;3])->Result<f64,String>{self.validate()?;self.evaluate(p)}
 pub(crate) fn evaluate(&self,p:[f64;3])->Result<f64,String>{if p.iter().any(|v|!finite(*v)){return Err("SDF point abs<=1e6 finite required".into());}let mut v=Vec::with_capacity(self.nodes.len());let min_smooth=|a:f64,b:f64,k:f64|{let h=(0.5+0.5*(b-a)/k).clamp(0.,1.);b*(1.-h)+a*h-k*h*(1.-h)};for n in &self.nodes{let d=match n{Field::Sphere{center,radius}=>(p[0]-center[0]).hypot(p[1]-center[1]).hypot(p[2]-center[2])-radius,Field::Box{center,half_size}=>{let q:[f64;3]=std::array::from_fn(|j|(p[j]-center[j]).abs()-half_size[j]);q[0].max(0.).hypot(q[1].max(0.)).hypot(q[2].max(0.))+q[0].max(q[1]).max(q[2]).min(0.)},Field::Plane{normal,offset}=>{let n=super::vertex_stream::unit(*normal)?;n.iter().zip(p).map(|(a,b)|a*b).sum::<f64>()-offset},Field::Cylinder{center,radius,half_height}=>{let dx=(p[0]-center[0]).hypot(p[2]-center[2])-radius;let dy=(p[1]-center[1]).abs()-half_height;dx.max(0.).hypot(dy.max(0.))+dx.max(dy).min(0.)},Field::Capsule{a,b,radius}=>{let ab=[b[0]-a[0],b[1]-a[1],b[2]-a[2]];let ap=[p[0]-a[0],p[1]-a[1],p[2]-a[2]];let len2=ab[0]*ab[0]+ab[1]*ab[1]+ab[2]*ab[2];let t=((ap[0]*ab[0]+ap[1]*ab[1]+ap[2]*ab[2])/len2).clamp(0.,1.);(ap[0]-t*ab[0]).hypot(ap[1]-t*ab[1]).hypot(ap[2]-t*ab[2])-radius},Field::Torus{center,major_radius,minor_radius}=>{let q=(p[0]-center[0]).hypot(p[2]-center[2])-major_radius;q.hypot(p[1]-center[1])-minor_radius},Field::Union{a,b}=>f64::min(v[*a],v[*b]),Field::Intersect{a,b}=>f64::max(v[*a],v[*b]),Field::Subtract{a,b}=>f64::max(v[*a],-v[*b]),Field::SmoothUnion{a,b,k}=>min_smooth(v[*a],v[*b],*k),Field::SmoothIntersect{a,b,k}=>-min_smooth(-v[*a],-v[*b],*k),Field::SmoothSubtract{a,b,k}=>-min_smooth(-v[*a],v[*b],*k)};v.push(d);}Ok(v[self.output])}
 /// Analytic (closed-form) gradient normal of the composed field: exact per-primitive
 /// gradients, CSG branch selection, smooth-blend chain rule. Typed error on the
 /// non-differentiable set (CSG operand ties, box face/edge boundaries, cylinder rim,
 /// capsule spine, torus axis ring, zero gradient) - never a guessed normal.
 pub fn normal_analytic(&self,p:[f64;3])->Result<[f64;3],String>{
 self.validate()?;if p.iter().any(|v|!finite(*v)){return Err("SDF point abs<=1e6 finite required".into());}
 let smin=|va:f64,ga:[f64;3],vb:f64,gb:[f64;3],k:f64|->(f64,[f64;3]){let h=(0.5+0.5*(vb-va)/k).clamp(0.,1.);let d=vb*(1.-h)+va*h-k*h*(1.-h);let g=if h==0.{gb}else if h==1.{ga}else{let c=(-(vb-va)-k*(1.-2.*h))*(0.5/k);std::array::from_fn(|j|(h-c)*ga[j]+(1.-h+c)*gb[j])};(d,g)};
 let mut vals:Vec<f64>=Vec::with_capacity(self.nodes.len());let mut grads:Vec<[f64;3]>=Vec::with_capacity(self.nodes.len());
 for n in &self.nodes{
  let(d,g):(f64,[f64;3])=match n{
  Field::Sphere{center,radius}=>{let q=[p[0]-center[0],p[1]-center[1],p[2]-center[2]];let r=(q[0]*q[0]+q[1]*q[1]+q[2]*q[2]).sqrt();if r==0.{return Err("sphere center: zero gradient".into());}(r-radius,[q[0]/r,q[1]/r,q[2]/r])}
  Field::Box{center,half_size}=>{let q:[f64;3]=std::array::from_fn(|j|(p[j]-center[j]).abs()-half_size[j]);let sgn:[f64;3]=std::array::from_fn(|j|if p[j]>=center[j]{1.}else{-1.});
   if q.iter().any(|&v|v==0.){return Err("box face/edge boundary: gradient discontinuous".into());}
   if q.iter().all(|&v|v<0.){let mx=q.iter().cloned().fold(f64::NEG_INFINITY,f64::max);let arg:Vec<usize>=(0..3).filter(|&j|q[j]==mx).collect();if arg.len()!=1{return Err("box edge/corner: gradient discontinuous".into());}(mx,std::array::from_fn(|j|if j==arg[0]{sgn[j]}else{0.}))}
   else{let d=q.iter().filter(|&&v|v>0.).map(|v|v*v).sum::<f64>().sqrt();(d,std::array::from_fn(|j|if q[j]>0.{sgn[j]*q[j]/d}else{0.}))}}
  Field::Plane{normal,offset}=>{let n=super::vertex_stream::unit(*normal)?;(n.iter().zip(p).map(|(a,b)|a*b).sum::<f64>()-offset,n)}
  Field::Cylinder{center,radius,half_height}=>{let w=(p[0]-center[0]).hypot(p[2]-center[2]);let dx=w-radius;let dy=(p[1]-center[1]).abs()-half_height;let sy=if p[1]>=center[1]{1.}else{-1.};let ux=if w>0.{(p[0]-center[0])/w}else{0.};let uz=if w>0.{(p[2]-center[2])/w}else{0.};
   if dx>0.&&dy>0.{let l=dx.hypot(dy);(l,[ux*dx/l,sy*dy/l,uz*dx/l])}
   else if dx>dy{if w==0.{return Err("cylinder axis: gradient discontinuous".into());}(dx,[ux,0.,uz])}
   else if dy>dx{(dy,[0.,sy,0.])}
   else{return Err("cylinder rim: gradient discontinuous".into());}}
  Field::Capsule{a,b,radius}=>{let ab=[b[0]-a[0],b[1]-a[1],b[2]-a[2]];let ap=[p[0]-a[0],p[1]-a[1],p[2]-a[2]];let len2=ab[0]*ab[0]+ab[1]*ab[1]+ab[2]*ab[2];let t=((ap[0]*ab[0]+ap[1]*ab[1]+ap[2]*ab[2])/len2).clamp(0.,1.);let q=[ap[0]-t*ab[0],ap[1]-t*ab[1],ap[2]-t*ab[2]];let l=(q[0]*q[0]+q[1]*q[1]+q[2]*q[2]).sqrt();if l==0.{return Err("capsule spine: zero gradient".into());}(l-radius,[q[0]/l,q[1]/l,q[2]/l])}
  Field::Torus{center,major_radius,minor_radius}=>{let w=(p[0]-center[0]).hypot(p[2]-center[2]);if w==0.{return Err("torus axis ring: gradient discontinuous".into());}let q0=w-major_radius;let q1=p[1]-center[1];let l=q0.hypot(q1);if l==0.{return Err("torus tube center circle: zero gradient".into());}(l-minor_radius,[(p[0]-center[0])/w*q0/l,q1/l,(p[2]-center[2])/w*q0/l])}
  Field::Union{a,b}|Field::Intersect{a,b}|Field::Subtract{a,b}=>{let(a,b)=(*a,*b);let(va,vb)=(vals[a],if matches!(n,Field::Subtract{..}){-vals[b]}else{vals[b]});let gb=if matches!(n,Field::Subtract{..}){grads[b].map(|v|-v)}else{grads[b]};if va==vb{return Err("CSG crease: operand value tie, gradient discontinuous".into());}let take_a=if matches!(n,Field::Union{..}){va<vb}else{va>vb};if take_a{(va,grads[a])}else{(vb,gb)}}
  Field::SmoothUnion{a,b,k}=>{let(a,b,k)=(*a,*b,*k);smin(vals[a],grads[a],vals[b],grads[b],k)}
  Field::SmoothIntersect{a,b,k}=>{let(a,b,k)=(*a,*b,*k);let(dm,gm)=smin(-vals[a],grads[a].map(|v|-v),-vals[b],grads[b].map(|v|-v),k);(-dm,gm.map(|v|-v))}
  Field::SmoothSubtract{a,b,k}=>{let(a,b,k)=(*a,*b,*k);let(dm,gm)=smin(-vals[a],grads[a].map(|v|-v),vals[b],grads[b],k);(-dm,gm.map(|v|-v))}
  };
  vals.push(d);grads.push(g);
 }
 super::vertex_stream::unit(grads[self.output])}
 #[cfg(feature="surface-contour")]pub(crate) fn normal(&self,p:[f64;3],eps:f64)->Result<[f64;3],String>{let mut n=[0.;3];for j in 0..3{let mut a=p;let mut b=p;a[j]+=eps;b[j]-=eps;n[j]=self.evaluate(a)?-self.evaluate(b)?;}super::vertex_stream::unit(n)}
}
#[cfg(test)]mod tests{use super::*;#[test]fn analytic_and_smooth_signs(){let mut g=Graph{nodes:vec![Field::Sphere{center:[0.;3],radius:1.},Field::Sphere{center:[1.,0.,0.],radius:1.},Field::SmoothUnion{a:0,b:1,k:0.2}],output:2};assert!(g.sample([0.5,0.,0.]).unwrap()<0.);g.nodes[2]=Field::SmoothIntersect{a:0,b:1,k:0.2};assert!(g.sample([0.5,0.,0.]).unwrap()<0.);g.nodes[2]=Field::SmoothSubtract{a:0,b:1,k:0.2};assert!(g.sample([0.9,0.,0.]).unwrap()>0.);g.nodes[2]=Field::Union{a:2,b:1};assert!(g.validate().is_err());}
 #[test]fn cylinder_capsule_torus_exact_distances_and_signs(){
 let eps=1e-9;
 let cyl=Graph{nodes:vec![Field::Cylinder{center:[0.;3],radius:1.,half_height:2.}],output:0};
 for(p,want)in [([0.;3],-1.),([1.5,0.,0.],0.5),([0.,3.,0.],1.),([1.,2.,0.],0.),([1.5,3.,0.],1.118033988749895),([0.,0.,5.],4.)]{let d=cyl.sample(p).unwrap();assert!((d-want).abs()<eps,"cylinder {p:?}: {d} vs {want}");}
 assert!(cyl.sample([0.5,0.,0.]).unwrap()<0.&&cyl.sample([0.,0.,1.5]).unwrap()>0.,"cylinder axis-aligned (Y) convention");
 let cap=Graph{nodes:vec![Field::Capsule{a:[0.,-1.,0.],b:[0.,1.,0.],radius:0.5}],output:0};
 for(p,want)in [([0.;3],-0.5),([0.,1.75,0.],0.25),([0.5,0.2,0.],0.),([0.,-1.5,0.],0.),([0.5,-1.2,0.],0.03851648071345037)]{let d=cap.sample(p).unwrap();assert!((d-want).abs()<eps,"capsule {p:?}: {d} vs {want}");}
 let tor=Graph{nodes:vec![Field::Torus{center:[0.;3],major_radius:2.,minor_radius:0.5}],output:0};
 for(p,want)in [([2.,0.,0.],-0.5),([2.5,0.,0.],0.),([0.;3],1.5),([2.,0.5,0.],0.),([1.5,0.,0.],0.)]{let d=tor.sample(p).unwrap();assert!((d-want).abs()<eps,"torus {p:?}: {d} vs {want}");}
 assert!(tor.sample([0.,0.,2.]).unwrap()<0.,"torus ring in XZ plane (Y axis) convention");
 }
 #[test]fn analytic_normals_exact_and_typed_errors(){
 let one=|n:Field|Graph{nodes:vec![n],output:0};
 let sph=one(Field::Sphere{center:[0.;3],radius:1.});
 assert!(sph.normal_analytic([0.,0.,1.]).unwrap().iter().zip([0.,0.,1.]).all(|(a,b)|(a-b).abs()<1e-12));
 assert!(sph.normal_analytic([0.;3]).is_err(),"sphere center must be a typed error");
 let bx=one(Field::Box{center:[0.;3],half_size:[1.,2.,3.]});
 assert!(bx.normal_analytic([1.5,0.3,0.2]).unwrap()==[1.,0.,0.]);
 assert!(bx.normal_analytic([0.3,0.2,0.5]).unwrap()==[1.,0.,0.],"inside face region picks unique closest axis");
 assert!(bx.normal_analytic([1.,0.,0.]).is_err(),"face boundary is a typed error");
 assert!(bx.normal_analytic([0.5,1.5,0.]).is_err(),"interior edge line tie is a typed error");
 let cyl=one(Field::Cylinder{center:[0.;3],radius:1.,half_height:2.});
 assert!(cyl.normal_analytic([1.5,0.,0.]).unwrap()==[1.,0.,0.]);
 assert!(cyl.normal_analytic([0.,3.,0.]).unwrap()==[0.,1.,0.]);
 let g=cyl.normal_analytic([1.5,2.5,0.]).unwrap();assert!((g[0]-std::f64::consts::FRAC_1_SQRT_2).abs()<1e-12&&(g[1]-std::f64::consts::FRAC_1_SQRT_2).abs()<1e-12,"exterior corner blend {g:?}");
 assert!(cyl.normal_analytic([1.,2.,0.]).is_err(),"rim is a typed error");
 assert!(cyl.normal_analytic([0.;3]).is_err(),"axis with radial dominance is a typed error");
 let cap=one(Field::Capsule{a:[0.,-1.,0.],b:[0.,1.,0.],radius:0.5});
 assert!(cap.normal_analytic([1.,0.,0.]).unwrap()==[1.,0.,0.]);
 assert!(cap.normal_analytic([0.,1.6,0.]).unwrap()==[0.,1.,0.]);
 assert!(cap.normal_analytic([0.,0.3,0.]).is_err(),"spine is a typed error");
 let tor=one(Field::Torus{center:[0.;3],major_radius:2.,minor_radius:0.5});
 assert!(tor.normal_analytic([2.5,0.,0.]).unwrap()==[1.,0.,0.]);
 assert!(tor.normal_analytic([2.,0.5,0.]).unwrap()==[0.,1.,0.]);
 assert!(tor.normal_analytic([0.;3]).is_err(),"axis ring is a typed error");
 let tie=Graph{nodes:vec![Field::Sphere{center:[-1.,0.,0.],radius:1.},Field::Sphere{center:[1.,0.,0.],radius:1.},Field::Union{a:0,b:1}],output:2};
 assert!(tie.normal_analytic([0.,0.,0.9]).is_err(),"CSG crease tie is a typed error");
 }
 #[test]fn analytic_normals_match_finite_difference(){
 // LCG-sampled generic points: analytic gradient vs central FD of sample(); smooth ops included.
 let graphs=[Graph{nodes:vec![Field::Torus{center:[0.1,-0.2,0.3],major_radius:1.4,minor_radius:0.5}],output:0},
  Graph{nodes:vec![Field::Capsule{a:[-0.8,-0.3,0.2],b:[0.7,0.5,-0.4],radius:0.45}],output:0},
  Graph{nodes:vec![Field::Cylinder{center:[0.2,0.,-0.1],radius:0.8,half_height:1.1}],output:0},
  Graph{nodes:vec![Field::Sphere{center:[-0.4,0.,0.],radius:0.9},Field::Torus{center:[0.3,0.1,0.],major_radius:0.9,minor_radius:0.35},Field::SmoothUnion{a:0,b:1,k:0.3}],output:2},
  Graph{nodes:vec![Field::Box{center:[0.;3],half_size:[0.7,0.5,0.9]},Field::Capsule{a:[-0.5;3],b:[0.5;3],radius:0.4},Field::SmoothSubtract{a:0,b:1,k:0.25}],output:2}];
 let mut st=0x9e3779b97f4a7c15u64;let mut rng=move||{st^=st<<13;st^=st>>7;st^=st<<17;(st>>11) as f64/(1u64<<53) as f64};
 let mut checked=0u32;let mut worst=0f64;
 for g in &graphs{let mut n=0;while n<60{let p=[rng()*3.-1.5,rng()*3.-1.5,rng()*3.-1.5];
   let Ok(na)=g.normal_analytic(p)else{continue};
   let h=1e-6;let mut nf=[0.;3];for j in 0..3{let mut a=p;let mut b=p;a[j]+=h;b[j]-=h;nf[j]=g.sample(a).unwrap()-g.sample(b).unwrap();}
   let l=(nf[0]*nf[0]+nf[1]*nf[1]+nf[2]*nf[2]).sqrt();if l==0.{continue}for v in &mut nf{*v/=l}
   let dot=(na[0]*nf[0]+na[1]*nf[1]+na[2]*nf[2]).clamp(-1.,1.);let ang=dot.acos();
   if ang>worst{worst=ang}
   assert!(ang<0.02,"analytic {na:?} vs FD {nf:?} at {p:?}: {ang} rad");
   n+=1;checked+=1;}}
 assert!(checked>=280,"sampled {checked}");
 println!("ANALYTIC-NORMAL FD-agreement: {checked} points, worst angle {:.2e} rad",worst);
 }
 #[test]fn new_primitives_validation_and_csg(){
 let bad=[Field::Cylinder{center:[0.;3],radius:0.,half_height:1.},Field::Cylinder{center:[0.;3],radius:1.,half_height:-1.},Field::Capsule{a:[0.;3],b:[0.;3],radius:0.5},Field::Capsule{a:[0.;3],b:[0.,1.,0.],radius:0.},Field::Torus{center:[0.;3],major_radius:0.,minor_radius:0.5},Field::Torus{center:[f64::NAN;3],major_radius:1.,minor_radius:0.5}];
 for (i,n)in bad.into_iter().enumerate(){let g=Graph{nodes:vec![n],output:0};assert!(g.validate().is_err(),"bad primitive {i} accepted");assert!(g.sample([0.;3]).is_err(),"bad primitive {i} sampled");}
 let mut g=Graph{nodes:vec![Field::Capsule{a:[-1.,0.,0.],b:[1.,0.,0.],radius:0.4},Field::Torus{center:[0.;3],major_radius:1.,minor_radius:0.45},Field::SmoothIntersect{a:0,b:1,k:0.2}],output:2};
 assert!(g.validate().is_ok()&&g.sample([1.,0.,0.]).unwrap()<0.,"capsule inside smooth-intersect lens");
 g.nodes[2]=Field::SmoothSubtract{a:1,b:0,k:0.2};
 assert!(g.sample([0.9,0.,0.]).unwrap()>0.,"capsule carved out of torus");
 g.nodes[2]=Field::Union{a:0,b:1};
 assert!(g.sample([1.,0.,0.]).unwrap()<0.);
 g.nodes[2]=Field::Subtract{a:1,b:0};
 assert!(g.sample([0.9,0.,0.]).unwrap()>0.);
 }}
