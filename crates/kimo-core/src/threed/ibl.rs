//! Image-based lighting on the PBR path: split-sum environment BRDF integration LUT.
//! Approach: Epic's split-sum (Karis 2013) as implemented in google/filament (Apache-2.0)
//! DFG LUT and KhronosGroup/glTF-Sample-Renderer (Apache-2.0) brdfLUT, adapted to kimo's
//! glTF-reference height-correlated V_GGX (threed/pbr.rs). Formulas re-derived against the
//! same integral; no upstream code copied. Zero new dependencies.
use serde::{Serialize,Deserialize};
/// Explicit maximum payload for one baked IBL set (does not include the source map).
pub const MAX_IBL_BAKE_BYTES:usize=128*1024*1024;
fn map_len(width:usize,height:usize,error:&str)->Result<usize,String>{
 if width==0||height==0||width>4096||height>4096{return Err(error.into());}
 width.checked_mul(height).ok_or_else(||error.into())
}
fn empty_texels<T>(len:usize)->Result<Vec<T>,String>{
 let mut out=Vec::new();out.try_reserve_exact(len).map_err(|_|"IBL allocation failed".to_string())?;Ok(out)
}
fn bake_bytes(irr:[usize;2],pf:[usize;2],levels:usize,lut:usize,samples:usize)->Result<usize,String>{
 if levels==0||levels>16{return Err("prefilter levels 1..=16 required".into());}
 let ni=map_len(irr[0],irr[1],"irradiance map size 1..=4096 per axis required")?;
 let np=map_len(pf[0],pf[1],"prefiltered map 1..=4096 per axis required")?;
 if lut==0||lut>1024{return Err("lut size 1..=1024 required".into());}
 if samples==0||samples>65536{return Err("bake samples 1..=65536 required".into());}
 let bytes=np.checked_mul(levels).and_then(|v|v.checked_add(ni)).and_then(|v|v.checked_mul(std::mem::size_of::<[f32;3]>())).and_then(|v|lut.checked_mul(lut).and_then(|n|n.checked_mul(std::mem::size_of::<[f32;2]>())).and_then(|n|v.checked_add(n))).ok_or("IBL bake byte count overflow")?;
 if bytes>MAX_IBL_BAKE_BYTES{return Err("IBL bake payload exceeds 128 MiB limit".into());}
 Ok(bytes)
}

/// Deterministic low-discrepancy sequence (van der Corput radical inverse in base 2).
fn hammersley(i:usize,n:usize)->[f64;2]{let x=i as f64/n as f64;let mut b=i;let mut inv=0.0;let mut f=0.5;while b>0{if b&1==1{inv+=f;}b>>=1;f*=0.5;}[x,inv]}
/// GGX half-vector inverse-CDF sample in the tangent frame (N = +Z).
fn importance_sample_ggx(xi:[f64;2],alpha:f64)->[f64;3]{let a2=alpha*alpha;let phi=std::f64::consts::TAU*xi[0];let cos_t=((1.0-xi[1])/(1.0+(a2-1.0)*xi[1])).sqrt();let sin_t=(1.0-cos_t*cos_t).max(0.0).sqrt();[sin_t*phi.cos(),sin_t*phi.sin(),cos_t]}
/// Environment BRDF integral for one (cosines, alpha) pair: returns [scale, bias] with
/// specular_ibl = F0 * scale + bias. Importance-samples D and folds in the correlated
/// visibility, so per-sample weight = 2*vh*nl / (nh*(GGXV+GGXL)).
pub fn integrate_brdf(ndotv:f64,alpha:f64,samples:usize)->Result<[f64;2],String>{if !ndotv.is_finite()||!alpha.is_finite()||!(0.0..=1.0).contains(&ndotv)||ndotv==0.0||!(0.0..=1.0).contains(&alpha)||alpha==0.0||samples==0||samples>65536{return Err("finite 0<ndotv<=1, 0<alpha<=1, 1..=65536 samples required".into());}
 let a2=alpha*alpha;let v=[(1.0-ndotv*ndotv).max(0.0).sqrt(),0.,ndotv];let(mut a,mut b)=(0.,0.);
 for i in 0..samples{let h=importance_sample_ggx(hammersley(i,samples),alpha);let vh=v[0]*h[0]+v[1]*h[1]+v[2]*h[2];let l=[2.0*vh*h[0]-v[0],2.0*vh*h[1]-v[1],2.0*vh*h[2]-v[2]];let nl=l[2];
  if nl>0.0{let nh=h[2];let ggxv=nl*(ndotv*ndotv*(1.0-a2)+a2).sqrt();let ggxl=ndotv*(nl*nl*(1.0-a2)+a2).sqrt();let g_vis=2.0*vh*nl/(nh*(ggxv+ggxl));if g_vis.is_finite(){let fc=(1.0-vh).powi(5);a+=(1.0-fc)*g_vis;b+=fc*g_vis;}}}
 Ok([a/samples as f64,b/samples as f64])}
