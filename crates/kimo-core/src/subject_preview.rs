//! Bounded preview-only semantic match: identity-transform clips, scratch masks detached.
use crate::{effects::Frame,model::*,render,color::{subject_match as sm,managed},keyframe::Prop};
use serde_json::{Value,json};
use std::collections::BTreeMap;
fn text<'a>(a:&'a Value,k:&str)->Result<&'a str,String>{a.get(k).and_then(Value::as_str).ok_or(format!("missing {k}"))}
fn num(a:&Value,k:&str)->Result<f64,String>{a.get(k).and_then(Value::as_f64).filter(|v|v.is_finite()).ok_or(format!("missing finite {k}"))}
fn allowed(v:&Value,keys:&[&str])->Result<(),String>{let o=v.as_object().ok_or("expected object")?;for k in o.keys(){if !keys.contains(&k.as_str()){return Err(format!("unknown key {k}"))}}Ok(())}
fn sample(p:&Project,id:&str,t:f64,spec:&Value)->Result<(Frame,Vec<[f32;3]>,[bool;3]),String>{
 allowed(spec,&["skin","background","lighting"])?;
 let c=p.find_clip(id).ok_or("no clip")?;let lt=t-c.start;if lt<0.||lt>=c.duration{return Err("sample time outside clip".into())}
 if c.corners.is_some()||c.motion_path.is_some()||c.echo.is_some()||c.motion_blur!=0.||c.posterize_fps!=0.||c.transition_in.is_some()||matches!(c.source,Source::Adjustment|Source::Precomp{..})||c.props.keys().any(|k|k.starts_with("pin_"))||c.effects.iter().any(|e|e.strings.contains_key("mask_layer"))||!matches!(c.time_mode,TimeMode::Normal){return Err("preview requires identity clip geometry/time mode".into())}
 let defs=crate::model::default_props();for k in ["x","y","scale","scale_x","scale_y","rotation","rot_x","rot_y","pos_z","opacity"]{if let Some(v)=c.props.get(k){let default=defs.get(k).map(|x|x.base).unwrap_or(0.);if v.base!=default||!v.keys.is_empty()||!v.expr.is_empty(){return Err(format!("preview requires default {k}"))}}}
 let mut q=p.clone();for tr in &mut q.tracks{tr.clips.retain(|x|x.id==id);}q.find_clip_mut(id).unwrap().mask=None;
 let mut cache=render::Cache::new();render::prepare_at(p,&mut cache,Some(t))?;let mut f=render::render_frame_opts(&q,t,&cache,true)?;
 if p.color_pipeline.enabled{let mut pr=BTreeMap::new();for(k,v)in[("from_gamut",p.color_pipeline.working_gamut),("to_gamut",0),("from_transfer",0),("to_transfer",1)]{pr.insert(k.into(),Prop::new(v as f64));}pr.insert("reference_white".into(),Prop::new(p.color_pipeline.reference_white));managed::apply(&pr,&mut f,0.)?;}
 crate::guard::reserve(f.w.checked_mul(f.h).and_then(|n|n.checked_mul(12)).ok_or("region overflow")?);let mut weights=vec![[0f32;3];f.w*f.h];let mut active=[false;3];
 for(r,name)in ["skin","background","lighting"].iter().enumerate(){if let Some(v)=spec.get(name){let mid=v.as_str().ok_or("mask layer ID must be string")?;let layer=c.mask.as_ref().and_then(|m|m.layers.iter().find(|l|l.id==mid)).ok_or(format!("no mask layer {mid} on {id}"))?;
 let refs=|rid:&str|render::render_clip_alone(p,rid,t,&cache);let ctx=crate::masks::Ctx{refs:&refs,fps:p.fps};if !layer.enabled||layer.kind=="matte"{return Err("disabled/reference matte unsupported in first preview".into())}let cov=crate::masks::layer_coverage(layer,&f,lt,&ctx);for(i,v)in cov.into_iter().enumerate(){if !v.is_finite()||v < -1e-6||v > 1.+1e-6{return Err(format!("invalid coverage {v}"))}weights[i][r]=v.clamp(0.,1.);}active[r]=true;}}
 Ok((f,weights,active))
}
fn stats(s:&sm::ShotStats)->Value{json!(s.regions.iter().map(|r|r.as_ref().map(|r|json!({"pixels":r.contributing_pixels,"weight":r.weight,"effective_samples":r.effective_samples,"lab_mean":r.lab_mean,"log_luminance_mean":r.log_luminance_mean,"log_luminance_variance":r.log_luminance_variance}))).collect::<Vec<_>>())}
pub fn run(p:&Project,a:&Value)->Result<Value,String>{
 allowed(a,&["clip_id","ref_clip_id","time","ref_time","source_regions","ref_regions","options","before_path","after_path"])?;
 let id=text(a,"clip_id")?;let rid=text(a,"ref_clip_id")?;if id==rid{return Err("source and reference must differ".into())}
 let before=text(a,"before_path")?;let after=text(a,"after_path")?;if before==after||![before,after].iter().all(|v|v.to_lowercase().ends_with(".png")){return Err("distinct PNG paths required".into())}
 let (fs,ws,active)=sample(p,id,num(a,"time")?,a.get("source_regions").ok_or("missing source_regions")?)?;let(fr,wr,ra)=sample(p,rid,num(a,"ref_time")?,a.get("ref_regions").ok_or("missing ref_regions")?)?;if active!=ra||!active.iter().any(|x|*x){return Err("source/reference must name same nonempty region set".into())}
 let mut o=sm::Options::default();for r in 0..3{if !active[r]{o.region_amount[r]=0.;}}
 if let Some(v)=a.get("options"){allowed(v,&["amount","region_amount","regularization","contrast_amount","max_exposure_stops","exposure_target"])?;for k in ["amount","regularization","contrast_amount","max_exposure_stops"]{if v.get(k).is_some(){let n=num(v,k)?;match k{"amount"=>o.amount=n,"regularization"=>o.regularization=n,"contrast_amount"=>o.contrast_amount=n,_=>o.max_exposure_stops=n}}}if let Some(v)=v.get("region_amount"){o.region_amount=serde_json::from_value(v.clone()).map_err(|e|e.to_string())?;for r in 0..3{if !active[r]&&o.region_amount[r]!=0.{return Err("missing region must have amount0".into())}}}if let Some(v)=v.get("exposure_target"){let vs:Vec<Value>=serde_json::from_value(v.clone()).map_err(|e|e.to_string())?;if vs.len()!=3{return Err("exposure_target needs3 entries".into())}for(r,v)in vs.iter().enumerate(){o.exposure_target[r]=match v.as_str(){Some("reference")=>sm::ExposureTarget::Reference,Some("preserve_source")=>sm::ExposureTarget::PreserveSource,_=>{let n=v.as_f64().ok_or("target reference/preserve_source or linear luminance")?;sm::ExposureTarget::LinearLuminance(n)}}}}}
 let plan=sm::fit(&fs,&sm::Regions{weights:&ws},&fr,&sm::Regions{weights:&wr},o)?;let mut out=Frame{w:fs.w,h:fs.h,d:fs.d.clone()};plan.apply(&mut out,&sm::Regions{weights:&ws})?;let result=sm::statistics(&out,&sm::Regions{weights:&ws})?;
 render::write_image(&fs,before)?;render::write_image(&out,after)?;
 Ok(json!({"before_path":before,"after_path":after,"source_stats":stats(&plan.source),"reference_stats":stats(&plan.reference),"result_stats":stats(&result),"mutated":false,"domain":"display_sRGB_Rec709_D65","note":"Preview only; identity geometry, semantic masks detached from visibility. No persistent recipe, automatic skin detection or physical lighting estimate."}))
}
