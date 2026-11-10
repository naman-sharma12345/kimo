//! ASC CDL-style slope/offset/power/saturation in the caller's chosen encoding.
use crate::effects::{p,Frame,Params};
pub fn apply(pr:&Params,f:&mut Frame,t:f64)->Result<(),String>{
    super::frame::validate(f)?;
    let get=|k:&str,d:f64|->Result<f32,String>{let v=p(pr,k,d,t);if !v.is_finite()||v.abs()>100.{return Err(format!("invalid CDL {k}"))}Ok(v)};
    let slope=[get("slope_r",1.)?,get("slope_g",1.)?,get("slope_b",1.)?];let offset=[get("offset_r",0.)?,get("offset_g",0.)?,get("offset_b",0.)?];let power=[get("power_r",1.)?,get("power_g",1.)?,get("power_b",1.)?];let sat=get("saturation",1.)?;
    if slope.iter().any(|v|*v<0.)||power.iter().any(|v|*v<=0.)||sat<0.{return Err("CDL slope/saturation nonnegative and power positive required".into())}
    for(i,px) in f.d.chunks_exact_mut(4).enumerate(){if i%65536==0{crate::guard::check()}let mut c=[0.;3];for j in 0..3{c[j]=(px[j]*slope[j]+offset[j]).clamp(0.,1.).powf(power[j])}let y=0.2126*c[0]+0.7152*c[1]+0.0722*c[2];for j in 0..3{px[j]=(y+sat*(c[j]-y)).clamp(0.,1.)}}Ok(())
}
pub fn catalog()->Vec<super::Entry>{vec![("cdl","ASC CDL-style clamped SOP+sat: clamp(input*slope+offset)^power then BT709-weighted saturation. Encoding is caller-defined, not automatically scene-linear. No XML import/export yet.",&[("slope_r",1.,"slope"),("slope_g",1.,""),("slope_b",1.,""),("offset_r",0.,"offset"),("offset_g",0.,""),("offset_b",0.,""),("power_r",1.,"positive power"),("power_g",1.,""),("power_b",1.,""),("saturation",1.,"nonnegative saturation")])]}
