//! Planar float GBR(A) interchange for ffmpeg EXR/PFM paths.
use crate::effects::Frame;
pub fn unpack(bytes:&[u8],w:usize,h:usize)->Result<Vec<f32>,String>{let n=w.checked_mul(h).ok_or("float dimensions overflow")?;if bytes.len()!=n.checked_mul(16).ok_or("float length overflow")?{return Err("invalid planar float frame length".into())}crate::guard::reserve(n.checked_mul(16).ok_or("float allocation overflow")?);let mut out=vec![0.;n*4];for (plane,ch)in [1,2,0,3].iter().enumerate(){for i in 0..n{if i%65536==0{crate::guard::check()}let k=(plane*n+i)*4;let v=f32::from_le_bytes(bytes[k..k+4].try_into().unwrap());if !v.is_finite(){return Err("nonfinite float source".into())}if *ch==3&&!(0. ..=1.).contains(&v){return Err("float source alpha outside0..1".into())}out[i*4+ch]=v;}}Ok(out)}
pub fn pack(f:&Frame)->Result<Vec<u8>,String>{super::frame::validate(f)?;let n=f.w.checked_mul(f.h).ok_or("float dimensions overflow")?;if n.checked_mul(4)!=Some(f.d.len()){return Err("invalid float frame shape".into())}let len=n.checked_mul(16).ok_or("float allocation overflow")?;crate::guard::reserve(len);let mut b=Vec::with_capacity(len);for ch in [1,2,0,3]{for i in 0..n{if i%65536==0{crate::guard::check()}let v=f.d[i*4+ch];if !v.is_finite(){return Err("nonfinite float output".into())}b.extend_from_slice(&v.to_le_bytes());}}Ok(b)}
/// Alpha-correct float fit: exact area integration when shrinking, bilinear when enlarging.
/// Preserves signed HDR samples without clipping. No ringing from negative-lobe kernels.
pub fn fit(f:&Frame,w:usize,h:usize,mode:f64)->Result<Vec<f32>,String>{
 if !mode.is_finite()||f.w.checked_mul(f.h).and_then(|n|n.checked_mul(4))!=Some(f.d.len())||f.w==0||f.h==0||w==0||h==0{return Err("invalid float dimensions".into())}
 if f.d.iter().any(|v|!v.is_finite())||f.d.chunks_exact(4).any(|v|!(0. ..=1.).contains(&v[3])){return Err("float fit requires finite RGB and alpha0..1".into())}
 let sx=w as f64/f.w as f64;let sy=h as f64/f.h as f64;
 let(sx,sy)=if mode>=1.5{(sx,sy)}else{let k=if mode>=0.5{sx.max(sy)}else{sx.min(sy)};(k,k)};
 let ox=(w as f64-f.w as f64*sx)/2.;let oy=(h as f64-f.h as f64*sy)/2.;
 let len=w.checked_mul(h).and_then(|n|n.checked_mul(4)).ok_or("float fit size overflow")?;crate::guard::reserve(len.checked_mul(4).ok_or("float fit allocation overflow")?);
 let xs=axis_weights(f.w,w,sx,ox);let ys=axis_weights(f.h,h,sy,oy);let mut out=vec![0.;len];
 for(y,yw)in ys.iter().enumerate(){crate::guard::check();for(x,xw)in xs.iter().enumerate(){let mut c=[0f64;4];for &(yy,wy)in yw{for &(xx,wx)in xw{let i=(yy*f.w+xx)*4;let a=f.d[i+3] as f64;let k=wy*wx;c[3]+=a*k;for j in 0..3{c[j]+=f.d[i+j] as f64*a*k;}}}let i=(y*w+x)*4;out[i+3]=c[3] as f32;for j in 0..3{out[i+j]=if c[3]>0.{(c[j]/c[3]) as f32}else{0.};}}}Ok(out)
}
fn axis_weights(src:usize,dst:usize,scale:f64,offset:f64)->Vec<Vec<(usize,f64)>>{
 (0..dst).map(|x|{let center=(x as f64+0.5-offset)/scale;if scale<1.{let lo=(x as f64-offset)/scale;let hi=(x as f64+1.-offset)/scale;let mut v=Vec::new();let a=lo.floor().max(0.) as usize;let b=(hi.ceil().max(0.) as usize).min(src);for i in a..b{let k=(hi.min(i as f64+1.)-lo.max(i as f64)).max(0.)/(hi-lo);if k>0.{v.push((i,k));}}v}else if center<0.||center>=src as f64{vec![]}else{let u=(center-0.5).clamp(0.,(src-1) as f64);let a=u.floor() as usize;let b=(a+1).min(src-1);let t=u-a as f64;if a==b{vec![(a,1.)]}else{vec![(a,1.-t),(b,t)]}}}).collect()
}
pub fn tiff_size(path:&str)->Result<(u32,u32),String>{let f=std::fs::File::open(path).map_err(|e|e.to_string())?;tiff::decoder::Decoder::new(std::io::BufReader::new(f)).and_then(|mut d|d.dimensions()).map_err(|e|e.to_string())}
pub fn tiff_frame(path:&str,budget:usize)->Result<Frame,String>{
 use tiff::decoder::{Decoder,DecodingResult};
 let file=std::fs::File::open(path).map_err(|e|e.to_string())?;let mut limits=tiff::decoder::Limits::default();limits.decoding_buffer_size=budget;limits.intermediate_buffer_size=budget;
 let mut dec=Decoder::new(std::io::BufReader::new(file)).map_err(|e|e.to_string())?.with_limits(limits);let(w,h)=dec.dimensions().map_err(|e|e.to_string())?;
 let channels=match dec.colortype().map_err(|e|e.to_string())?{tiff::ColorType::RGB(_)=>3,tiff::ColorType::RGBA(_)=>4,_=>return Err("TIFF supports RGB/RGBA only".into())};
 if dec.get_tag_u32(tiff::tags::Tag::Orientation).unwrap_or(1)!=1{return Err("TIFF orientation must be top-left (1)".into())}
 if channels==4&&dec.get_tag_u16_vec(tiff::tags::Tag::ExtraSamples).map(|v|v.contains(&1)).unwrap_or(false){return Err("associated-alpha TIFF unsupported; provide straight-alpha TIFF".into())}
 let n=(w as usize).checked_mul(h as usize).ok_or("TIFF dimensions overflow")?;if n.checked_mul(32).ok_or("TIFF allocation overflow")?>budget{return Err("TIFF exceeds float decode budget".into())}
 crate::guard::reserve(n*32);
 let values=match dec.read_image().map_err(|e|e.to_string())?{DecodingResult::F32(v)=>v,DecodingResult::F16(v)=>v.iter().map(|v|v.to_f32()).collect(),DecodingResult::U8(v)=>v.iter().map(|v|*v as f32/255.).collect(),DecodingResult::U16(v)=>v.iter().map(|v|*v as f32/65535.).collect(),_=>return Err("TIFF supported samples: float16/32 or unsigned8/16".into())};
 if values.len()!=n*channels||values.iter().any(|v|!v.is_finite()){return Err("invalid/nonfinite TIFF samples".into())}
 let mut d=Vec::with_capacity(n*4);for v in values.chunks_exact(channels){d.extend_from_slice(&v[..3]);d.push(if channels==4{v[3]}else{1.})}Ok(Frame{w:w as usize,h:h as usize,d})
}
/// Scene-linear RGBA with standard chromaticities and caller-declared white luminance.
pub fn write_exr(f:&Frame,path:&str,gamut:usize,white:f64)->Result<(),String>{
 use exr::{meta::attribute::Chromaticities,prelude::*};
 if gamut>7||!white.is_finite()||!(1. ..=10000.).contains(&white){return Err("invalid EXR color metadata".into())}
 if f.w.checked_mul(f.h).and_then(|v|v.checked_mul(4))!=Some(f.d.len())||f.w==0||f.h==0||f.d.iter().any(|v|!v.is_finite())||f.d.chunks_exact(4).any(|v|!(0. ..=1.).contains(&v[3])){return Err("EXR needs finite RGB and alpha0..1".into())}
 let (c,w)=super::managed::chroma(gamut);let xy=|q:[f64;2]|Vec2(q[0] as f32,q[1] as f32);
 let mut attributes=LayerAttributes::named("scene-linear");attributes.white_luminance=Some(white as f32);attributes.adopted_neutral=Some(xy(w));attributes.comments=Some(Text::from("Scene-linear working RGB; whiteLuminance is caller-declared, not measured. No ACES output transform."));
 let layer=Layer::new((f.w,f.h),attributes,Encoding::SMALL_FAST_LOSSLESS,SpecificChannels::rgba(|pos:Vec2<usize>|{let i=(pos.y()*f.w+pos.x())*4;(f.d[i],f.d[i+1],f.d[i+2],f.d[i+3])}));let mut image=Image::from_layer(layer);image.attributes.chromaticities=Some(Chromaticities{red:xy(c[0]),green:xy(c[1]),blue:xy(c[2]),white:xy(w)});image.write().to_file(path).map_err(|e|e.to_string())
}
/// Bounded header-only inspection, does not select an input transform.
pub fn exr_info(path:&str)->Result<serde_json::Value,String>{
 use std::io::Read;use exr::meta::MetaData;use serde_json::json;
 let file=std::fs::File::open(path).map_err(|e|e.to_string())?;let meta=MetaData::read_from_unbuffered(file.take(1024*1024),true).map_err(|e|format!("EXR header (max1MiB): {e}"))?;
 let mut layers=Vec::new();for h in &meta.headers{
 let chroma=h.shared_attributes.chromaticities.as_ref().map(|c|[[c.red.x(),c.red.y()],[c.green.x(),c.green.y()],[c.blue.x(),c.blue.y()],[c.white.x(),c.white.y()]]);
 let matches=chroma.map(|q|(0..8).filter(|&g|{let(p,w)=super::managed::chroma(g);let target=[p[0],p[1],p[2],w];(0..4).all(|i|(0..2).all(|j|(q[i][j] as f64-target[i][j]).abs()<1e-5))}).collect::<Vec<_>>());
 let channels:Vec<_>=h.channels.list.iter().map(|c|json!({"name":c.name.to_string(),"sample_type":format!("{:?}",c.sample_type),"sampling":[c.sampling.x(),c.sampling.y()]})).collect();
 let standard=h.channels.list.iter().all(|c|["R","G","B","A"].contains(&c.name.to_string().as_str())&&c.sampling.x()==1&&c.sampling.y()==1&&c.sample_type!=exr::meta::attribute::SampleType::U32)&&["R","G","B"].iter().all(|name|h.channels.list.iter().any(|c|c.name.to_string()==*name));
 layers.push(json!({"size":[h.layer_size.x(),h.layer_size.y()],"data_origin":[h.own_attributes.layer_position.x(),h.own_attributes.layer_position.y()],"deep":h.deep,"channels":channels,"standard_rgb_channels":standard,"chromaticities_xy":chroma,"matching_gamut_ids":matches,"declared_white_luminance":h.own_attributes.white_luminance,"compression":format!("{:?}",h.compression)}));}
 Ok(json!({"format":"EXR","layers":layers,"transfer":"not inferred from chromaticities","note":"Metadata is file-declared. Matching gamut is only a suggestion, never applied automatically; confirm encoding before set_clip_input_color."}))
}
pub fn validate_exr_source(path:&str)->Result<(),String>{let v=exr_info(path)?;let layers=v["layers"].as_array().ok_or("invalid EXR metadata")?;if layers.len()!=1||layers[0]["deep"]==true||layers[0]["standard_rgb_channels"]!=true{return Err("EXR decode supports single flat RGB(A) layer with unit sampling only".into())}if layers[0]["data_origin"]!=serde_json::json!([0,0]){return Err("EXR nonzero data-window origin unsupported".into())}Ok(())}
pub fn tiff_info(path:&str)->Result<serde_json::Value,String>{
 use serde_json::json;use tiff::{decoder::Decoder,tags::Tag};let mut d=Decoder::new(std::io::BufReader::new(std::fs::File::open(path).map_err(|e|e.to_string())?)).map_err(|e|e.to_string())?;
 let(w,h)=d.dimensions().map_err(|e|e.to_string())?;let color=format!("{:?}",d.colortype().map_err(|e|e.to_string())?);let sample=d.get_tag_u16_vec(Tag::SampleFormat).ok();let extras=d.get_tag_u16_vec(Tag::ExtraSamples).ok();let orientation=d.get_tag_u32(Tag::Orientation).unwrap_or(1);let description=d.get_tag_ascii_string(Tag::ImageDescription).ok();let white=tiff_rational_values(&mut d,Tag::Unknown(318));let primaries=tiff_rational_values(&mut d,Tag::Unknown(319));
 Ok(json!({"format":"TIFF","size":[w,h],"color_type":color,"sample_format":sample,"extra_samples":extras,"orientation":orientation,"file_description":description,"white_point_xy":white,"primary_chromaticities_xy":primaries,"more_pages":d.more_images(),"note":"First-page header inspection only. Gamut/transfer not inferred, ICC not applied. Associated alpha and non-top-left orientation are rejected in managed decode."}))
}
/// Full-float working RGBA TIFF, straight alpha, explicit descriptive color metadata.
pub fn write_tiff(f:&Frame,path:&str,gamut:usize,white:f64)->Result<(),String>{
 use tiff::{encoder::{TiffEncoder,colortype::RGBA32Float,Rational},tags::Tag};
 if gamut>7||!white.is_finite()||!(1. ..=10000.).contains(&white)||f.w==0||f.h==0||f.w>u32::MAX as usize||f.h>u32::MAX as usize{return Err("invalid TIFF color/dimensions".into())}
 if f.w.checked_mul(f.h).and_then(|n|n.checked_mul(4))!=Some(f.d.len())||f.d.iter().any(|v|!v.is_finite())||f.d.chunks_exact(4).any(|v|!(0. ..=1.).contains(&v[3])){return Err("TIFF needs finite RGB and alpha0..1".into())}
 let file=std::fs::File::create(path).map_err(|e|e.to_string())?;let mut enc=TiffEncoder::new(std::io::BufWriter::new(file)).map_err(|e|e.to_string())?;let mut image=enc.new_image::<RGBA32Float>(f.w as u32,f.h as u32).map_err(|e|e.to_string())?;
 image.encoder().write_tag(Tag::ExtraSamples,&[2u16][..]).map_err(|e|e.to_string())?;
 let description=format!("Kimo scene-linear working RGB; gamut_id={gamut}; declared_reference_white_nits={white}; no ICC profile applied; no output transform");image.encoder().write_tag(Tag::ImageDescription,description.as_str()).map_err(|e|e.to_string())?;
 let(p,w)=super::managed::chroma(gamut);if p.iter().flatten().chain(w.iter()).all(|v|*v>=0.){let rat=|v:f64|Rational{n:(v*1000000.).round() as u32,d:1000000};image.encoder().write_tag(Tag::Unknown(318),&[rat(w[0]),rat(w[1])][..]).map_err(|e|e.to_string())?;image.encoder().write_tag(Tag::Unknown(319),&p.iter().flatten().map(|v|rat(*v)).collect::<Vec<_>>()[..]).map_err(|e|e.to_string())?;}
 image.write_data(&f.d).map_err(|e|e.to_string())
}
fn tiff_rational_values(d:&mut tiff::decoder::Decoder<std::io::BufReader<std::fs::File>>,tag:tiff::tags::Tag)->Option<Vec<f64>>{use tiff::decoder::ifd::Value;fn one(v:Value)->Option<f64>{match v{Value::Rational(n,d)if d!=0=>Some(n as f64/d as f64),Value::SRational(n,d)if d!=0=>Some(n as f64/d as f64),_=>None}}match d.get_tag(tag).ok()?{Value::List(v)=>v.into_iter().map(one).collect(),v=>Some(vec![one(v)?])}}
