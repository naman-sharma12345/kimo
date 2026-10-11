//! Scene-linear grading: exposed gains, signed contrast, luminance saturation.
use crate::effects::{p,Params,Frame};
pub fn apply(pr:&Params,f:&mut Frame,t:f64)->Result<(),String>{super::frame::validate(f)?;
    let g=|k:&str,d:f64|p(pr,k,d,t) as f64;let gamut=g("gamut",0.);if gamut.fract()!=0.||!(0. ..=7.).contains(&gamut){return Err("linear grade gamut 0..7 required".into())}
    let exposure=g("exposure",0.);let contrast=g("contrast",1.);let pivot=g("pivot",0.18);let saturation=g("saturation",1.);let offset=g("offset",0.);let gains=[g("gain_r",1.),g("gain_g",1.),g("gain_b",1.)];
    if !exposure.is_finite()||exposure.abs()>20.||!(0.01..=10.).contains(&contrast)||!(1e-5..=16.).contains(&pivot)||!(0. ..=10.).contains(&saturation)||!offset.is_finite()||offset.abs()>16.||gains.iter().any(|x|!x.is_finite()||*x<0.||*x>100.){return Err("linear grade parameters outside bounded ranges".into())}
    let weights=super::managed::luma_weights(gamut as usize);let ex=2f64.powf(exposure);
    // Preserve legacy first-overflow partial-write behavior for extreme parameter/input cases.
    if contrast>2.||exposure.abs()>2.||saturation>2.||offset.abs()>1.||gains.iter().any(|g|*g>2.)||f.d.chunks_exact(4).any(|p|p[..3].iter().any(|v|v.abs()>16.)){
    for(i,px) in f.d.chunks_exact_mut(4).enumerate(){if i%65536==0{crate::guard::check()}let mut c=[0.;3];for j in 0..3{let v=px[j] as f64*ex*gains[j]+offset;c[j]=pivot*(v.abs()/pivot).powf(contrast).copysign(v)}let y=(0..3).map(|j|c[j]*weights[j]).sum::<f64>();for j in 0..3{let v=y+(c[j]-y)*saturation;if !v.is_finite()||v.abs()>f32::MAX as f64{return Err("linear grade overflow".into())}px[j]=v as f32}}
        return Ok(())
    }
    let failed=std::sync::atomic::AtomicBool::new(false);let ptr=crate::par::SyncPtr(f.d.as_mut_ptr());
    crate::par::ranges(f.w*f.h,32768,|a,b|{let ptr=ptr;for i in a..b{if i%65536==0{crate::guard::check()}let px=unsafe{std::slice::from_raw_parts_mut(ptr.0.add(i*4),4)};
let mut c=[0.;3];for j in 0..3{let v=px[j] as f64*ex*gains[j]+offset;c[j]=pivot*(v.abs()/pivot).powf(contrast).copysign(v)}let y=(0..3).map(|j|c[j]*weights[j]).sum::<f64>();for j in 0..3{let v=y+(c[j]-y)*saturation;if !v.is_finite()||v.abs()>f32::MAX as f64{failed.store(true,std::sync::atomic::Ordering::Relaxed);continue}px[j]=v as f32}}});
    if failed.load(std::sync::atomic::Ordering::Relaxed){return Err("linear grade overflow".into())}
    Ok(())
}
pub fn catalog()->Vec<super::Entry>{vec![("linear_grade","Scene-linear gain/exposure + signed pivoted power contrast + gamut-aware luminance saturation. Preserves negative and HDR values. Input must already be linear; insert color_managed decode first, encode/output-map afterward.",&[("gamut",0.,"0 Rec709,1 Rec2020,2 P3,3 AP1,4 AP0,5 ARRI WG3,6 SGamut3,7 SGamut3Cine"),("exposure",0.,"stops +/-20"),("gain_r",1.,"0..100"),("gain_g",1.,"0..100"),("gain_b",1.,"0..100"),("offset",0.,"+/-16 linear"),("contrast",1.,"0.01..10, pivoted power"),("pivot",0.18,"linear middle grey"),("saturation",1.,"0..10")])]}
