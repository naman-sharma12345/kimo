//! Image-based lighting on the PBR path: split-sum environment BRDF integration LUT.
//! Approach: Epic's split-sum (Karis 2013) as implemented in google/filament (Apache-2.0)
//! DFG LUT and KhronosGroup/glTF-Sample-Renderer (Apache-2.0) brdfLUT, adapted to kimo's
//! glTF-reference height-correlated V_GGX (threed/pbr.rs). Formulas re-derived against the
//! same integral; no upstream code copied. Zero new dependencies.
use serde::{Serialize,Deserialize};
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
pub fn brdf_lut(size:usize,samples:usize)->Result<BrdfLut,String>{if size==0||size>1024{return Err("lut size 1..=1024 required".into());}let mut texels=Vec::with_capacity(size*size);for r in 0..size{crate::guard::check();let alpha=(r as f64+0.5)/size as f64;for c in 0..size{let ndotv=(c as f64+0.5)/size as f64;let[a,b]=integrate_brdf(ndotv,alpha,samples)?;texels.push([a as f32,b as f32]);}}Ok(BrdfLut{size,texels})}

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
}
