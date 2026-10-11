use super::kit::*;
pub fn commands()->Vec<Command>{vec![
Command{name:"set_ocio_view_delivery",description:"Store an explicit native OCIO view video selection. Caller declares input scene-linear gamut and actual view output gamut/transfer independently of view names. Validates config/pair; ordinary export remains unchanged.",mutates:true,schema:obj(vec![("selection",json!({"type":"object","properties":{"config_path":{"type":"string"},"source":{"type":"string"},"display":{"type":"string"},"view":{"type":"string"},"input_gamut":{"type":"integer","minimum":0,"maximum":7},"output_gamut":{"type":"integer","minimum":0,"maximum":1},"output_transfer":{"type":"integer","enum":[0,1,6,7,8]}},"required":["config_path","source","display","view","input_gamut","output_gamut","output_transfer"],"additionalProperties":false}))],&["selection"]),run:|ss,a|{
#[cfg(not(feature="native-ocio"))]{let _=(ss,a);Err("requires native-ocio feature and OCIO2.5.1".into())}
#[cfg(feature="native-ocio")]{let s:crate::color::view_video::Selection=serde_json::from_value(a.get("selection").ok_or("selection required")?.clone()).map_err(|e|e.to_string())?;s.processor(&ss.project)?;ss.project.ocio_view_delivery=Some(s);Ok(json!({"selection":ss.project.ocio_view_delivery,"ordinary_export_unchanged":true}))}
}},
Command{name:"export_ocio_view_video",description:"Encode timeline through stored explicit native OCIO2.5.1 display/view including looks, bypass configured project output, tag caller-declared view output through strict delivery contract. HEVC10/FFV116, audio retained. No view-name inference or mastering inheritance.",mutates:false,schema:obj(vec![("path",s("video output")),("format",json!({"type":"string","enum":["hevc10","ffv1_16"]})),("crf",json!({"type":"integer","minimum":0,"maximum":51}))],&["path","format"]),run:|ss,a|{
#[cfg(not(feature="native-ocio"))]{let _=(ss,a);Err("requires native-ocio feature and OCIO2.5.1".into())}
#[cfg(feature="native-ocio")]{let crf=match a.get("crf"){None=>20,Some(v)=>v.as_u64().filter(|v|*v<=51).ok_or("integer crf0..51 required")? as u32};crate::color::view_video::export(&ss.project,str_arg(a,"path")?,str_arg(a,"format")?,crf)}
}}
]}
