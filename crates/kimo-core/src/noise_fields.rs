//! Independent, raw field operators. No imported algorithms, tables or code.
//! These build on unit-1's original fields; no state or implicit presets.
use crate::noise::{sample,Kind,Sample,COORD_LIMIT};

#[derive(Clone,Copy,Debug,PartialEq)]
pub struct Worley {pub f1:f64,pub f2:f64,pub gradient1:[f64;2],pub gradient2:[f64;2],pub cell1:[i64;2],pub cell2:[i64;2]}
fn feature(seed:u64,x:i64,y:i64,lane:u8)->f64 {
    // Own coordinate hash, explicit bytes. Independent lanes for cell feature axes.
    let mut h=0xcbf29ce484222325u64;
    for b in seed.to_le_bytes().into_iter().chain(x.to_le_bytes()).chain(y.to_le_bytes()).chain([lane]) {h^=u64::from(b);h=h.wrapping_mul(0x100000001b3);}
    h^=h.rotate_left(21);h^=h>>31;
    ((h>>11) as f64+0.5)/9007199254740992.0
}
fn coords(x:f64,y:f64)->Result<(),String>{if !x.is_finite()||!y.is_finite()||x.abs()>COORD_LIMIT||y.abs()>COORD_LIMIT {Err("field coordinates must be finite and within +/-1e9".into())}else{Ok(())}}
/// One independent feature per unit cell, Euclidean distances, F1/F2 not normalized.
/// Radius 3 is sufficient: two feature cells give F2<=sqrt(5)<3, while excluded
/// cells have coordinate distance>=3. Cell IDs are exact lattice pairs, not floats.
pub fn worley(seed:u64,x:f64,y:f64)->Result<Worley,String>{
    coords(x,y)?;let (ix,iy)=(x.floor() as i64,y.floor() as i64);
    let mut nearest=[(f64::INFINITY,[0i64;2],[0.0;2]);2];
    for dy in -3..=3 {for dx in -3..=3 {let cell=[ix+dx,iy+dy];
        let r=[x-(cell[0] as f64+feature(seed,cell[0],cell[1],0)),y-(cell[1] as f64+feature(seed,cell[0],cell[1],1))];
        let d=r[0]*r[0]+r[1]*r[1];let candidate=(d,cell,r);
        // Lexical tie-break makes cell identity deterministic on Voronoi boundaries.
        let before=|a:&(f64,[i64;2],[f64;2]),b:&(f64,[i64;2],[f64;2])|a.0<b.0||(a.0==b.0&&a.1<b.1);
        if before(&candidate,&nearest[0]){nearest[1]=nearest[0];nearest[0]=candidate;}else if before(&candidate,&nearest[1]){nearest[1]=candidate;}
    }}
    let distance=|n:(f64,[i64;2],[f64;2])|{let d=n.0.sqrt();(d,if d>0.0{[n.2[0]/d,n.2[1]/d]}else{[0.0;2]})};
    let(a,ga)=distance(nearest[0]);let(b,gb)=distance(nearest[1]);
    Ok(Worley{f1:a,f2:b,gradient1:ga,gradient2:gb,cell1:nearest[0].1,cell2:nearest[1].1})
}
#[derive(Clone,Copy,Debug)]
pub struct Fbm {pub kind:Kind,pub seed:u64,pub octaves:u32,pub lacunarity:f64,pub gain:f64,pub normalize:bool}
impl Fbm {
    pub fn validate(self)->Result<(),String>{
        if self.octaves==0||self.octaves>16{return Err("octaves must be 1..16 (work limit)".into());}
        if !self.lacunarity.is_finite()||self.lacunarity<=0.0||!self.gain.is_finite(){return Err("lacunarity must be positive finite; gain finite".into());}Ok(())
    }
    pub fn sample(self,x:f64,y:f64)->Result<Sample,String>{
        self.validate()?;coords(x,y)?;let(mut amplitude,mut frequency,mut total)=(1.0f64,1.0f64,0.0f64);
        let mut out=Sample{value:0.0,gradient:[0.0;2]};
        for o in 0..self.octaves {let s=sample(self.kind,self.seed.wrapping_add(u64::from(o)),x*frequency,y*frequency)?;
            out.value+=amplitude*s.value;for j in 0..2{out.gradient[j]+=amplitude*frequency*s.gradient[j];}
            total+=amplitude.abs();
            if o+1<self.octaves {amplitude*=self.gain;frequency*=self.lacunarity;}
        }
        if self.normalize {out.value/=total;for j in 0..2{out.gradient[j]/=total;}}
        if !out.value.is_finite()||out.gradient.iter().any(|v|!v.is_finite()){return Err("FBM amplitude/frequency/output overflow".into());}Ok(out)
    }
}
#[derive(Clone,Copy,Debug)]
pub struct Warp {pub source:Kind,pub source_seed:u64,pub warp:Kind,pub warp_seed:u64,pub frequency:f64,pub amplitude:f64}
impl Warp {
    /// q=p+amplitude*[W(seed,f*p),W(seed+1,f*p)]. Analytic chain rule included.
    pub fn sample(self,x:f64,y:f64)->Result<Sample,String>{
        coords(x,y)?;if !self.frequency.is_finite()||!self.amplitude.is_finite(){return Err("warp frequency/amplitude must be finite".into());}
        let a=sample(self.warp,self.warp_seed,x*self.frequency,y*self.frequency)?;
        let b=sample(self.warp,self.warp_seed.wrapping_add(1),x*self.frequency,y*self.frequency)?;
        let s=sample(self.source,self.source_seed,x+self.amplitude*a.value,y+self.amplitude*b.value)?;
        let f=self.amplitude*self.frequency;
        let out=Sample{value:s.value,gradient:[s.gradient[0]*(1.0+f*a.gradient[0])+s.gradient[1]*f*b.gradient[0],s.gradient[0]*f*a.gradient[1]+s.gradient[1]*(1.0+f*b.gradient[1])]};
        if out.gradient.iter().any(|v|!v.is_finite()){return Err("warp gradient overflow".into());}Ok(out)
    }
}
/// Curl of scalar potential psi(p)=base(seed,frequency*p)*amplitude.
/// Constant amplitude only: spatially varying gain can invalidate zero divergence.
pub fn curl(kind:Kind,seed:u64,x:f64,y:f64,frequency:f64,amplitude:f64)->Result<[f64;2],String>{
    coords(x,y)?;if !frequency.is_finite()||!amplitude.is_finite(){return Err("curl frequency/amplitude must be finite".into());}
    let s=sample(kind,seed,x*frequency,y*frequency)?;let k=frequency*amplitude;
    let v=[k*s.gradient[1],-k*s.gradient[0]];if v.iter().any(|x|!x.is_finite()){return Err("curl output overflow".into());}Ok(v)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn worley_matches_large_neighborhood_oracle(){for seed in [0,17,u64::MAX]{for n in 0..55{let x=n as f64*0.31-8.1;let y=n as f64*0.17-4.7;let got=worley(seed,x,y).unwrap();
        let mut all=Vec::new();for j in -8..=8{for i in -8..=8{let cx=x.floor() as i64+i;let cy=y.floor() as i64+j;let dx=x-cx as f64-feature(seed,cx,cy,0);let dy=y-cy as f64-feature(seed,cx,cy,1);all.push((dx.hypot(dy),[cx,cy]));}}
        all.sort_by(|a,b|a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));assert!((got.f1-all[0].0).abs()<1e-12);assert!((got.f2-all[1].0).abs()<1e-12);assert_eq!(got.cell1,all[0].1);assert_eq!(got.cell2,all[1].1);assert!(got.f1<=got.f2&&got.f2<=5.0f64.sqrt()+1e-12);
    }}}
    #[test]fn worley_feature_value_is_zero(){let x=feature(5,0,0,0);let y=feature(5,0,0,1);let v=worley(5,x,y).unwrap();assert_eq!(v.f1,0.0);assert_eq!(v.gradient1,[0.0;2]);assert_eq!(v.cell1,[0,0]);}
}

