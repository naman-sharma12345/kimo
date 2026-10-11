//! Explicit native view delivery, never selected by ordinary export.
use serde::{Serialize,Deserialize};
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {pub config_path:String,pub source:String,pub display:String,pub view:String,pub input_gamut:usize,pub output_gamut:usize,pub output_transfer:usize}
impl Selection {
 pub fn delivery(&self,p:&crate::model::Project)->Result<super::pipeline::Pipeline,String>{
 let q=&p.color_pipeline;q.validate()?;if !q.enabled||self.input_gamut!=q.working_gamut{return Err("view delivery requires enabled scene-linear project and input_gamut matching working_gamut; source space must represent that gamut/white".into())}
 let mut d=q.clone();d.output_gamut=self.output_gamut;d.output_transfer=self.output_transfer;d.mastering=None;d.validate()?;super::encode_contract::high(&d,"hevc10")?;Ok(d)
 }
 #[cfg(feature="native-ocio")]
 pub fn processor(&self,p:&crate::model::Project)->Result<super::ocio_config::Processor,String>{self.delivery(p)?;if super::ocio_config::version()!="2.5.1"{return Err("view video requires pinned native OCIO 2.5.1".into())}super::ocio_config::Config::from_file(&self.config_path)?.display_processor(&self.source,&self.display,&self.view)}
}
#[cfg(feature="native-ocio")]
pub fn export(p:&crate::model::Project,path:&str,format:&str,crf:u32)->Result<serde_json::Value,String>{
 let s=p.ocio_view_delivery.as_ref().ok_or("set explicit OCIO view delivery first")?;let d=s.delivery(p)?;let processor=s.processor(p)?;let mut excursions=0usize;
 let duration=super::pipeline::export_high_with(p,path,format,crf,&d,|f|{processor.apply(f)?;excursions+=f.d.chunks_exact(4).filter(|v|v[..3].iter().any(|v|*v<0.||*v>1.)).count();Ok(())})?;
 Ok(serde_json::json!({"path":path,"duration":duration,"selection":s,"delivery":super::encode_contract::high(&d,format)?,"ocio_version":super::ocio_config::version(),"configured_project_output_applied":false,"looks_bypassed":false,"out_of_range_pixels":excursions,"note":"Caller declares source gamut/white and actual view output encoding. Tags do not verify that declaration. Integer encoding clips0..1; no display/mastering certification. Stored view does not affect ordinary export. Existing project mastering metadata not inherited."}))
}
