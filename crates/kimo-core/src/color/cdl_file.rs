use crate::effects::Frame;use std::collections::BTreeMap;
pub fn apply(strings:&BTreeMap<String,String>,f:&mut Frame)->Result<(),String>{super::frame::validate(f)?;
#[cfg(not(feature="native-ocio"))]{let _=strings;Err("nativeASC CDL requiresnative-ocio+OCIO2".into())}
#[cfg(feature="native-ocio")]{let c:super::cdl_xml::Correction=serde_json::from_str(strings.get("cdl_payload").ok_or("missingCDLpayload")?).map_err(|e|e.to_string())?;let p=super::ocio_config::Processor::cdl(&c)?;crate::guard::reserve(f.d.len().checked_mul(4).ok_or("CDLallocationoverflow")?);let mut out=Frame{w:f.w,h:f.h,d:f.d.clone()};if let Some(path)=strings.get("config_path"){let config=super::ocio_config::Config::from_file(path)?;let linear=strings.get("linear_space").ok_or("linearspacerequired")?;let cct=strings.get("cct_space").ok_or("cctspacerequired")?;config.processor(linear,cct)?.apply(&mut out)?;p.apply(&mut out)?;config.processor(cct,linear)?.apply(&mut out)?;}else{p.apply(&mut out)?;}f.d=out.d;Ok(())}
}
