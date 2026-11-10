//! Explicit film-style response, not a calibrated stock or an ACES output transform.
use crate::effects::{Frame,Params,p};
/// Identity below knee, C1 soft shoulder above knee, asymptote at peak.
pub fn shoulder(x:f64,knee:f64,peak:f64)->f64{if x<=knee{x}else{knee+(peak-knee)*(-(-(x-knee)/(peak-knee)).exp_m1())}}
fn hash(mut v:u64)->u64{v=(v^(v>>30)).wrapping_mul(0xbf58476d1ce4e5b9);v=(v^(v>>27)).wrapping_mul(0x94d049bb133111eb);v^(v>>31)}
fn random(x:usize,y:usize,seed:u64,lane:u64)->f64{let v=hash(seed^(x as u64).wrapping_mul(0x9e3779b97f4a7c15)^(y as u64).wrapping_mul(0xbf58476d1ce4e5b9)^lane);(v>>11) as f64/9007199254740992.}
pub fn density(luma:f64,shadow:f64,highlight:f64,falloff:f64)->f64{highlight+(shadow-highlight)*(1.-luma.clamp(0.,1.)).powf(falloff)}
fn get(pr:&Params,t:f64,k:&str,d:f64,lo:f64,hi:f64)->Result<f64,String>{let v=p(pr,k,d,t) as f64;if !v.is_finite()||!(lo..=hi).contains(&v){Err(format!("invalid film response parameter {k}"))}else{Ok(v)}}
pub fn apply(kind:&str,pr:&Params,f:&mut Frame,t:f64,seed:u64)->Result<(),String>{super::frame::validate(f)?;
 let amount=get(pr,t,"amount",if kind=="film_soft_shoulder"{1.}else{0.025},0.,1.)?;
 if kind=="film_soft_shoulder"{let knee=get(pr,t,"knee",0.75,0.,1.)?;let peak=get(pr,t,"peak",1.,0.001,16.)?;if peak<=knee{return Err("film shoulder peak must exceed knee".into())}if amount==0.{return Ok(())}for(i,c)in f.d.chunks_exact_mut(4).enumerate(){if i%65536==0{crate::guard::check()}if c[3]==0.{continue}for v in &mut c[..3]{let x=*v as f64;*v=(x+amount*(shoulder(x,knee,peak)-x)) as f32;}}return Ok(())}
 if kind!="film_luma_grain"{return Err("unknown film response effect".into())}
 let shadow=get(pr,t,"shadow_density",0.9,0.,1.)?;let highlight=get(pr,t,"highlight_density",0.08,0.,1.)?;let falloff=get(pr,t,"falloff",1.5,0.1,8.)?;let size=get(pr,t,"grain_size",1.,1.,8.)?;if shadow<highlight{return Err("shadow_density must be >= highlight_density".into())}if amount==0.{return Ok(())}
 for y in 0..f.h{crate::guard::check();for x in 0..f.w{let c=&mut f.d[(y*f.w+x)*4..(y*f.w+x)*4+4];if c[3]==0.{continue}let l=0.2126*c[0] as f64+0.7152*c[1] as f64+0.0722*c[2] as f64;let d=density(l,shadow,highlight,falloff);let gx=(x as f64/size).floor() as usize;let gy=(y as f64/size).floor() as usize;
 // Independent occupancy and amplitude: expected mean is zero before output clipping.
 let n=if random(gx,gy,seed,0)<d{(random(gx,gy,seed,0xD1B54A32D192ED03)*2.-1.)*amount}else{0.};for v in &mut c[..3]{let out=*v as f64+n;if !out.is_finite()||out.abs()>f32::MAX as f64{return Err("film grain overflow".into())}*v=out as f32;}
 }}Ok(())
}
pub fn catalog()->Vec<super::Entry>{vec![
("film_soft_shoulder","Opt-in per-channel film-style exponential shoulder. Explicit current/display signal; identity below knee, C1 response, asymptote peak. No hard clip, hue preservation or measured stock claim. For managed pixels use explicit native OCIO space sandwich or scene-linear luminance_shoulder instead.",&[("knee",0.75,"signal value0..1"),("peak",1.,"asymptote>knee up to16"),("amount",1.,"0..1; partial amount does not bound HDR")]),
("film_luma_grain","Opt-in zero-mean monochrome grain in explicitly display-referred Rec709 RGB. Occupancy decreases monotonically from shadow to highlight density. Added last, no clipping; seed is timeline frame, nested precomp local seed. Native OCIO explicit display-space sandwich supported; not measured stock or temporal film scan.",&[("amount",0.025,"max additive signal amplitude0..1"),("shadow_density",0.9,"occupancy0..1 >= highlight"),("highlight_density",0.08,"occupancy0..1"),("falloff",1.5,"density curve exponent0.1..8"),("grain_size",1.,"block size output pixels1..8")])
]}
