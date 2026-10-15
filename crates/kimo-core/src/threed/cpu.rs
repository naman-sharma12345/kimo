use super::mesh::Mesh;
use crate::effects::Frame;
/// Initial orthographic reference raster. Mesh x/y in NDC, z in0..1.
/// Not a complete perspective camera/material renderer. Triangles outside depth
/// domain are rejected pending near/far polygon clipping, never silently dropped.
pub fn render(mesh:&Mesh,w:usize,h:usize,color:[f32;4])->Result<(Frame,Vec<f64>),String>{mesh.validate()?;if w==0||h==0||w>4096||h>4096||color.iter().any(|v|!v.is_finite())||!(0.0..=1.0).contains(&color[3]){return Err("target1..4096 and finite straight RGBA required".into());}if mesh.positions.iter().any(|p|!(0.0..=1.0).contains(&p[2])){return Err("reference raster requires z0..1; depth clipping pending".into());}let mut out=Frame::try_new(w,h)?;let mut depth=vec![f64::INFINITY;w*h];let edge=|a:[f64;3],b:[f64;3],x:f64,y:f64|(b[0]-a[0])*(y-a[1])-(b[1]-a[1])*(x-a[0]);
 for idx in &mesh.indices{crate::guard::check();let p=idx.map(|i|{let v=mesh.positions[i as usize];[(v[0]+1.0)*0.5*w as f64,(1.0-v[1])*0.5*h as f64,v[2]]});let area=edge(p[0],p[1],p[2][0],p[2][1]);if area==0.0{continue;}let lo_x=p.iter().map(|p|p[0]).fold(f64::INFINITY,f64::min).floor().max(0.0).min(w as f64) as usize;let hi_x=p.iter().map(|p|p[0]).fold(f64::NEG_INFINITY,f64::max).ceil().max(0.0).min(w as f64) as usize;let lo_y=p.iter().map(|p|p[1]).fold(f64::INFINITY,f64::min).floor().max(0.0).min(h as f64) as usize;let hi_y=p.iter().map(|p|p[1]).fold(f64::NEG_INFINITY,f64::max).ceil().max(0.0).min(h as f64) as usize;
 for y in lo_y..hi_y{crate::guard::check();for x in lo_x..hi_x{let (sx,sy)=(x as f64+0.5,y as f64+0.5);let b=[edge(p[1],p[2],sx,sy)/area,edge(p[2],p[0],sx,sy)/area,edge(p[0],p[1],sx,sy)/area];if b.iter().any(|v|*v<0.0){continue;}let z=(0..3).map(|i|b[i]*p[i][2]).sum::<f64>();let i=y*w+x;if z<depth[i]{depth[i]=z;out.d[i*4..i*4+4].copy_from_slice(&color);}}}
 }Ok((out,depth))}

#[cfg(test)]
mod tests{
 use super::*;
 fn tri(p:[[f64;3];3])->Mesh{Mesh{positions:p.to_vec(),indices:vec![[0,1,2]]}}
 fn bitmap(f:&Frame)->Vec<String>{(0..f.h).map(|y|(0..f.w).map(|x|if f.d[(y*f.w+x)*4+3]>0.{'X'}else{'.'}).collect()).collect()}
 // Coverage bitmaps pinned against an independent Python transcription of the
 // edge-function rasterizer (b>=0 inside test, pixel-center sampling).
 #[test]fn coverage_bitmap_matches_independent_pin(){let(f,d)=render(&tri([[-1.,1.,0.5],[1.,1.,0.5],[-1.,-1.,0.5]]),8,8,[1.,0.,0.,1.]).unwrap();
  assert_eq!(bitmap(&f),["XXXXXXXX","XXXXXXX.","XXXXXX..","XXXXX...","XXXX....","XXX.....","XX......","X......."]);
  assert!(d.iter().filter(|z|z.is_finite()).all(|z|*z==0.5),"flat triangle writes constant depth");
  let(f,_)=render(&tri([[-1.,-1.,0.5],[1.,-1.,0.5],[1.,1.,0.5]]),8,8,[1.,0.,0.,1.]).unwrap();
  assert_eq!(bitmap(&f),[".......X","......XX",".....XXX","....XXXX","...XXXXX","..XXXXXX",".XXXXXXX","XXXXXXXX"]);}
 #[test]fn nearer_triangle_wins_depth(){let near=tri([[-1.,1.,0.25],[1.,1.,0.25],[-1.,-1.,0.25]]);let far=tri([[-1.,1.,0.75],[1.,1.,0.75],[1.,-1.,0.75]]);
  let both=Mesh{positions:near.positions.iter().chain(far.positions.iter()).cloned().collect(),indices:vec![[0,1,2],[3,4,5]]};
  let(f,d)=render(&both,8,8,[0.25,0.5,1.,1.]).unwrap();
  let near_cov=["XXXXXXXX","XXXXXXX.","XXXXXX..","XXXXX...","XXXX....","XXX.....","XX......","X......."];
  let far_cov=["XXXXXXXX",".XXXXXXX","..XXXXXX","...XXXXX","....XXXX",".....XXX","......XX",".......X"];
  for y in 0..8{for x in 0..8{let i=(y*8+x)*4;let n=near_cov[y].as_bytes()[x]==b'X';let f2=far_cov[y].as_bytes()[x]==b'X';
   assert_eq!(f.d[i+3]>0.,n||f2,"pixel ({x},{y}) covered iff either triangle covers");
   if n{assert_eq!(d[y*8+x],0.25,"pixel ({x},{y}): nearer triangle wins wherever it covers");}
   else if f2{assert_eq!(d[y*8+x],0.75,"pixel ({x},{y}): far triangle only");}
   else{assert!(d[y*8+x].is_infinite(),"pixel ({x},{y}): uncovered keeps infinite depth");}}}}
 #[test]fn z_domain_and_degenerate_and_typed_errors(){assert_eq!(render(&tri([[0.,0.,1.5],[1.,0.,0.5],[0.,1.,0.5]]),8,8,[1.;4]).err().unwrap(),"reference raster requires z0..1; depth clipping pending");
  assert!(render(&tri([[0.,0.,0.5],[1.,1.,0.5],[2.,2.,0.5]]),8,8,[1.;4]).unwrap().0.d.iter().all(|v|*v==0.),"zero-area triangle skipped silently");
  let t=tri([[-1.,1.,0.5],[1.,1.,0.5],[-1.,-1.,0.5]]);
  assert_eq!(render(&t,0,8,[1.;4]).err().unwrap(),"target1..4096 and finite straight RGBA required");
  assert_eq!(render(&t,4097,8,[1.;4]).err().unwrap(),"target1..4096 and finite straight RGBA required");
  assert_eq!(render(&t,8,8,[1.,1.,1.,1.5]).err().unwrap(),"target1..4096 and finite straight RGBA required");
  assert_eq!(render(&t,8,8,[f32::NAN;4]).err().unwrap(),"target1..4096 and finite straight RGBA required");}
}
