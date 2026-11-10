//! Opt-in display-sRGB/Rec709 finishing. Not a calibrated film-stock model.
use crate::effects::{Frame, Params, p};
fn noise(x:usize,y:usize,seed:u64)->f32 {
    let mut v=seed ^ (x as u64).wrapping_mul(0x9e3779b97f4a7c15) ^ (y as u64).wrapping_mul(0xbf58476d1ce4e5b9);
    v=(v^(v>>30)).wrapping_mul(0xbf58476d1ce4e5b9);v=(v^(v>>27)).wrapping_mul(0x94d049bb133111eb);v^=v>>31;
    (v>>40) as f32/16777216.-0.5
}
fn smooth(v:f32)->f32{let v=v.clamp(0.,1.);v*v*(3.-2.*v)}
fn grain(x:usize,y:usize,size:f32,seed:u64)->f32{
    let (u,v)=(x as f32/size,y as f32/size);let(a,b)=(u.floor() as usize,v.floor() as usize);let(s,t)=(smooth(u-u.floor()),smooth(v-v.floor()));
    let lo=noise(a,b,seed)*(1.-s)+noise(a+1,b,seed)*s;let hi=noise(a,b+1,seed)*(1.-s)+noise(a+1,b+1,seed)*s;lo*(1.-t)+hi*t
}
pub fn apply(pr:&Params,f:&mut Frame,t:f64,seed:u64)->Result<(),String>{
    super::frame::validate(f)?;
    let get=|k:&str,d:f64,lo:f32,hi:f32|->Result<f32,String>{let v=p(pr,k,d,t);if !v.is_finite()||v<lo||v>hi{return Err(format!("invalid film_finish {k}"))}Ok(v)};
    let bloom=get("bloom",0.12,0.,1.)?;let halo=get("halation",0.08,0.,1.)?;let ga=get("grain",0.025,0.,0.25)?;let vig=get("vignette",0.12,0.,1.)?;
    let threshold=get("threshold",0.7,0.,1.)?;let knee=get("knee",0.15,0.001,1.)?;let radius=get("radius",0.018,0.0001,0.25)?;let size=get("grain_size",1.5,1.,8.)?;let soft=get("vignette_softness",0.6,0.05,1.)?;
    if bloom==0.&&halo==0.&&ga==0.&&vig==0.{return Ok(())}
    crate::guard::reserve(f.d.len().saturating_mul(std::mem::size_of::<f32>()));
    let mut out=f.d.clone();
    let needs_scatter=bloom>0.||halo>0.;
    let linear=if needs_scatter{crate::guard::reserve(f.d.len().saturating_mul(8));Some(f.d.chunks_exact(4).enumerate().map(|(i,c)|{if i%65536==0{crate::guard::check()}[super::fast_transfer::decode(c[0]),super::fast_transfer::decode(c[1]),super::fast_transfer::decode(c[2])]}).collect::<Vec<_>>())}else{None};
    let scattered=if needs_scatter{
        let mut b=Frame::new(f.w,f.h);
        for(i,c)in f.d.chunks_exact(4).enumerate(){if i%65536==0{crate::guard::check()}let l=0.2126*c[0]+0.7152*c[1]+0.0722*c[2];let mask=smooth((l-threshold)/knee);for j in 0..3{b.d[i*4+j]=(linear.as_ref().unwrap()[i][j].max(0.)*mask as f64*c[3] as f64).min(f32::MAX as f64) as f32;}b.d[i*4+3]=c[3];}
        let r=((f.w.min(f.h) as f32*radius).round() as usize).clamp(1,512);
        crate::guard::check();super::blur_v2_reference::blur(&mut b,r);crate::guard::check();Some(b)
    }else{None};
    crate::guard::reserve(f.w.saturating_mul(4));let xs:Vec<f32>=(0..f.w).map(|x|(x as f32+0.5)/f.w as f32*2.-1.).collect();
    for y in 0..f.h{crate::guard::check();let dy=(y as f32+0.5)/f.h as f32*2.-1.;for x in 0..f.w{
        let i=(y*f.w+x)*4;let c=&f.d[i..i+4];if c[3]==0.{continue}
        let l=(0.2126*c[0]+0.7152*c[1]+0.0722*c[2]).clamp(0.,1.);
        let bright=smooth((l-threshold)/knee);
        let dx=xs[x];let r=(dx*dx+dy*dy).sqrt()/std::f32::consts::SQRT_2;let fall=1.-vig*smooth((r-(1.-soft))/soft);
        // PRE-GRAIN extraction above; zero-mean monochrome grain is added LAST.
        let n=if ga>0.{grain(x,y,size,seed)*ga*(0.2+0.8*(4.*l*(1.-l)).sqrt())}else{0.};
        let halo_luma=scattered.as_ref().map(|b|0.2126*b.d[i]+0.7152*b.d[i+1]+0.0722*b.d[i+2]).unwrap_or(0.);
        for j in 0..3{
            let v=if let Some(b)=&scattered{let tint=[1.,0.22,0.04][j];let scatter=(b.d[i+j]*bloom+halo_luma*halo*tint*(1.-bright)) as f64;super::fast_transfer::encode(linear.as_ref().unwrap()[i/4][j]+scatter)}else{c[j] as f64};
            let v=v*fall as f64+n as f64;if !v.is_finite()||v.abs()>f32::MAX as f64{return Err("film_finish overflow".into())}out[i+j]=v as f32;
        }
    }}f.d=out;Ok(())
}
pub fn catalog()->Vec<super::Entry>{vec![("film_finish","Opt-in display-sRGB Rec709 finishing: linear-light highlight bloom/red rim, luma-shaped monochrome grain LAST, feathered vignette. Existing effect wrapper required for managed linear projects. NOT calibrated film stock/HDR. Seed is outer timeline frame; nested precomp has local seed.",&[("bloom",0.12,"0..1"),("halation",0.08,"0..1"),("grain",0.025,"0..0.25"),("vignette",0.12,"0..1"),("threshold",0.7,"display luma 0..1"),("knee",0.15,"display luma 0.001..1"),("radius",0.018,"fraction of shorter frame edge 0.0001..0.25, bounded512px"),("grain_size",1.5,"output pixels1..8"),("vignette_softness",0.6,"0.05..1")])]}