/// Preintegrated environment BRDF over a size x size (ndotv x alpha) grid; texel centers.
#[derive(Clone,Debug,Serialize,Deserialize)]pub struct BrdfLut{pub size:usize,pub texels:Vec<[f32;2]>}
pub fn brdf_lut(size:usize,samples:usize)->Result<BrdfLut,String>{if size==0||size>1024{return Err("lut size 1..=1024 required".into());}let mut texels=empty_texels(size*size)?;for r in 0..size{crate::guard::check();let alpha=(r as f64+0.5)/size as f64;for c in 0..size{let ndotv=(c as f64+0.5)/size as f64;let[a,b]=integrate_brdf(ndotv,alpha,samples)?;texels.push([a as f32,b as f32]);}}Ok(BrdfLut{size,texels})}

/// Equirectangular environment map of linear radiance. Convention: u = atan2(z,x)/TAU + 0.5
/// (wrapped), v = acos(y)/PI, +Y up. Bilinear sampling with wrapped u, clamped v.
#[derive(Clone,Debug,Serialize,Deserialize)]pub struct EnvMap{pub width:usize,pub height:usize,pub texels:Vec<[f32;3]>}
impl EnvMap{
 pub fn from_fn(width:usize,height:usize,f:impl Fn([f64;3])->[f64;3])->Result<Self,String>{if width==0||height==0||width>4096||height>4096{return Err("env map 1..=4096 per axis required".into());}let mut texels=empty_texels(map_len(width,height,"env map 1..=4096 per axis required")?)?;for j in 0..height{for i in 0..width{crate::guard::check();let u=(i as f64+0.5)/width as f64;let v=(j as f64+0.5)/height as f64;let phi=(u-0.5)*std::f64::consts::TAU;let y=(v*std::f64::consts::PI).cos();let r=(v*std::f64::consts::PI).sin();let c=f([r*phi.cos(),y,r*phi.sin()]);if c.iter().any(|v|!v.is_finite()){return Err("env generator must return finite radiance".into());}texels.push([c[0]as f32,c[1]as f32,c[2]as f32]);}}Ok(Self{width,height,texels})}
 pub fn sample(&self,dir:[f64;3])->Result<[f64;3],String>{if self.width==0||self.height==0||self.texels.len()!=self.width*self.height{return Err("env map buffers consistent required".into());}if dir.iter().any(|v|!v.is_finite())||dir.iter().all(|v|*v==0.0){return Err("finite nonzero direction required".into());}let l=(dir[0]*dir[0]+dir[1]*dir[1]+dir[2]*dir[2]).sqrt();let d=[dir[0]/l,dir[1]/l,dir[2]/l];let u=d[2].atan2(d[0])/std::f64::consts::TAU+0.5;let v=d[1].clamp(-1.,1.).acos()/std::f64::consts::PI;
  let x=u*self.width as f64-0.5;let y=(v*self.height as f64-0.5).clamp(0.,(self.height-1)as f64);let x0=x.floor();let fx=x-x0;let y0=y.floor()as usize;let fy=y-y0 as f64;let i0=(x0 as i64).rem_euclid(self.width as i64)as usize;let i1=(i0+1)%self.width;let j1=(y0+1).min(self.height-1);let mut out=[0.;3];
  for(w_,j,i)in[((1.-fx)*(1.-fy),y0,i0),(fx*(1.-fy),y0,i1),((1.-fx)*fy,j1,i0),(fx*fy,j1,i1)]{for k in 0..3{out[k]+=w_*self.texels[j*self.width+i][k]as f64;}}Ok(out)}
}
fn tangent_basis(n:[f64;3])->([f64;3],[f64;3]){let cr=|a:[f64;3],b:[f64;3]|[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];let un=|a:[f64;3]|{let l=(a[0]*a[0]+a[1]*a[1]+a[2]*a[2]).sqrt();[a[0]/l,a[1]/l,a[2]/l]};let t=if n[2].abs()<0.999{un(cr([0.,0.,1.],n))}else{un(cr([1.,0.,0.],n))};(t,cr(n,t))}
fn valid_dir_samples(name:&str,d:[f64;3],samples:usize)->Result<(),String>{if d.iter().any(|v|!v.is_finite())||d.iter().all(|v|*v==0.0)||samples==0||samples>65536{return Err(format!("finite nonzero {name} direction and 1..=65536 samples required"));}Ok(())}
/// Diffuse irradiance: cosine-convolved environment, raw (unnormalized by PI); shading
/// divides by PI so a constant env returns env*albedo. Cosine-weighted Monte Carlo.
pub fn irradiance(env:&EnvMap,n:[f64;3],samples:usize)->Result<[f64;3],String>{valid_dir_samples("irradiance",n,samples)?;let nl=(n[0]*n[0]+n[1]*n[1]+n[2]*n[2]).sqrt();let n=[n[0]/nl,n[1]/nl,n[2]/nl];let(t,b)=tangent_basis(n);let mut acc=[0.;3];
 for i in 0..samples{let xi=hammersley(i,samples);let phi=std::f64::consts::TAU*xi[0];let cos_t=xi[1].sqrt();let sin_t=(1.0-xi[1]).max(0.0).sqrt();let l=[t[0]*sin_t*phi.cos()+b[0]*sin_t*phi.sin()+n[0]*cos_t,t[1]*sin_t*phi.cos()+b[1]*sin_t*phi.sin()+n[1]*cos_t,t[2]*sin_t*phi.cos()+b[2]*sin_t*phi.sin()+n[2]*cos_t];let c=env.sample(l)?;for k in 0..3{acc[k]+=c[k];}}
 Ok(acc.map(|v|v*std::f64::consts::PI/samples as f64))}
