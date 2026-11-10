//! Explicit per-effect encoding sandwich. Caller names current working gamut.
use crate::{effects::{Frame,Params},model::Effect};
use std::collections::BTreeMap;
pub fn apply(kind:&str,pr:&Params,strings:&BTreeMap<String,String>,f:&mut Frame,t:f64,seed:u64)->Result<(),String>{
    let id=|k:&str,max:usize|->Result<usize,String>{let n=strings.get(k).ok_or(format!("missing wrapper {k}"))?.parse::<usize>().map_err(|_|format!("bad wrapper {k}"))?;if n>max{return Err(format!("invalid wrapper {k}"))}Ok(n)};
    let w=id("wrapper_working_gamut",7)?;let g=id("wrapper_gamut",7)?;let tr=id("wrapper_transfer",8)?;
    let white=strings.get("wrapper_reference_white").map(|s|s.parse::<f64>()).transpose().map_err(|_|"bad wrapper white")?.unwrap_or(100.);if !white.is_finite()||!(1. ..=10000.).contains(&white){return Err("invalid wrapper white".into())}
    let xform=|f:&mut Frame,a:usize,b:usize,x:usize,y:usize|{let mut p=Params::new();for(k,v)in[("from_gamut",a),("to_gamut",b),("from_transfer",x),("to_transfer",y)]{p.insert(k.into(),crate::keyframe::Prop::new(v as f64));}p.insert("reference_white".into(),crate::keyframe::Prop::new(white));super::managed::apply(&p,f,0.)};
    xform(f,w,g,0,tr)?;let mut s=strings.clone();s.remove("wrapper_working_gamut");crate::effects::apply(kind,pr,&s,f,t,seed)?;xform(f,g,w,tr,0)
}
pub fn configure(e:&mut Effect,w:usize,g:usize,t:usize)->Result<(),String>{if w>7||g>7||t>8{return Err("wrapper gamut 0..7, transfer0..8".into())}for(k,v)in[("wrapper_working_gamut",w),("wrapper_gamut",g),("wrapper_transfer",t)]{e.strings.insert(k.into(),v.to_string());}Ok(())}

pub fn configure_white(e:&mut Effect,w:usize,g:usize,t:usize,white:f64)->Result<(),String>{if !white.is_finite()||!(1. ..=10000.).contains(&white){return Err("invalid wrapper reference white".into())}configure(e,w,g,t)?;e.strings.insert("wrapper_reference_white".into(),white.to_string());Ok(())}
