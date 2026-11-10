//! f32 sRGB paths for finite decode |x|<=1, encode |x|<=16; reference-f64 fallback outside tested domain.
pub fn decode(x:f32)->f64{if x.abs()>1.{return super::managed::decode(1,x as f64,100.)}if x.abs()<=0.04045{(x/12.92) as f64}else{((x.abs()+0.055)/1.055).powf(2.4).copysign(x) as f64}}
pub fn encode(x:f64)->f64{if x.abs()>16.{return super::managed::encode(1,x,100.)}let x=x as f32;if x.abs()<=0.0031308{(x*12.92) as f64}else{(1.055*x.abs().powf(1./2.4)-0.055).copysign(x) as f64}}
