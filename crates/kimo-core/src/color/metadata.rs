//! Caller-declared static HDR mastering metadata. Never inferred from footage.
use serde::{Serialize,Deserialize};
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]pub struct Mastering {
    pub red:[f64;2],pub green:[f64;2],pub blue:[f64;2],pub white:[f64;2],
    pub min_nits:f64,pub max_nits:f64,pub max_cll:u16,pub max_fall:u16,
}
impl Mastering{
    pub fn validate(&self)->Result<(),String>{
        for xy in [self.red,self.green,self.blue,self.white]{if xy.iter().any(|v|!v.is_finite()||*v<0.||*v>1.)||xy[0]+xy[1]>1.00001||xy[1]<=0.{return Err("mastering xy must be finite physical chromaticities".into())}}
        let area=(self.green[0]-self.red[0])*(self.blue[1]-self.red[1])-(self.green[1]-self.red[1])*(self.blue[0]-self.red[0]);if area.abs()<1e-5{return Err("degenerate mastering primaries".into())}
        // A declared RGB display white must be producible with nonnegative primary weights.
        let bx=(self.white[0]-self.red[0])*(self.blue[1]-self.red[1])-(self.white[1]-self.red[1])*(self.blue[0]-self.red[0]);let by=(self.green[0]-self.red[0])*(self.white[1]-self.red[1])-(self.green[1]-self.red[1])*(self.white[0]-self.red[0]);let weights=[1.-bx/area-by/area,bx/area,by/area];if weights.iter().any(|v|*v< -1e-8){return Err("mastering white must lie within primary triangle".into())}
        if !self.min_nits.is_finite()||!self.max_nits.is_finite()||self.min_nits<0.||self.max_nits<=self.min_nits||self.max_nits>10000.||self.max_cll>10000||self.max_fall>self.max_cll{return Err("invalid mastering luminance / MaxCLL / MaxFALL".into())}if (self.max_nits*10000.).round()<=(self.min_nits*10000.).round(){return Err("mastering min/max collapse at0.0001nit metadata precision".into())}Ok(())
    }
    pub fn x265(&self)->Result<String,String>{self.validate()?;let xy=|v:[f64;2]|format!("{},{}",(v[0]*50000.).round() as u32,(v[1]*50000.).round() as u32);Ok(format!("hdr10=1:master-display=G({})B({})R({})WP({})L({},{}):max-cll={},{}",xy(self.green),xy(self.blue),xy(self.red),xy(self.white),(self.max_nits*10000.).round() as u32,(self.min_nits*10000.).round() as u32,self.max_cll,self.max_fall))}
}
