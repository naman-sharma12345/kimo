//! Separate display-scope command; legacy render_scope and pipeline defaults unchanged.
use super::scope_image::{self, Encoding, Gamut, Kind, Options, Units};
use crate::commands::Command;
use serde_json::{json, Value};
fn string<'a>(a: &'a Value, k: &str) -> Result<&'a str, String> {
    a.get(k)
        .and_then(Value::as_str)
        .ok_or(format!("missing string {k}"))
}
fn number(a: &Value, k: &str, default: f64) -> Result<f64, String> {
    match a.get(k) {
        None => Ok(default),
        Some(v) => v
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or(format!("invalid number {k}")),
    }
}
fn flag(a: &Value, k: &str) -> Result<bool, String> {
    match a.get(k) {
        None => Ok(false),
        Some(v) => v.as_bool().ok_or(format!("{k} must be boolean")),
    }
}
fn integer(a: &Value, k: &str, lo: usize, hi: usize) -> Result<Option<usize>, String> {
    match a.get(k) {
        None => Ok(None),
        Some(v) => v
            .as_u64()
            .map(|n| n as usize)
            .filter(|n| (lo..=hi).contains(n))
            .map(Some)
            .ok_or(format!("{k} must be an integer {lo}..{hi}")),
    }
}
pub fn command() -> Command {
    Command{name:"render_display_scope",description:"Deterministic PNG scope bytes written to path: waveform, RGB parade, vectorscope or histogram. Uses configured output transform for managed projects (Rec709/Rec2020 only); legacy assumes encoded BT709. IRE is full-range signal, not YUV legal range. PQ absolute nits; HLG uses caller-declared peak/gamma OOTF. No calibrated mastering claim. Does not change project settings.",mutates:false,
        schema:json!({"type":"object","properties":{
            "time":{"type":"number"},"scope":{"type":"string","enum":["waveform","parade","vectorscope","histogram"]},
            "path":{"type":"string","description":"PNG destination"},"clip_id":{"type":"string"},
            "units":{"type":"string","enum":["ire","nits"],"description":"default nits for PQ/HLG/linear, otherwise ire; vectorscope always ire"},
            "ceiling":{"type":"number","description":"positive scale maximum <=10000, default100IRE/10000PQ/1000HLG/declared reference-white"},
            "hlg_peak_nits":{"type":"number","description":"100..10000 default1000, explicit display assumption"},
            "hlg_system_gamma":{"type":"number","description":"1..2 default1.2, explicit display assumption"},
            "data":{"type":"boolean","description":"default false; true adds exact integer counts (levels, waveform, parade, histogram, vectorscope grid with 100%/75% targets) as result.data; PNG still written"},
            "bins":{"type":"integer","description":"data level bins 16..4096 default256, spanning 0..ceiling"},
            "columns":{"type":"integer","description":"data waveform/parade columns 1..4096 default min(frame width,256)"},
            "grid":{"type":"integer","description":"data vectorscope grid 16..1024 default128"},
            "targets_75":{"type":"boolean","description":"default false; vectorscope PNG also marks 75% targets (smaller, dimmer boxes)"}},"required":["time","scope","path"]}),
        run:|ss,a| {
            let kind=match string(a,"scope")? {"waveform"=>Kind::Waveform,"parade"=>Kind::Parade,"vectorscope"=>Kind::Vectorscope,"histogram"=>Kind::Histogram,_=>return Err("unknown display scope".into())};
            let time=a.get("time").and_then(Value::as_f64).filter(|t|t.is_finite()&&*t>=0.).ok_or("finite nonnegative time required")?;
            let path=string(a,"path")?;if !path.to_ascii_lowercase().ends_with(".png"){return Err("scope destination must end in .png".into());}
            let clip=match a.get("clip_id") {None=>None,Some(v)=>Some(v.as_str().ok_or("clip_id must be string")?)};
            let q=&ss.project.color_pipeline;q.validate()?;
            let mut o=Options::sdr(kind);
            if q.enabled {
                o.gamut=match q.output_gamut {0=>Gamut::Rec709,1=>Gamut::Rec2020,_=>return Err("display scope supports configured Rec709 or Rec2020 output only".into())};
                o.encoding=match q.output_transfer {0=>Encoding::Linear,1=>Encoding::Srgb,6=>Encoding::Pq,7=>Encoding::Hlg,8=>Encoding::Bt709,_=>return Err("scope output transfer must be linear/sRGB/BT709/PQ/HLG".into())};
                o.reference_white_nits=q.reference_white;
            }
            let default_units=if kind==Kind::Vectorscope || !matches!(o.encoding,Encoding::Pq|Encoding::Hlg|Encoding::Linear){"ire"}else{"nits"};
            o.units=match a.get("units").map(|_|string(a,"units")).transpose()?.unwrap_or(default_units){"ire"=>Units::Ire,"nits"=>Units::Nits,_=>return Err("units must be ire/nits".into())};
            o.hlg_peak_nits=number(a,"hlg_peak_nits",1000.)?;o.hlg_system_gamma=number(a,"hlg_system_gamma",1.2)?;
            let ceiling=if o.units==Units::Ire{100.}else{match o.encoding {Encoding::Pq=>10000.,Encoding::Hlg=>o.hlg_peak_nits,_=>o.reference_white_nits}};
            o.ceiling=number(a,"ceiling",ceiling)?;
            o.targets_75=flag(a,"targets_75")?;
            let want_data=flag(a,"data")?;
            let bins=integer(a,"bins",16,4096)?.unwrap_or(256);
            let columns=integer(a,"columns",1,4096)?;
            let grid=integer(a,"grid",16,1024)?.unwrap_or(128);
            // Reject invalid declarations before loading/rendering media.
            o.validate()?;
            let mut frame=crate::colortools::any_frame(&ss.project,clip,time)?;
            if q.enabled {super::pipeline::output(&ss.project,&mut frame)?;}
            let image=scope_image::render(&frame,o)?;
            let data=if want_data {Some(scope_image::data(&frame,o,bins,columns.unwrap_or(frame.w.min(256)),grid)?)} else {None};
            std::fs::write(path,&image.png).map_err(|e|format!("write scope PNG: {e}"))?;
            let mut out=json!({"path":path,"png_bytes":image.png.len(),"configured_output_transform_applied":q.enabled,"scope":image.metadata});
            if let Some(d)=data {out["data"]=d;}
            Ok(out)
        }}
}