/// Specular prefiltered environment for one lookup direction and alpha (split-sum first
/// sum): GGX importance samples around the reflection direction, nl-weighted average.
pub fn prefiltered_env(env:&EnvMap,dir:[f64;3],alpha:f64,samples:usize)->Result<[f64;3],String>{valid_dir_samples("prefilter",dir,samples)?;if !alpha.is_finite()||!(0.0..=1.0).contains(&alpha)||alpha==0.0{return Err("finite 0<alpha<=1 required".into());}let dl=(dir[0]*dir[0]+dir[1]*dir[1]+dir[2]*dir[2]).sqrt();let n=[dir[0]/dl,dir[1]/dl,dir[2]/dl];let(t,b)=tangent_basis(n);let mut acc=[0.;3];let mut tw=0.;
 for i in 0..samples{let h=importance_sample_ggx(hammersley(i,samples),alpha);let lt=[2.0*h[2]*h[0],2.0*h[2]*h[1],2.0*h[2]*h[2]-1.0];let lw=[t[0]*lt[0]+b[0]*lt[1]+n[0]*lt[2],t[1]*lt[0]+b[1]*lt[1]+n[1]*lt[2],t[2]*lt[0]+b[2]*lt[1]+n[2]*lt[2]];let nl=lw[0]*n[0]+lw[1]*n[1]+lw[2]*n[2];if nl>0.{let c=env.sample(lw)?;for k in 0..3{acc[k]+=c[k]*nl;}tw+=nl;}}
 if tw<=0.0{return Err("prefilter samples all below horizon".into());}Ok(acc.map(|v|v/tw))}
