//! Test-only frozen 6ee2d7b reference for speed/error comparison.
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
    let si=stock as usize;
    for(i,px) in f.d.chunks_exact_mut(4).enumerate(){
        if i%65536==0{crate::guard::check()}
        if px[3]==0.{continue}
        for c in 0..3{ let v=px[c] as f64; let o=curve(si,c,v,pivot,pts); px[c]=(v+(o-v)*amount) as f32; }
    }
    Ok(())
}
