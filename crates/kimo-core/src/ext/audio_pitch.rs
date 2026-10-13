//! Read-only YIN pitch frames using strict stereo 48k acquisition and a pure std vendored core.
use super::kit::*;
fn num(a:&Value,k:&str,d:f64)->Result<f64,String> {match a.get(k) {None=>Ok(d),Some(v)=>v.as_f64().filter(|x|x.is_finite()).ok_or_else(||crate::errors::arg_err(a,k,"number"))}}
fn run(ss:&mut Session,a:&Value)->Result<Value,String> {
    let min=num(a,"min_hz",50.)?; let max=num(a,"max_hz",2000.)?; let threshold=num(a,"threshold",0.15)?;
    if min<20. || max<=min || max>=24000. {return Err("pitch range must satisfy 20 <= min_hz < max_hz < 24000".into());}
    if threshold<=0. || threshold>=1. {return Err("threshold must be >0 and <1".into());}
    let cap=match a.get("max_items") {None=>500,Some(v)=>v.as_u64().filter(|v|*v>=1 && *v<=5000).ok_or("max_items must be 1..5000")? as usize};
    let target=a.get("target").cloned().unwrap_or(json!("mix"));
    let decoded=crate::perception::audio_targets::decode(ss,&target)?;
    if decoded.pcm.is_empty() {return Err("no_audio: empty target".into());}
    let total=decoded.pcm.len()/2; let start=num(a,"start",decoded.origin)?; let end=num(a,"end",decoded.origin+total as f64/48000.)?;
    if start<decoded.origin || end<=start || start>=decoded.origin+total as f64/48000. {return Err("invalid pitch range".into());}
    let i0=((start-decoded.origin)*48000.).round() as usize;
    let i1=(((end-decoded.origin)*48000.).round() as usize).min(total);
    crate::guard::reserve((i1-i0).checked_mul(4).ok_or("pitch allocation overflow")?);
    let mono:Vec<f32>=decoded.pcm[2*i0..2*i1].chunks_exact(2).map(|x|(x[0]+x[1])*0.5).collect();
    const WIN:usize=4096; const HOP:usize=1024;
    let n=if mono.len()<WIN {0} else {(mono.len()-WIN)/HOP+1};
    let mut frames=Vec::new();
    for j in 0..n.min(cap) {crate::guard::check();let st=j*HOP;let x=&mono[st..st+WIN];
        let rms=(x.iter().map(|v|(*v as f64).powi(2)).sum::<f64>()/WIN as f64).sqrt();
        let pitch=if rms>1e-5 {crate::dsp::pitch::pitch_detect(x,48000.,min as f32,max as f32,threshold as f32)} else {None};
        let (hz,confidence)=pitch.map_or((None,0.),|(h,c)|(Some(h),c));
        frames.push(json!({"t":decoded.origin+(i0+st) as f64/48000.,"start_sample":i0+st,"end_sample":i0+st+WIN,"hz":hz,"confidence":confidence,"voiced":hz.is_some(),"rms_db":if rms>0.{Some(20.*rms.log10())}else{None}}));
    }
    Ok(json!({"frames":frames,"count":frames.len(),"truncated":n>cap,"timebase":crate::perception::timebase::meta(ss.project.fps),"method":{"algorithm":"YIN CMND with parabolic lag interpolation","confidence":"1 - CMND at selected lag; zero when unvoiced, not a calibrated probability","sample_rate":48000,"channels":"mono arithmetic stereo mean (L+R)/2","rms_gate":1e-5,"rms_gate_method":"unvoiced when mono frame RMS <= 1e-5","frame_samples":WIN,"hop_samples":HOP,"min_hz":min,"max_hz":max,"threshold":threshold,"source":decoded.source}}))
}
pub fn commands()->Vec<Command> {vec![Command{name:"audio_pitch",mutates:false,description:"Read-only YIN fundamental pitch frames, Hz and CMND confidence (not calibrated probability). Target clip_id, track_id, mix or local path; strict stereo48k decode, arithmetic mono mean. Fixed4096-sample windows and1024-sample hops. Unvoiced Hz null and confidence0; max_items cap reports truncation. No note transcription or polyphonic pitch claim.",schema:obj(vec![("target",json!({"oneOf":[{"type":"string"},{"type":"object"}]})),("start",n("timeline seconds; path source seconds")),("end",n("range end seconds")),("min_hz",n("default50, minimum20")),("max_hz",n("default2000, below24000")),("threshold",n("YIN threshold, default0.15, >0 and <1")),("max_items",json!({"type":"integer","minimum":1,"maximum":5000}))],&[]),run}]}
