//! Opt-in per-effect domain declarations. Unknown effects are not guessed.
//! A declaration says which domain an effect expects, not a calibrated display view.
use crate::effects::{Frame,Params};
use std::collections::BTreeMap;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum WorkingDomain { Current, SceneLinear, DisplayReferred }
impl WorkingDomain {
    pub fn parse(v:&str)->Result<Self,String>{match v {
        "current"=>Ok(Self::Current), "scene-linear"=>Ok(Self::SceneLinear),
        "display-referred"=>Ok(Self::DisplayReferred),_=>Err("OCIO domain: current, scene-linear or display-referred".into())}}
}
#[cfg(feature="native-ocio")]
pub struct DomainProcessor { forward:Option<super::ocio_config::Processor>, reverse:Option<super::ocio_config::Processor> }
#[cfg(feature="native-ocio")]
impl DomainProcessor {
    pub fn new(config:&super::ocio_config::Config,current:&str,linear:&str,display:&str,domain:WorkingDomain)->Result<Self,String>{
        let target=match domain{WorkingDomain::Current=>current,WorkingDomain::SceneLinear=>linear,WorkingDomain::DisplayReferred=>display};
        // Compile both directions before touching a frame, including same-name validation.
        let forward=config.processor(current,target)?;let reverse=config.processor(target,current)?;
        if forward.is_noop() && reverse.is_noop(){Ok(Self{forward:None,reverse:None})}
        else{Ok(Self{forward:Some(forward),reverse:Some(reverse)})}
    }
    pub fn apply(&self,kind:&str,params:&Params,strings:&BTreeMap<String,String>,f:&mut Frame,t:f64,seed:u64)->Result<(),String>{
        if strings.contains_key("wrapper_working_gamut"){return Err("cannot combine OCIO and legacy domain wrappers".into())}
        super::frame::validate(f)?;crate::guard::reserve(f.d.len().checked_mul(4).ok_or("OCIO effect allocation overflow")?);
        let mut temp=Frame{w:f.w,h:f.h,d:f.d.clone()};
        if let Some(p)=&self.forward{p.apply(&mut temp)?}
        let mut strings=strings.clone(); strings.remove("ocio_domain");
        crate::effects::apply(kind,params,&strings,&mut temp,t,seed)?;
        if let Some(p)=&self.reverse{p.apply(&mut temp)?}
        super::frame::validate(&temp)?;*f=temp;Ok(())
    }
}
/// Wiring for an effect with explicit strings: ocio_domain, ocio_config,
/// ocio_working_space, ocio_linear_space, ocio_display_space. Nothing inferred.
pub fn apply(kind:&str,params:&Params,strings:&BTreeMap<String,String>,f:&mut Frame,t:f64,seed:u64)->Result<(),String>{
    let domain=WorkingDomain::parse(strings.get("ocio_domain").ok_or("missing OCIO domain")?)?;
    if strings.contains_key("wrapper_working_gamut"){return Err("cannot combine OCIO and legacy domain wrappers".into())}
    #[cfg(feature="native-ocio")]{
        let get=|k:&str| strings.get(k).map(String::as_str).ok_or_else(||format!("missing {k}"));
        let config=super::ocio_config::Config::from_file(get("ocio_config")?)?;
        DomainProcessor::new(&config,get("ocio_working_space")?,get("ocio_linear_space")?,get("ocio_display_space")?,domain)?.apply(kind,params,strings,f,t,seed)
    }
    #[cfg(not(feature="native-ocio"))]{let _=(domain,kind,params,f,t,seed);Err("OCIO domain effects require native-ocio feature and OCIO 2 library".into())}
}
