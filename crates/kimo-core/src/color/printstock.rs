//! Named print-stock looks driven by a smooth density-response model (opt-in effect `print_stock`).
//!
//! WHAT THIS IS: a parametric characteristic curve D(logE) per channel (logistic S-curve in density vs
//! log exposure), viewed through projection (transmittance 10^-D, display-encoded by a fixed 1/2.6 power).
//! WHAT IS PUBLISHED / ANCHORED: the Status A laboratory aim densities (R,G,B = 1.09,1.06,1.03) for
//! KODAK VISION Color Print Film 2383/3383 and VISION Premier 2393 (Kodak H-1-2383 data sheet, text of
//! "The Status A density aim is: R G B Density 1.09 1.06 1.03"). The curve is solved so a neutral mid-tone
//! (input == `pivot`) lands exactly on that aim density per channel.
//! WHAT IS NOT PUBLISHED HERE: the data-sheet sensitometric curves are graphs; they were NOT digitised.
//! Dmin, Dmax, gamma (slope) and the negative-to-print coupling `SPAN` are APPROXIMATE representative
//! values chosen by the author (2383: gamma 2.8, Dmax ~3.9; 2393: gamma 3.0, Dmax ~4.1; "low contrast
//! generic": gamma 2.2, Dmax 3.4). Per-channel gamma/Dmax differences, dye crosstalk, gamut and
//! halation are NOT modelled. These are "-style" looks, not calibrated emulations of a real emulsion
//! or process, and are not a substitute for a measured LUT.
//! Input is treated as display-encoded RGB like `film_print`. Alpha is never modified; alpha==0 pixels
//! are left untouched. amount==0 returns before any arithmetic (bit-exact identity).
use crate::effects::{Frame,Params,p};
pub const SPAN:f64=1.6;           // log10 print exposure per unit input (assumed negative-to-print coupling)
const PROJ_GAMMA:f64=2.6;
struct Stock{name:&'static str,dmin:f64,dmax:f64,gamma:f64,aim:[f64;3]}
const STOCKS:[Stock;3]=[
 Stock{name:"print_2383_style",dmin:0.06,dmax:3.9,gamma:2.8,aim:[1.09,1.06,1.03]},
 Stock{name:"print_2393_style",dmin:0.06,dmax:4.1,gamma:3.0,aim:[1.09,1.06,1.03]},
 Stock{name:"print_soft_generic",dmin:0.06,dmax:3.4,gamma:2.2,aim:[1.06,1.06,1.06]},
];
/// Density of channel `c` for print exposure `e` (log10 units, positive = more light = denser).
fn density(s:&Stock,c:usize,e:f64)->f64{
    let dr=s.dmax-s.dmin; let k=4.*s.gamma/dr;
    let sa=((s.aim[c]-s.dmin)/dr).clamp(1e-6,1.-1e-6); let shift=(sa/(1.-sa)).ln()/k; // D(e=0)=aim
    s.dmin+dr/(1.+(-k*(e+shift)).exp())
}
/// Display value for input `v` (encoded), channel c, pivot, print-light points (1 pt = 0.025 logE).
pub fn curve(stock:usize,c:usize,v:f64,pivot:f64,points:f64)->f64{
    let s=&STOCKS[stock]; let e=-(v-pivot)*SPAN+points*0.025; let d=density(s,c,e);
    let t=10f64.powf(-(d-s.dmin)); t.powf(1./PROJ_GAMMA)
}
pub fn apply(pr:&Params,f:&mut Frame,t:f64)->Result<(),String>{
    super::frame::validate(f)?;
    let stock=p(pr,"stock",0.,t) as f64; let amount=p(pr,"amount",1.,t) as f64;
    let pivot=p(pr,"pivot",0.435,t) as f64; let pts=p(pr,"print_light",0.,t) as f64;
    if stock.fract()!=0.||!(0. ..=(STOCKS.len()-1) as f64).contains(&stock)||!amount.is_finite()||!(0. ..=1.).contains(&amount)||!pivot.is_finite()||!(0.05..=0.95).contains(&pivot)||!pts.is_finite()||pts.abs()>12.{
        return Err("invalid print_stock params (stock 0..2 integer, amount 0..1, pivot 0.05..0.95, print_light -12..12)".into())}
    if amount==0.{return Ok(())}
    let si=stock as usize;let s=&STOCKS[si];let dr=s.dmax-s.dmin;let k=4.*s.gamma/dr;
    let shifts:[f64;3]=std::array::from_fn(|c|{let sa=((s.aim[c]-s.dmin)/dr).clamp(1e-6,1.-1e-6);(sa/(1.-sa)).ln()/k});
    let projection=-std::f64::consts::LN_10/PROJ_GAMMA;
    for(i,px) in f.d.chunks_exact_mut(4).enumerate(){
        if i%65536==0{crate::guard::check()}
        if px[3]==0.{continue}
        for c in 0..3{ let v=px[c] as f64; let e=-(v-pivot)*SPAN+pts*0.025;let density_delta=dr/(1.+(-k*(e+shifts[c])).exp());let o=(projection*density_delta).exp(); px[c]=(v+(o-v)*amount) as f32; }
    }
    Ok(())
}
pub fn catalog()->Vec<super::Entry>{vec![("print_stock","Opt-in print-stock-STYLE look: smooth density-vs-logE S-curve (logistic) per channel, anchored to the published Kodak Status A aim densities 1.09/1.06/1.03 at mid-tone; Dmin/Dmax/gamma are approximate, NOT digitised data-sheet curves. stock 0=2383-style 1=2393-style 2=soft generic. Alpha untouched; amount 0 = exact identity.",&[("stock",0.,"0..2 integer"),("pivot",0.435,"0.05..0.95 input value mapped to aim density"),("print_light",0.,"-12..12 printer points, + = darker (0.025 logE each)"),("amount",1.,"0..1")])]}
#[cfg(test)]mod tests{use super::*;use crate::keyframe::Prop;
 fn pr(s:f64,a:f64)->Params{let mut m=Params::new();m.insert("stock".into(),Prop::new(s));m.insert("amount".into(),Prop::new(a));m}
 fn ramp(n:usize)->Frame{let mut f=Frame::new(n,1);for i in 0..n{let v=i as f32/(n-1) as f32;f.d[i*4..i*4+4].copy_from_slice(&[v,(v*0.7+0.1).min(1.),1.-v,1.]);}f}
 #[test]fn identity_amount_zero_bit_exact(){for s in 0..3{let mut f=ramp(257);let o=f.d.clone();apply(&pr(s as f64,0.),&mut f,0.).unwrap();assert_eq!(f.d,o);}}
 #[test]fn deterministic(){let mut a=ramp(300);let mut b=ramp(300);apply(&pr(1.,1.),&mut a,0.).unwrap();apply(&pr(1.,1.),&mut b,0.).unwrap();assert_eq!(a.d,b.d);}
 #[test]fn aim_density_at_pivot(){for (si,s) in STOCKS.iter().enumerate(){for c in 0..3{let d=density(s,c,0.);assert!((d-s.aim[c]).abs()<1e-9,"{si} {c} {d}");
   let o=curve(si,c,0.435,0.435,0.);let exp=10f64.powf(-(s.aim[c]-s.dmin)/PROJ_GAMMA);assert!((o-exp).abs()<1e-9);}}}
 #[test]fn monotonic_and_bounded(){for si in 0..3{for c in 0..3{for pts in [-12.,0.,7.,12.]{let mut prev=-1.;for i in 0..=5000{let v=-1.+5.*i as f64/5000.;let o=curve(si,c,v,0.435,pts);assert!(o.is_finite()&&(0. ..=1.).contains(&o));assert!(o>=prev,"{si}{c}{pts}{v}");prev=o;}}}}}
 #[test]fn strictly_increasing_in_working_range(){for si in 0..3{let mut prev=-1.;for i in 0..=1000{let o=curve(si,1,i as f64/1000.,0.435,0.);assert!(o>prev);prev=o;}}}
 #[test]fn alpha_untouched_and_transparent_skipped(){let mut f=Frame::new(2,1);f.d=vec![0.3,0.5,0.7,0.0, 0.3,0.5,0.7,0.4];apply(&pr(0.,1.),&mut f,0.).unwrap();assert_eq!(&f.d[0..4],&[0.3,0.5,0.7,0.0]);assert_eq!(f.d[7],0.4);assert!(f.d[4]!=0.3);}
 #[test]fn hostile_params(){for (s,a) in [(f64::NAN,1.),(3.,1.),(-1.,1.),(0.5,1.),(0.,2.),(0.,-0.1),(0.,f64::INFINITY),(0.,f64::NAN)]{let mut f=ramp(8);assert!(apply(&pr(s,a),&mut f,0.).is_err(),"{s} {a}");}
   let mut m=pr(0.,1.);m.insert("pivot".into(),Prop::new(0.));assert!(apply(&m,&mut ramp(4),0.).is_err());let mut m=pr(0.,1.);m.insert("print_light".into(),Prop::new(1e30));assert!(apply(&m,&mut ramp(4),0.).is_err());}
 #[test]fn hostile_pixels(){let mut f=Frame::new(1,1);f.d=vec![f32::NAN,0.,0.,1.];assert!(apply(&pr(0.,1.),&mut f,0.).is_err());let mut f=Frame::new(1,1);f.d=vec![1e30,-1e30,f32::MAX,1.];apply(&pr(0.,1.),&mut f,0.).unwrap();assert!(f.d.iter().all(|v|v.is_finite()));let mut f=Frame::new(1,1);f.d=vec![0.,0.,0.,2.];assert!(apply(&pr(0.,1.),&mut f,0.).is_err());}
 #[test]fn partial_amount_blends(){let mut a=ramp(9);let mut b=Frame{w:a.w,h:a.h,d:a.d.clone()};apply(&pr(0.,1.),&mut a,0.).unwrap();apply(&pr(0.,0.5),&mut b,0.).unwrap();let o=ramp(9);for i in 0..9*4{if i%4==3{continue}let want=o.d[i]+(a.d[i]-o.d[i])*0.5;assert!((b.d[i]-want).abs()<1e-5);}}
}
