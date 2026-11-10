use kimo_core::particle_fields::{simulate,State,Force,Plane};
fn state()->State{State{position:[0.,2.,0.],velocity:[1.,0.,0.],mass:1.,radius:0.25}}
#[test]fn fixed_acceleration_matches_discrete_oracle_and_converges(){for steps in [10u32,100,1000]{let dt=1.0/steps as f64;let out=simulate(state(),&[Force::Acceleration{value:[0.,-1.,0.]}],&[],dt,steps).unwrap();let s=out.last().unwrap();assert!((s.position[0]-1.0).abs()<1e-10);let discrete=2.0-0.5*(1.0+dt);assert!((s.position[1]-discrete).abs()<1e-10);assert!((s.position[1]-1.5).abs()<=dt);}}
#[test]fn sweep_prevents_fast_tunneling_and_restitution(){let p=Plane{normal:[0.,1.,0.],offset:0.,restitution:1.};let s=State{velocity:[0.,-1000.,0.],..state()};let out=simulate(s,&[],&[p],0.01,1).unwrap();let s=&out[1];assert!(s.position[1]>=0.25);assert_eq!(s.velocity[1],1000.);assert!((s.position[1]-8.5).abs()<1e-10);}
#[test]fn deterministic_order_repeated_query_and_force_mass_contract(){let f=[Force::VectorField{value:[2.,0.,0.]}];let a=simulate(State{mass:2.,..state()},&f,&[],0.1,20).unwrap();let b=simulate(State{mass:2.,..state()},&f,&[],0.1,20).unwrap();assert_eq!(serde_json::to_string(&a).unwrap(),serde_json::to_string(&b).unwrap());assert!((a[20].velocity[0]-3.).abs()<1e-10);}
#[test]fn bad_dt_and_nonunit_plane_rejected(){assert!(simulate(state(),&[],&[],0.,1).is_err());assert!(simulate(state(),&[],&[Plane{normal:[0.,2.,0.],offset:0.,restitution:0.5}],0.1,1).is_err());}
#[test]fn drag_loses_energy_at_small_step(){let out=simulate(State{velocity:[2.,0.,0.],..state()},&[Force::LinearDrag{coefficient:1.}],&[],0.001,1000).unwrap();assert!((out[1000].velocity[0]-2.0*(-1.0f64).exp()).abs()<0.001);}
/// 1080p trajectory panels for external visual inspection: gravity parabola,
/// plane bounce, drag spacing, attractor curve. Side-view orthographic dots,
/// not a golden update and not a render-quality claim.
#[test]#[ignore="1080p trajectory panels after numeric tests"]
fn render_1080p_particle_panels(){
 use std::{io::Write,path::PathBuf};
 let dir=PathBuf::from(std::env::var("KIMO_PARTICLE_FRAMES").expect("set output directory"));std::fs::create_dir_all(&dir).unwrap();
 let (w,h)=(1920usize,1080usize);
 let scenes:[(&str,State,Vec<Force>,Vec<Plane>,f64,u32,(usize,usize));4]=[
  ("gravity",state(),vec![Force::Acceleration{value:[0.,-9.8,0.]}],vec![],0.005,200,(0,1)),
  ("bounce",State{position:[0.,2.,0.],velocity:[3.,0.,0.],mass:1.,radius:0.25},vec![Force::Acceleration{value:[0.,-9.8,0.]}],vec![Plane{normal:[0.,1.,0.],offset:0.,restitution:0.8}],0.005,400,(0,1)),
  ("drag",State{position:[0.,1.,0.],velocity:[12.,0.,0.],mass:1.,radius:0.25},vec![Force::LinearDrag{coefficient:1.}],vec![],0.01,300,(0,1)),
  ("attractor",State{position:[-2.,1.,0.],velocity:[0.,0.,1.4],mass:1.,radius:0.25},vec![Force::Attractor{position:[0.,1.,0.],strength:3.,softening:0.3}],vec![],0.005,600,(0,2))];
 for (name,st,forces,planes,dt,steps,(ax,ay)) in scenes{
  let traj=simulate(st,&forces,&planes,dt,steps).unwrap();
  let (mut mnx,mut mxx,mut mny,mut mxy)=(f64::INFINITY,f64::NEG_INFINITY,f64::INFINITY,f64::NEG_INFINITY);
  for s in &traj{mnx=mnx.min(s.position[ax]);mxx=mxx.max(s.position[ax]);mny=mny.min(s.position[ay]);mxy=mxy.max(s.position[ay]);}
  if name=="attractor"{mnx=mnx.min(-3.);mxx=mxx.max(3.);mny=mny.min(-1.);mxy=mxy.max(5.);}
  let pad=0.5; mnx-=pad;mxx+=pad;mny-=pad;mxy+=pad;
  let mut img=vec![0u8;w*h*3];
  for i in 0..w*h{let gx=(i%w/120+i/w/120)%2;let v=if gx==0{24}else{40};img[i*3]=v;img[i*3+1]=v;img[i*3+2]=v+8;}
  let n=traj.len();
  for (k,s) in traj.iter().enumerate(){
   let px=((s.position[ax]-mnx)/(mxx-mnx)*(w as f64-1.0)) as i32;
   let py=((1.0-(s.position[ay]-mny)/(mxy-mny))*(h as f64-1.0)) as i32;
   let t=k as f64/(n-1).max(1) as f64;
   let (r,g,b)=((255.0*(1.0-t)) as u8,(255.0*t) as u8,180u8);
   for dy in -3..=3{for dx in -3..=3{if dx*dx+dy*dy<=9{let (x,y)=(px+dx,py+dy);if x>=0&&x<w as i32&&y>=0&&y<h as i32{let i=(y as usize*w+x as usize)*3;img[i]=r;img[i+1]=g;img[i+2]=b;}}}}
  }
  let mut file=std::fs::File::create(dir.join(format!("{name}.ppm"))).unwrap();write!(file,"P6\n{w} {h}\n255\n").unwrap();file.write_all(&img).unwrap();
 }
}