/// Full-map bakes for the shading path.
pub fn irradiance_map(env:&EnvMap,width:usize,height:usize,samples:usize)->Result<EnvMap,String>{let len=map_len(width,height,"irradiance map size 1..=4096 per axis required")?;let mut out=empty_texels(len)?;for j in 0..height{for i in 0..width{crate::guard::check();let u=(i as f64+0.5)/width as f64;let v=(j as f64+0.5)/height as f64;let phi=(u-0.5)*std::f64::consts::TAU;let y=(v*std::f64::consts::PI).cos();let r=(v*std::f64::consts::PI).sin();let c=irradiance(env,[r*phi.cos(),y,r*phi.sin()],samples)?;out.push([c[0]as f32,c[1]as f32,c[2]as f32]);}}Ok(EnvMap{width,height,texels:out})}
pub fn prefiltered_map(env:&EnvMap,width:usize,height:usize,alpha:f64,samples:usize)->Result<EnvMap,String>{let len=map_len(width,height,"prefiltered map 1..=4096 per axis required")?;let mut out=empty_texels(len)?;for j in 0..height{for i in 0..width{crate::guard::check();let u=(i as f64+0.5)/width as f64;let v=(j as f64+0.5)/height as f64;let phi=(u-0.5)*std::f64::consts::TAU;let y=(v*std::f64::consts::PI).cos();let r=(v*std::f64::consts::PI).sin();let c=prefiltered_env(env,[r*phi.cos(),y,r*phi.sin()],alpha,samples)?;out.push([c[0]as f32,c[1]as f32,c[2]as f32]);}}Ok(EnvMap{width,height,texels:out})}

impl BrdfLut{
 /// Bilinear sample at (ndotv, alpha) over texel centers, clamped to the grid.
 pub fn sample(&self,ndotv:f64,alpha:f64)->Result<[f64;2],String>{if self.size==0||self.texels.len()!=self.size*self.size{return Err("lut buffers consistent required".into());}if !ndotv.is_finite()||!alpha.is_finite(){return Err("finite ndotv/alpha required".into());}let n=self.size as f64;let x=(ndotv.clamp(0.,1.)*n-0.5).clamp(0.,(self.size-1)as f64);let y=(alpha.clamp(0.,1.)*n-0.5).clamp(0.,(self.size-1)as f64);let x0=x.floor()as usize;let y0=y.floor()as usize;let fx=x-x0 as f64;let fy=y-y0 as f64;let x1=(x0+1).min(self.size-1);let y1=(y0+1).min(self.size-1);
  let t=|xx:usize,yy:usize|self.texels[yy*self.size+xx];let mut out=[0.;2];for k in 0..1+1{let a=t(x0,y0)[k]as f64;let b=t(x1,y0)[k]as f64;let c=t(x0,y1)[k]as f64;let d=t(x1,y1)[k]as f64;out[k]=(1.-fy)*((1.-fx)*a+fx*b)+fy*((1.-fx)*c+fx*d);}Ok(out)}
}
/// Baked IBL set: cosine-convolved irradiance, per-alpha prefiltered levels, BRDF LUT.
#[derive(Clone,Debug,Serialize,Deserialize)]pub struct IblSet{pub irradiance:EnvMap,pub prefiltered:Vec<EnvMap>,pub lut:BrdfLut}
impl IblSet{
 pub fn bake(env:&EnvMap,irr_size:[usize;2],pf_size:[usize;2],levels:usize,lut_size:usize,samples:usize)->Result<Self,String>{bake_bytes(irr_size,pf_size,levels,lut_size,samples)?;let irradiance=irradiance_map(env,irr_size[0],irr_size[1],samples)?;let mut prefiltered=Vec::with_capacity(levels);for l in 0..levels{let alpha=(l as f64+0.5)/levels as f64;prefiltered.push(prefiltered_map(env,pf_size[0],pf_size[1],alpha,samples)?);}Ok(Self{irradiance,prefiltered,lut:brdf_lut(lut_size,samples)?})}
 pub fn validate(&self)->Result<(),String>{if self.prefiltered.is_empty()||self.prefiltered.len()>16{return Err("prefilter levels 1..=16 required".into());}Ok(())}
 /// Prefiltered specular at (R, alpha): linear level selection + interpolation.
 pub fn prefiltered_at(&self,r:[f64;3],alpha:f64)->Result<[f64;3],String>{self.validate()?;let l=self.prefiltered.len();let x=(alpha.clamp(0.,1.)*l as f64-0.5).clamp(0.,(l-1)as f64);let l0=x.floor()as usize;let l1=(l0+1).min(l-1);let f=x-l0 as f64;let a=self.prefiltered[l0].sample(r)?;if l0==l1{return Ok(a);}let b=self.prefiltered[l1].sample(r)?;Ok(std::array::from_fn(|k|a[k]*(1.-f)+b[k]*f))}
}

