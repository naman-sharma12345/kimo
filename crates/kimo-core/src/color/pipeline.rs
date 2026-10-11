//! Opt-in project color pipeline. Legacy absent/default is unchanged.
use serde::{Serialize,Deserialize};
use crate::{effects::Frame,model::{Project,Clip}};
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(default)]pub struct Pipeline {
    pub linear_resample:bool,pub mastering:Option<super::metadata::Mastering>,pub enabled:bool,pub working_gamut:usize,pub input_gamut:usize,pub input_transfer:usize,
    pub output_gamut:usize,pub output_transfer:usize,pub reference_white:f64,pub tone_map:bool,pub gamut_compress:bool,
}
impl Default for Pipeline{fn default()->Self{Self{linear_resample:false,mastering:None,enabled:false,working_gamut:3,input_gamut:0,input_transfer:1,output_gamut:0,output_transfer:1,reference_white:100.,tone_map:false,gamut_compress:false}}}
impl Pipeline{pub fn validate(&self)->Result<(),String>{if self.working_gamut>7||self.input_gamut>7||self.output_gamut>7||self.input_transfer>8||self.output_transfer>8||!self.reference_white.is_finite()||!(1. ..=10000.).contains(&self.reference_white){return Err("invalid project color pipeline".into())}if let Some(m)=&self.mastering{m.validate()?;if !self.enabled||self.output_gamut!=1||self.output_transfer!=6{return Err("static mastering metadata requires enabled Rec2020 PQ output".into())}}Ok(())}}
fn transform(f:&mut Frame,from:usize,to:usize,a:usize,b:usize,white:f64,tone:bool,compress:bool)->Result<(),String>{
    let mut params=crate::effects::Params::new();for(k,v)in[("from_gamut",from as f64),("to_gamut",to as f64),("from_transfer",a as f64),("to_transfer",b as f64),("reference_white",white),("tone_map",if tone{1.}else{0.}),("gamut_compress",if compress{1.}else{0.})]{params.insert(k.into(),crate::keyframe::Prop::new(v));}super::managed::apply(&params,f,0.)
}
fn static_input(c:&Clip)->Result<(),String>{for k in ["input_gamut","input_transfer"]{if let Some(v)=c.props.get(k){if !v.keys.is_empty()||!v.expr.is_empty(){return Err("input color encoding is static; remove input_gamut/input_transfer animation".into())}}}Ok(())}
pub fn input(p:&Project,c:&Clip,f:&mut Frame)->Result<(),String>{let q=&p.color_pipeline;if !q.enabled{return Ok(())}q.validate()?;
    let is_media=matches!(c.source,crate::model::Source::Media{..});let (g,t)=if is_media {static_input(c)?;let g=c.props.get("input_gamut").map(|v|v.base).unwrap_or(q.input_gamut as f64);let t=c.props.get("input_transfer").map(|v|v.base).unwrap_or(q.input_transfer as f64);if g.fract()!=0.||t.fract()!=0.||!(0. ..=7.).contains(&g)||!(0. ..=8.).contains(&t){return Err("invalid clip input color".into())}(g as usize,t as usize)}else{(0,1)};
    transform(f,g,q.working_gamut,t,0,q.reference_white,false,false)
}
pub fn output(p:&Project,f:&mut Frame)->Result<(),String>{let q=&p.color_pipeline;if !q.enabled{return Ok(())}q.validate()?;transform(f,q.working_gamut,q.output_gamut,0,q.output_transfer,q.reference_white,q.tone_map,q.gamut_compress)}
pub fn rgba16(f:&Frame)->Result<Vec<u8>,String>{super::frame::validate(f)?;if f.w==0||f.h==0||f.w.checked_mul(f.h).and_then(|v|v.checked_mul(4))!=Some(f.d.len()){return Err("invalid RGBA16 frame shape".into())}crate::guard::reserve(f.d.len().checked_mul(2).ok_or("RGBA16 size overflow")?);let mut b=Vec::with_capacity(f.d.len()*2);for(i,v) in f.d.iter().enumerate(){if i%262144==0{crate::guard::check()}if !v.is_finite(){return Err("nonfinite output".into())}b.extend_from_slice(&((v.clamp(0.,1.)*65535.+0.5) as u16).to_le_bytes())}Ok(b)}

