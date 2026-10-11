//! Explicit Kimo LGG in ACEScct. Not an application-specific primary-wheel match.
use crate::effects::{p,Frame,Params};use std::collections::BTreeMap;
pub fn map(x:f64,lift:f64,gamma:f64,gain:f64,offset:f64,inverse:bool)->f64{if inverse{let a=(x-offset)/gain;(a.abs().powf(gamma).copysign(a)-lift)/(1.-lift)}else{let a=x+lift*(1.-x);a.abs().powf(1./gamma).copysign(a)*gain+offset}}
pub fn apply(pr:&Params,strings:&BTreeMap<String,String>,f:&mut Frame,t:f64)->Result<(),String>{
 super::frame::validate(f)?;
 let g=|k:&str,d:f64|p(pr,k,d,t) as f64;
 let lift=[g("lift_r",0.),g("lift_g",0.),g("lift_b",0.)];let gamma=[g("gamma_r",1.),g("gamma_g",1.),g("gamma_b",1.)];let gain=[g("gain_r",1.),g("gain_g",1.),g("gain_b",1.)];let offset=[g("offset_r",0.),g("offset_g",0.),g("offset_b",0.)];let inv=g("inverse",0.);
 if g("gamut",3.)!=3.||lift.iter().any(|v|!v.is_finite()||*v< -1.||*v>=1.)||gamma.iter().any(|v|!v.is_finite()||*v<0.1||*v>10.)||gain.iter().any(|v|!v.is_finite()||*v<0.01||*v>10.)||offset.iter().any(|v|!v.is_finite()||v.abs()>1.)||![0.,1.].contains(&inv){return Err("invalid ACEScct LGG params: AP1only,lift[-1,1),gamma[0.1,10],gain[0.01,10],offset+-1,inverse0/1".into())}
 #[cfg(not(feature="native-ocio"))]{let _=(strings,t,lift,gamma,gain,offset);Err("ACEScctgrade requires native-ocio and OCIO2 library".into())}
 #[cfg(feature="native-ocio")]{let path=strings.get("config_path").ok_or("ACEScctgrade needs config_path")?;let linear=strings.get("linear_space").ok_or("declaredAP1linear_space required")?;let cct=strings.get("cct_space").ok_or("declaredACEScct_space required")?;let c=super::ocio_config::Config::from_file(path)?;let enc=c.processor(linear,cct)?;let dec=c.processor(cct,linear)?;
 if lift==[0.;3]&&gamma==[1.;3]&&gain==[1.;3]&&offset==[0.;3]{return Ok(())}
 crate::guard::reserve(f.d.len().checked_mul(4).ok_or("gradeallocation overflow")?);let mut out=Frame{w:f.w,h:f.h,d:f.d.clone()};enc.apply(&mut out)?;
 for(i,px)in out.d.chunks_exact_mut(4).enumerate(){if i%65536==0{crate::guard::check()}for j in 0..3{let v=map(px[j] as f64,lift[j],gamma[j],gain[j],offset[j],inv==1.);if !v.is_finite()||v.abs()>f32::MAX as f64{return Err("ACEScctgrade overflow".into())}px[j]=v as f32;}}
 dec.apply(&mut out)?;f.d=out.d;Ok(())}
}
pub fn catalog()->Vec<super::Entry>{vec![("acescct_primaries","NativeOCIOAP1->ACEScct, explicitKimo signedLGG+offset, backAP1. RequiresboundAP1project/configspaces. NOT Resolve wheel parity.",&[("gamut",3.,"AP1only"),("lift_r",0.,"-1..1exclusive"),("lift_g",0.,""),("lift_b",0.,""),("gamma_r",1.,"0.1..10"),("gamma_g",1.,""),("gamma_b",1.,""),("gain_r",1.,"0.01..10"),("gain_g",1.,""),("gain_b",1.,""),("offset_r",0.,"+-1"),("offset_g",0.,""),("offset_b",0.,""),("inverse",0.,"0/1")])]}