#[cfg(test)]
mod tests{
 use super::*;
 // LUT_EXPECTED is an independent Python (stdlib math) transcription of the identical
 // deterministic estimator (same Hammersley sequence, inverse CDF, correlated-V weight),
 // pinned at 1e-12.
 pub const LUT_SAMPLES:usize=1024;
 pub const LUT_NDOTV:[f64;5]=[0.05,0.25,0.5,0.75,0.95];
 pub const LUT_ALPHA:[f64;6]=[0.001,0.05,0.25,0.5,0.75,1.0];
 pub const LUT_EXPECTED:[[[f64;2];6];5]=[
 [
 [2.26524360998281188e-01,7.73466659139795842e-01],
 [3.95526339341494282e-01,4.89105059643717754e-01],
 [7.73822526406142908e-01,1.64189835449098070e-01],
 [8.45970040922458422e-01,7.55648494644381719e-02],
 [8.41039935924593030e-01,4.58130881011362748e-02],
 [8.15531021308833703e-01,3.22073208263436925e-02],
 ],
 [
 [7.62739645267632604e-01,2.37290996449853575e-01],
 [7.60030572572119367e-01,2.14313425558675663e-01],
 [7.54469504356716669e-01,8.11692610410730819e-02],
 [7.29894505023280682e-01,3.33689385721761753e-02],
 [6.62728199536688822e-01,1.77822086073785059e-02],
 [5.85625258273753002e-01,1.09026365417883991e-02],
 ],
 [
 [9.68765491442990734e-01,3.12500666259224388e-02],
 [9.61312189044581156e-01,3.25790504989449947e-02],
 [8.33190736220279171e-01,2.23349400171635425e-02],
 [6.87376423649477597e-01,1.01928278315289375e-02],
 [5.59361563191895339e-01,5.20691287774456991e-03],
 [4.47346999398708589e-01,2.97633766265587452e-03],
 ],
 [
 [9.99031597067721266e-01,9.76699611879709587e-04],
 [9.96024156853824505e-01,1.35779506062497701e-03],
 [8.90208578042457455e-01,2.73404879703360425e-03],
 [6.80511481880897384e-01,1.88652261941591403e-03],
 [5.00824112598378668e-01,1.07698407989648137e-03],
 [3.64074933904007025e-01,6.30560703329275307e-04],
 ],
 [
 [1.00000287010263689e+00,3.12927924324815136e-07],
 [9.97746224600035814e-01,1.30167839600308615e-05],
 [9.11599509951063647e-01,1.28101918975648087e-04],
 [6.86103863374929901e-01,1.67282202070615546e-04],
 [4.70319320328850210e-01,1.28498814863361034e-04],
 [3.16976684738577097e-01,8.63649792613813095e-05],
 ],
 ];
 // max A+B over oracle grid (F0=1 directional albedo, correlated single-scatter): 1.00003064171748624e+00
 fn close(a:f64,b:f64)->bool{(a-b).abs()<=1e-12*b.abs().max(1.0)}
 #[test]fn invalid_bake_dimensions_and_aggregate_budget_fail_before_allocation(){
  let env=EnvMap{width:1,height:1,texels:vec![[1.;3]]};
  assert_eq!(irradiance_map(&env,usize::MAX,2,1).unwrap_err(),"irradiance map size 1..=4096 per axis required");
  assert_eq!(prefiltered_map(&env,2,usize::MAX,0.5,1).unwrap_err(),"prefiltered map 1..=4096 per axis required");
  assert_eq!(IblSet::bake(&env,[4096,4096],[4096,4096],16,1024,1).unwrap_err(),"IBL bake payload exceeds 128 MiB limit");
  assert_eq!(bake_bytes([1,1],[2,2],2,2,1).unwrap(),12+2*4*12+4*8);
  assert_eq!(IblSet::bake(&env,[1,1],[1,1],1,0,1).unwrap_err(),"lut size 1..=1024 required");
  assert_eq!(IblSet::bake(&env,[1,1],[1,1],1,1,0).unwrap_err(),"bake samples 1..=65536 required");
 }
 #[test]fn integrate_brdf_matches_python_oracle(){for (i,nv) in LUT_NDOTV.iter().enumerate(){for (j,alpha) in LUT_ALPHA.iter().enumerate(){let got=integrate_brdf(*nv,*alpha,LUT_SAMPLES).unwrap();let want=LUT_EXPECTED[i][j];assert!(close(got[0],want[0])&&close(got[1],want[1]),"nv {nv} alpha {alpha}: got {got:?} want {want:?}");}}}
 #[test]fn energy_and_smooth_limit_invariants(){for i in 0..5{for j in 0..6{let[a,b]=LUT_EXPECTED[i][j];assert!(a>=0.&&b>=0.,"non-negative scale/bias");assert!(a+b<=1.0001,"correlated single-scatter albedo <= 1 + sampling noise (measured grid max 1.0000306)");}}
  let[a,b]=integrate_brdf(0.95,0.001,1024).unwrap();assert!((a+b-1.0).abs()<1e-3,"smooth near-normal incidence conserves energy at F0=1 (measured 1.000003)");
  let[a,b]=integrate_brdf(0.95,1.0,1024).unwrap();assert!((a+b-0.317).abs()<0.005,"rough near-normal single-scatter albedo band (measured 0.317063)");}
 #[test]fn lut_grid_consistent_with_integrator_and_deterministic(){let l=brdf_lut(8,256).unwrap();assert_eq!(l.texels.len(),64);
  for r in 0..8{for c in 0..8{let[a,b]=integrate_brdf((c as f64+0.5)/8.,(r as f64+0.5)/8.,256).unwrap();assert_eq!(l.texels[r*8+c],[a as f32,b as f32],"texel ({c},{r}) equals f32-cast integrator at texel center");}}
  let m=brdf_lut(8,256).unwrap();assert!(l.texels.iter().zip(&m.texels).all(|(a,b)|a==b),"bit-exact regeneration");}
 #[test]fn typed_errors(){assert_eq!(integrate_brdf(0.,0.5,16).unwrap_err(),"finite 0<ndotv<=1, 0<alpha<=1, 1..=65536 samples required");
  assert_eq!(integrate_brdf(1.5,0.5,16).unwrap_err(),"finite 0<ndotv<=1, 0<alpha<=1, 1..=65536 samples required");
  assert_eq!(integrate_brdf(0.5,0.,16).unwrap_err(),"finite 0<ndotv<=1, 0<alpha<=1, 1..=65536 samples required");
  assert_eq!(integrate_brdf(f64::NAN,0.5,16).unwrap_err(),"finite 0<ndotv<=1, 0<alpha<=1, 1..=65536 samples required");
  assert_eq!(integrate_brdf(0.5,0.5,0).unwrap_err(),"finite 0<ndotv<=1, 0<alpha<=1, 1..=65536 samples required");
  assert_eq!(brdf_lut(0,16).unwrap_err(),"lut size 1..=1024 required");
  assert_eq!(brdf_lut(1025,16).unwrap_err(),"lut size 1..=1024 required");}
 const ENV_W:usize=16;
 const ENV_H:usize=8;
 const IRR_CASES:[([f64;3],[f64;3]);3]=[
([0.0, 1.0, 0.0],[2.34577067112141124e+00,1.18070319986390571e+00,4.03122900769657677e-01]),
([0.0, -1.0, 0.0],[7.95821982468381983e-01,1.56819037202716349e+00,1.95307158942268733e+00]),
([1.0, 0.0, 0.0],[1.57071539608477573e+00,1.37446701862306542e+00,1.17817817580629347e+00]),
];
 const PF_CASES:[([f64;3],f64,[f64;3]);3]=[
([0.0, 1.0, 0.0],0.5,[7.48889145784454979e-01,3.75277713553887005e-01,1.26110854215545548e-01]),
([0.3, 0.7, -0.6439500342258215],0.25,[7.32006050311738821e-01,3.79498487422066599e-01,1.42993949688263094e-01]),
([1.0, 0.0, 0.0],1.0,[4.99641029347310861e-01,4.37589742663172077e-01,3.75358970652689083e-01]),
];
// analytic: irr(+y)=pi*cup=['2.356194', '1.178097', '0.392699'], irr(-y)=pi*cdn, irr(+x)=(pi/2)*(cup+cdn)

 fn step_env()->EnvMap{EnvMap::from_fn(ENV_W,ENV_H,|d|if d[1]>0.{[0.75,0.375,0.125]}else{[0.25,0.5,0.625]}).unwrap()}
 fn close3(a:[f64;3],b:[f64;3],tol:f64)->bool{(0..3).all(|i|(a[i]-b[i]).abs()<=tol*b[i].abs().max(1.0))}
 #[test]fn irradiance_matches_python_oracle_and_analytic_step(){let env=step_env();for(n,want)in IRR_CASES{let got=irradiance(&env,n,1024).unwrap();assert!(close3(got,want,1e-12),"n {n:?}: got {got:?} want {want:?}");}
  let up=irradiance(&env,[0.,1.,0.],1024).unwrap();let analytic=[std::f64::consts::PI*0.75,std::f64::consts::PI*0.375,std::f64::consts::PI*0.125];assert!(close3(up,analytic,0.03),"near-analytic above the step (texel blur band 3%)");
  let x=irradiance(&env,[1.,0.,0.],1024).unwrap();let analytic=[std::f64::consts::FRAC_PI_2*1.0,std::f64::consts::FRAC_PI_2*0.875,std::f64::consts::FRAC_PI_2*0.75];assert!(close3(x,analytic,0.005),"side normal averages the step (band 0.5%)");}
 #[test]fn prefilter_matches_python_oracle(){let env=step_env();for(d,a,want)in PF_CASES{let got=prefiltered_env(&env,d,a,1024).unwrap();assert!(close3(got,want,1e-12),"dir {d:?} alpha {a}: got {got:?} want {want:?}");}}
 #[test]fn constant_env_identity_both_paths(){let env=EnvMap::from_fn(16,8,|_|[0.6,0.4,0.2]).unwrap();
  for a in [0.001,0.25,1.0]{let c=prefiltered_env(&env,[0.3,0.7,-0.6],a,256).unwrap();assert!(close3(c,[0.6,0.4,0.2],1e-6),"constant env survives prefilter at alpha {a}");}
  let c=irradiance(&env,[0.3,0.7,-0.6],256).unwrap();assert!(close3(c,[0.6*std::f64::consts::PI,0.4*std::f64::consts::PI,0.2*std::f64::consts::PI],1e-6),"constant env irradiance = pi*c");
  let m=prefiltered_map(&env,8,4,0.5,64).unwrap();assert!(m.texels.iter().all(|t|close3(t.map(|v|v as f64),[0.6,0.4,0.2],1e-6)),"baked map constant");
  let im=irradiance_map(&env,8,4,64).unwrap();assert!(im.texels.iter().all(|t|close3(t.map(|v|v as f64),[0.6*std::f64::consts::PI,0.4*std::f64::consts::PI,0.2*std::f64::consts::PI],1e-5)),"baked irradiance map constant*pi");}
 #[test]fn env_sample_texel_centers_exact_and_typed_errors(){let env=step_env();let c=env.sample([0.,1.,0.]).unwrap();assert!((c[0]-0.75).abs()<1e-6,"north pole row all up-color");
  assert_eq!(env.sample([0.,0.,0.]).unwrap_err(),"finite nonzero direction required");
  assert_eq!(EnvMap::from_fn(0,8,|_|[0.;3]).unwrap_err(),"env map 1..=4096 per axis required");
  assert_eq!(EnvMap::from_fn(8,8,|_|[f64::NAN;3]).unwrap_err(),"env generator must return finite radiance");
  assert_eq!(irradiance(&env,[0.,0.,0.],64).unwrap_err(),"finite nonzero irradiance direction and 1..=65536 samples required");
  assert_eq!(prefiltered_env(&env,[0.,1.,0.],0.,64).unwrap_err(),"finite 0<alpha<=1 required");
  assert_eq!(prefiltered_env(&env,[0.,1.,0.],0.5,0).unwrap_err(),"finite nonzero prefilter direction and 1..=65536 samples required");}
}