/// High-bit streaming export. PQ: Rec2020/PQ 10-bit HEVC tagged HDR10-compatible,
/// but no dynamic metadata and no implied HDR mastering certification.
pub fn export_high(p:&Project,path:&str,format:&str,crf:u32)->Result<f64,String>{
    export_high_with(p,path,format,crf,&p.color_pipeline,|f|output(p,f))
}
/// Shared encoding mechanics; caller provides an explicit delivery declaration and transform.
pub(crate) fn export_high_with(p:&Project,path:&str,format:&str,crf:u32,delivery_settings:&Pipeline,mut transform:impl FnMut(&mut Frame)->Result<(),String>)->Result<f64,String>{
    use std::process::Command;
    let (codec,pix)=match format{"hevc10"=>("libx265","yuv420p10le"),"ffv1_16"=>("ffv1","gbrp16le"),_=>return Err("format must be hevc10 or ffv1_16".into())};
    if crf>51{return Err("crf 0..51 required".into())}p.color_pipeline.validate()?;
    let duration=p.duration();if (duration*p.fps).round()<1.{return Err("timeline rounds to zero output frames".into())}if p.width==0||p.height==0||!p.fps.is_finite()||p.fps<=0.||!duration.is_finite()||duration<=0.{return Err("high-bit export requires nonzero dimensions, positive finite fps and duration".into())}
    if format=="hevc10"&&(p.width%2!=0||p.height%2!=0){return Err("HEVC420 requires even dimensions".into())}crate::guard::check_project(p,crate::guard::limits())?;
    let hdr=delivery_settings.enabled&&delivery_settings.output_transfer==6;
    if hdr&&delivery_settings.output_gamut!=1{return Err("PQ export requires Rec2020 output gamut=1".into())}
    if delivery_settings.enabled&&delivery_settings.output_transfer==7&&delivery_settings.output_gamut!=1{return Err("HLG export requires Rec2020 output gamut=1".into())}
    let delivery=super::encode_contract::high(delivery_settings,format)?;
    if format=="ffv1_16"&&!path.to_lowercase().ends_with(".mkv"){return Err("ffv1_16 requires .mkv".into())}
    let mut cmd=Command::new("ffmpeg");cmd.args(["-v","error","-y","-f","rawvideo","-pix_fmt","rgba64le","-s",&format!("{}x{}",p.width,p.height),"-r",&p.fps.to_string(),"-i","-","-c:v",codec,"-pix_fmt",pix]);
    if delivery_settings.mastering.is_some()&&format!="hevc10"{return Err("static mastering metadata supported by HEVC10 only".into())}
    let mut encoder_params="pools=2:frame-threads=2:log-level=error".to_string();if let Some(m)=&delivery_settings.mastering{encoder_params.push(':');encoder_params.push_str(&m.x265()?);}
    if format=="hevc10"{cmd.args(["-crf",&crf.to_string(),"-x265-params",&encoder_params,"-tag:v","hvc1"]);}
    if let Some(matrix)=delivery.matrix_scale{cmd.args(["-vf",&format!("scale=in_range=pc:out_color_matrix={matrix}:out_range=tv")]);}
    cmd.args(["-color_range",delivery.range,"-color_primaries",delivery.primaries,"-color_trc",delivery.transfer_tag,"-colorspace",delivery.matrix]);
    let count=(p.duration()*p.fps).round() as usize;let frame_bytes=(p.width as usize).checked_mul(p.height as usize).and_then(|v|v.checked_mul(8)).ok_or("encoder frame overflow")?;
    let mut encoder=super::process::EncodeStream::new(cmd.arg(path),frame_bytes,super::process::encode_deadline(count))?;let mut cache=crate::render::Cache::new();
    for i in 0..count{crate::guard::check();let t=i as f64/p.fps;crate::render::prepare_export_range(p,&mut cache,t,t)?;let mut f=crate::render::render_frame(p,t,&cache)?;transform(&mut f)?;encoder.write(rgba16(&f)?)?}encoder.finish()?;
    // Keep PCM audio intact in FFV1/MKV; AAC for MP4 HEVC.
    let pcm=crate::audio::mix_strict(p).map_err(|e|e.to_string())?;if pcm.iter().any(|v|v.abs()>1e-6){super::mux::audio(path,&pcm,format,count)?}
    Ok(count as f64/p.fps)
}
/// Exact integer16 transfer lookup, scoped to one native decode window.
/// Table uses the same normalized f32 input values as the general float transform.
pub struct Input16{table:Vec<f64>,matrix:super::managed::Mat}
impl Input16{
 pub fn new(p:&Project,c:&Clip)->Result<Self,String>{let q=&p.color_pipeline;q.validate()?;static_input(c)?;let g=c.props.get("input_gamut").map(|v|v.base).unwrap_or(q.input_gamut as f64);let t=c.props.get("input_transfer").map(|v|v.base).unwrap_or(q.input_transfer as f64);if g.fract()!=0.||t.fract()!=0.||!(0. ..=7.).contains(&g)||!(0. ..=8.).contains(&t){return Err("invalid clip input color".into())}crate::guard::reserve(65536*8);let table=(0..65536).map(|v|super::managed::decode(t as usize,(v as f32/65535.) as f64,q.reference_white as f32 as f64)).collect();Ok(Self{table,matrix:super::managed::conversion(g as usize,q.working_gamut)})}
 pub fn decode(&self,bytes:&[u8])->Result<Vec<f32>,String>{if bytes.len()%8!=0{return Err("invalid integer16 frame".into())}let mut out=Vec::with_capacity(bytes.len()/2);for px in bytes.chunks_exact(8){let word=|j:usize|u16::from_le_bytes([px[j*2],px[j*2+1]]) as usize;let c=super::managed::mv(self.matrix,[self.table[word(0)],self.table[word(1)],self.table[word(2)]]);if c.iter().any(|v|!v.is_finite()||v.abs()>f32::MAX as f64){return Err("integer input color overflow".into())}out.extend(c.map(|v|v as f32));out.push(word(3) as f32/65535.);}Ok(out)}
}
/// Inspect declarations and effect contracts, without media/dependency probing.
pub fn inspect(p:&Project)->Result<serde_json::Value,String>{use serde_json::json;p.color_pipeline.validate()?;let q=&p.color_pipeline;let gamuts=["Rec709 D65","Rec2020 D65","P3 D65","ACEScg AP1 D60","ACES AP0 D60","ARRI Wide Gamut3","Sony SGamut3","Sony SGamut3Cine"];let transfers=["scene linear","sRGB","gamma2.4","ACEScct","LogC3 EI800","SLog3","ST2084 PQ","scene HLG OETF","BT709 OETF"];let mut bindings=Vec::new();let mut inputs=Vec::new();for c in crate::render::flat_clips(&p.tracks,0.){if let crate::model::Source::Media{asset}=&c.source{let g=c.props.get("input_gamut").map(|v|v.base).unwrap_or(q.input_gamut as f64);let t=c.props.get("input_transfer").map(|v|v.base).unwrap_or(q.input_transfer as f64);let valid=g.is_finite()&&t.is_finite()&&g.fract()==0.&&t.fract()==0.&&(0. ..=7.).contains(&g)&&(0. ..=8.).contains(&t);let animated=["input_gamut","input_transfer"].iter().any(|k|c.props.get(*k).map(|v|!v.keys.is_empty()||!v.expr.is_empty()).unwrap_or(false));inputs.push(json!({"clip_id":c.id,"asset_id":asset,"input_gamut_id":g,"input_transfer_id":t,"valid_ids":valid,"unsupported_animation_present":animated,"selection":"base override or project default, never file inferred"}));}for e in &c.effects{if e.strings.contains_key("bound_linear_gamut")||e.strings.contains_key("wrapper_working_gamut"){let bound=e.strings.get("bound_linear_gamut");let wrapper=e.strings.get("wrapper_working_gamut");let white=e.strings.get("wrapper_reference_white");let stale=!q.enabled||bound.map(|v|v.parse::<usize>().ok()!=Some(q.working_gamut)||e.params.get("gamut").map(|v|(v.base,v.keys.is_empty(),v.expr.is_empty()))!=Some((q.working_gamut as f64,true,true))).unwrap_or(false)||wrapper.map(|v|v.parse::<usize>().ok()!=Some(q.working_gamut)).unwrap_or(false)||white.map(|v|v.parse::<f64>().ok()!=Some(q.reference_white)).unwrap_or(false);bindings.push(json!({"clip_id":c.id,"effect_id":e.id,"kind":e.kind,"bound_linear_gamut":bound,"wrapper_working_gamut":wrapper,"wrapper_gamut":e.strings.get("wrapper_gamut"),"wrapper_transfer":e.strings.get("wrapper_transfer"),"stale_project_contract":stale}));}}}
 let output_tag=match q.output_transfer{0=>Some("linear"),1=>Some("iec61966-2-1"),6=>Some("smpte2084"),7=>Some("arib-std-b67"),8=>Some("bt709"),_=>None};
 Ok(json!({"enabled":q.enabled,"render_domain":if q.enabled{"working_scene_linear"}else{"legacy_display_referred"},"working_gamut":gamuts[q.working_gamut],"default_media_input_gamut":gamuts[q.input_gamut],"default_media_input_transfer":transfers[q.input_transfer],"synthetic_input":"legacy Rec709/sRGB declaration","configured_output_gamut":gamuts[q.output_gamut],"configured_output_transfer":transfers[q.output_transfer],"settings":q,"media_input_contracts":inputs,"effect_contracts":bindings,"reference_tool":{"optional":true,"requires":"installed python3 + opencolorio==2.5.1","dependency_availability":"not checked","needs_ap1_working":true,"video_audio":false,"view":"ACES 2.0 - SDR 100 nits (Rec.709)"},"high_bit_output":{"exact_transfer_tag_candidate":output_tag,"tagged_gamut_paths":"Strict Rec709/Rec2020 delivery contract; unsupported pairs fail before encoding","delivery_contracts":{"hevc10":super::encode_contract::high(q,"hevc10").map(|v|serde_json::to_value(v).unwrap()).unwrap_or_else(|e|json!({"error":e})),"ffv1_16":super::encode_contract::high(q,"ffv1_16").map(|v|serde_json::to_value(v).unwrap()).unwrap_or_else(|e|json!({"error":e}))},"formats":["hevc10","ffv1_16"]},"limitations":["No automatic camera IDT or file-declared color selection","Legacy effects require caller-selected encoding wrappers when used in managed projects","HLG is scene OETF, not display OOTF","Reference white and mastering metadata are caller-declared, not measured","No calibrated mastering, ACES certification or industry parity claim","Optional OCIO is reference only, never the project default"]}))
}