/// Public raw operator config. Each variant is explicitly selected, never bundled.
#[derive(Clone,Copy,Debug)]
pub enum Field {
    Worley{seed:u64,channel:u8},
    Fbm(Fbm), Warp(Warp),
    Curl{kind:Kind,seed:u64,frequency:f64,amplitude:f64,channel:u8},
}
impl Field {
    pub fn values(self,x:f64,y:f64)->Result<([f64;2],Option<[f64;2]>),String>{
        match self {
            Self::Worley{seed,channel}=>{let w=worley(seed,x,y)?;match channel{0=>Ok(([w.f1,w.f2],Some(w.gradient1))),1=>Ok(([w.f2,w.f1],Some(w.gradient2))),_=>Err("worley channel must be 0 (F1) or 1 (F2)".into())}}
            Self::Fbm(f)=>{let s=f.sample(x,y)?;Ok(([s.value,0.0],Some(s.gradient)))}
            Self::Warp(w)=>{let s=w.sample(x,y)?;Ok(([s.value,0.0],Some(s.gradient)))}
            Self::Curl{kind,seed,frequency,amplitude,channel}=>{if channel>1{return Err("curl channel must be 0 or 1".into());}let v=curl(kind,seed,x,y,frequency,amplitude)?;Ok(([v[channel as usize],v[1-channel as usize]],None))}
        }
    }
}
pub fn parse(a:&serde_json::Value)->Result<Field,String>{
    let kind=|key:&str,default:&str|Kind::parse(a.get(key).and_then(|v|v.as_str()).unwrap_or(default));
    let number=|key:&str,d:f64|->Result<f64,String>{match a.get(key){None=>Ok(d),Some(v)=>v.as_f64().filter(|v|v.is_finite()).ok_or_else(||format!("{key} must be finite numeric"))}};
    let integer=|key:&str,d:u64|->Result<u64,String>{match a.get(key){None=>Ok(d),Some(v)=>v.as_u64().ok_or_else(||format!("{key} must be u64 integer"))}};
    let seed=integer("seed",0)?;
    let channel=integer("channel",0)?;if channel>1{return Err("channel must be 0 or 1".into());}
    match a.get("op").and_then(|v|v.as_str()).ok_or("op required: worley|fbm|warp|curl")? {
        "worley"=>Ok(Field::Worley{seed,channel:channel as u8}),
        "fbm"=>{let octaves=integer("octaves",1)?;if !(1..=16).contains(&octaves){return Err("octaves must be 1..16 (work limit)".into());}
            let normalize=match a.get("normalize"){None=>false,Some(v)=>v.as_bool().ok_or("normalize must be boolean")?};
            let f=Fbm{kind:kind("kind","perlin")?,seed,octaves:octaves as u32,lacunarity:number("lacunarity",2.0)?,gain:number("octave_gain",0.5)?,normalize};f.validate()?;Ok(Field::Fbm(f))}
        "warp"=>Ok(Field::Warp(Warp{source:kind("kind","perlin")?,source_seed:seed,warp:kind("warp_kind","perlin")?,warp_seed:integer("warp_seed",1)?,frequency:number("warp_frequency",1.0)?,amplitude:number("warp_amplitude",0.0)?})),
        "curl"=>Ok(Field::Curl{kind:kind("kind","perlin")?,seed,frequency:number("curl_frequency",1.0)?,amplitude:number("curl_amplitude",1.0)?,channel:channel as u8}),
        op=>Err(format!("op must be worley|fbm|warp|curl, got '{op}'"))
    }
}
pub fn apply(params:&crate::effects::Params,strings:&std::collections::BTreeMap<String,String>,frame:&mut crate::effects::Frame,time:f64)->Result<(),String>{
    let text=strings.get("field").ok_or("field_operator needs strings.field JSON config")?;
    if text.len()>8192{return Err("field config exceeds 8192 bytes".into());}
    let a:serde_json::Value=serde_json::from_str(text).map_err(|e|format!("field config: {e}"))?;let field=parse(&a)?;
    let get=|k:&str,d:f64|params.get(k).map(|p|p.at(time)).unwrap_or(d);
    let(frequency,ox,oy,rx,ry,gain,bias,amount)=(get("frequency",8.0),get("offset_x",0.0),get("offset_y",0.0),get("rate_x",0.0),get("rate_y",0.0),get("gain",1.0),get("bias",0.0),get("amount",1.0));
    if [time,frequency,ox,oy,rx,ry,gain,bias,amount].iter().any(|v|!v.is_finite())||!(0.0..=1.0).contains(&amount){return Err("field effect values finite; amount 0..1".into());}
    if frame.w==0||frame.h==0||frame.d.len()!=frame.w.checked_mul(frame.h).and_then(|v|v.checked_mul(4)).ok_or("field dimensions overflow")?{return Err("field requires valid nonempty RGBA frame".into());}
    let lim=crate::resource_limits::current();let count=frame.w.checked_mul(frame.h).ok_or("field pixels overflow")?;
    // Maximum selected work is explicit: 49 cellular cells, 16 octaves, or 3 base fields.
    let cost=match field{Field::Worley{..}=>1024,Field::Fbm(f)=>u64::from(f.octaves)*256,Field::Warp(_)=>768,Field::Curl{..}=>256};
    if count as u64>lim.max_pixels||(count as u64).saturating_mul(cost)>lim.expr_max_pixel_ops.unwrap_or(u64::MAX){return Err("field exceeds max_pixels or expression work budget".into());}
    if amount==0.0{return Ok(());}
    crate::resource_limits::reserve(frame.d.len().checked_mul(4).ok_or("field output overflow")?)?;
    let mut output=crate::effects::Frame::try_new(frame.w,frame.h)?;output.d.copy_from_slice(&frame.d);
    let start=std::time::Instant::now();let h=frame.h as f64;
    for y in 0..frame.h{crate::guard::check();if lim.expr_timeout_ms.map(|t|start.elapsed().as_millis()>u128::from(t)).unwrap_or(false){return Err("field timed out".into());}
        for x in 0..frame.w{let (v,_)=field.values((x as f64+0.5)/h*frequency+ox+rx*time,(y as f64+0.5)/h*frequency+oy+ry*time)?;let mapped=v[0]*gain+bias;
            let i=(y*frame.w+x)*4;for c in 0..3{let val=(frame.d[i+c] as f64*(1.0-amount)+mapped*amount) as f32;if !val.is_finite(){return Err("field output f32 overflow".into());}output.d[i+c]=val;}}
    }frame.d=output.d;Ok(())
}
