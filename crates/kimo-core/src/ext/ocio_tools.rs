//! Opt-in OCIO inspection and sample transforms; auto-registered, no project schema edits.
use super::kit::*;
#[cfg(feature="native-ocio")]
use crate::color::ocio_config::Config;
pub fn commands()->Vec<Command>{vec![
    Command{name:"inspect_ocio_config",description:"Inspect a self-contained OCIO2 config: spaces and display/view names. Optional native-ocio feature, no global/project output changes; external LUT files rejected.",mutates:false,
        schema:obj(vec![("path",s("self-contained .ocio config path"))],&["path"]),run:|_,a|{
            #[cfg(feature="native-ocio")]{let c=Config::from_file(str_arg(a,"path")?)?;Ok(json!({"ocio_version":crate::color::ocio_config::version(),"colorspaces":c.colorspaces(),"views":c.views().into_iter().map(|v|json!({"display":v.display,"view":v.view})).collect::<Vec<_>>(),"external_assets":false,"production_view_selected":false}))}
            #[cfg(not(feature="native-ocio"))]{let _=a;Err("requires native-ocio feature and OCIO2 library".into())}
        }},
    Command{name:"transform_ocio_samples",description:"Apply named OCIO colorspace CPU transform to up to4096 RGBA samples, preserve alpha. Config is caller-selected, not a project output view or certification.",mutates:false,
        schema:obj(vec![("path",s("self-contained .ocio path")),("source",s("source colorspace name")),("destination",s("destination colorspace name")),("rgba",json!({"type":"array","items":{"type":"number"},"description":"flat RGBA array; finite RGB, alpha0..1"}))],&["path","source","destination","rgba"]),run:|_,a|{
            #[cfg(feature="native-ocio")]{let v=a.get("rgba").and_then(Value::as_array).ok_or("RGBA array required")?;if v.is_empty()||v.len()%4!=0||v.len()>4096*4{return Err("need 1..4096 RGBA samples".into())}let d=v.iter().map(|v|v.as_f64().filter(|n|n.is_finite()).map(|n|n as f32).ok_or("finite RGBA numbers required".to_string())).collect::<Result<Vec<_>,_>>()?;let mut f=crate::effects::Frame{w:d.len()/4,h:1,d};crate::color::frame::validate(&f)?;let c=Config::from_file(str_arg(a,"path")?)?;let p=c.processor(str_arg(a,"source")?,str_arg(a,"destination")?)?;p.apply(&mut f)?;Ok(json!({"rgba":f.d,"noop":p.is_noop(),"ocio_version":crate::color::ocio_config::version()}))}
            #[cfg(not(feature="native-ocio"))]{let _=a;Err("requires native-ocio feature and OCIO2 library".into())}
        }},
]}
