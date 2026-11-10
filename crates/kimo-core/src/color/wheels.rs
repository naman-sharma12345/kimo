//! Exact Resolve-style lift/gamma/gain/offset wheels (primaries model 1): per-channel ASC CDL
//! (slope=gain-lift, offset=lift+offset, power=1/gamma), luma-preserving master, contrast, saturation.
use crate::effects::{p,Frame,Params};
const CH:[&str;4]=["r","g","b","m"];
fn neutral(l:f32,g:f32,k:f32,o:f32)->bool{l==0.&&g==1.&&k==1.&&o==0.}
pub fn primaries_exact(pr:&Params,f:&mut Frame,t:f64)->Result<(),String>{
    let get=|k:&str,d:f64,lo:f32,hi:f32|->Result<f32,String>{let v=p(pr,k,d,t);if !v.is_finite(){return Err(format!("primaries model 1: {k} must be finite"))}if v<lo||v>hi{return Err(format!("primaries model 1: {k} must be in {lo}..{hi}"))}Ok(v)};
    let (mut lift,mut gam,mut gain,mut off)=([0f32;4],[1f32;4],[1f32;4],[0f32;4]);
    for (j,c) in CH.iter().enumerate(){lift[j]=get(&format!("lift_{c}"),0.,-2.,2.)?;gam[j]=get(&format!("gamma_{c}"),1.,0.05,20.)?;gain[j]=get(&format!("gain_{c}"),1.,0.,16.)?;off[j]=get(&format!("offset_{c}"),0.,-2.,2.)?;}
    let ct=get("contrast",1.,0.,4.)?;let pv=get("pivot",0.435,0.,1.)?;let sat=get("saturation",1.,0.,4.)?;let sp=get("space",0.,0.,5.)?;
    if sp.fract()!=0.{return Err("primaries model 1: space must be an integer 0..5".into())}let sp=sp as u32;
    for k in ["shadows","highlights","temperature","tint"]{if p(pr,k,0.,t)!=0.{return Err("primaries model 1: shadows/highlights/temperature/tint are model 0 only; use their own effects".into())}}
    let nz:[bool;4]=[0usize,1,2,3].map(|j|neutral(lift[j],gam[j],gain[j],off[j]));
    if nz.iter().all(|v|*v)&&ct==1.&&sat==1.{return Ok(())}
    super::frame::validate(f)?;
    let slope=[0usize,1,2,3].map(|j|gain[j]-lift[j]);let ofs=[0usize,1,2,3].map(|j|lift[j]+off[j]);let pw=[0usize,1,2,3].map(|j|1.0/gam[j]);
    for(i,px) in f.d.chunks_exact_mut(4).enumerate(){if i%65536==0{crate::guard::check()}
        let mut c=[px[0],px[1],px[2]];if sp!=0{for v in c.iter_mut(){*v=super::from_lin(sp,*v)}}
        for j in 0..3{if nz[j]{continue}let mut y=slope[j]*c[j]+ofs[j];if gam[j]!=1.{y=y.max(0.0).powf(pw[j])}c[j]=y}
        if !nz[3]{let y=0.2126*c[0]+0.7152*c[1]+0.0722*c[2];let mut y2=slope[3]*y+ofs[3];if gam[3]!=1.{y2=y2.max(0.0).powf(pw[3])}let d=y2-y;for v in c.iter_mut(){*v+=d}}
        if ct!=1.{for v in c.iter_mut(){*v=(*v-pv)*ct+pv}}
        if sat!=1.{let y=0.2126*c[0]+0.7152*c[1]+0.0722*c[2];for v in c.iter_mut(){*v=y+sat*(*v-y)}}
        if sp!=0{for v in c.iter_mut(){*v=super::to_lin(sp,*v)}}
        px[..3].copy_from_slice(&c);}
    Ok(())
}
pub fn cdl_equivalent(lift:[f64;3],gamma:[f64;3],gain:[f64;3],offset:[f64;3],saturation:f64)->serde_json::Value{
    let slope=[0usize,1,2].map(|j|gain[j]-lift[j]);let off=[0usize,1,2].map(|j|lift[j]+offset[j]);let power=[0usize,1,2].map(|j|1.0/gamma[j]);
    serde_json::json!({"slope":slope,"offset":off,"power":power,"saturation":saturation})
}
