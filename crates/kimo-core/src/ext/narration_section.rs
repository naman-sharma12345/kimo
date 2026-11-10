//! RV-12 narration section: pure expansion over RV-8 bound transcript times.
use super::kit::*;
use super::batch::Op;
pub const VERSION: u32 = 1;
pub const KEYWORDS: &[&str] = &["narration", "speech", "captions", "transcript", "ducking"];
pub const SUMMARY: &str = "Transcript-bound narration captions and speech-window ducking; no audio or transcript is silently moved.";
pub fn schema() -> Value {
 let mut s=obj(vec![
  ("transcript_id",s("Existing RV-8 transcript with chosen takes and exact bound timeline words")),
  ("track_id",s("Existing unlocked caption track; omitted creates a dedicated track")),
  ("name_prefix",s("Name for a newly created caption track; default narration_section")),
  ("start",n("Optional assertion: must match first chosen word's absolute timeline start; never offsets media")),
  ("style",json!({"type":"string","enum":["subtitle","boxed","yellow"],"description":"RV-8 caption style; default subtitle"})),
  ("music_track",s("Existing unlocked music track to duck by -18 dB relative gain; omitted creates captions only")), ("attack",n("Linear pre-roll seconds, .01..2; default .1")), ("release",n("Linear post-roll seconds, .01..5; default .3")), ("font",s("Caption font path/family")), ("max_lines",json!({"type":"integer","minimum":1,"maximum":4,"description":"Maximum lines per cue; default 2"})),
  ("max_chars_per_line",json!({"type":"integer","minimum":8,"maximum":120,"description":"Maximum characters per line; default 42"})),
  ("max_cps",n("Reading speed >= 1 characters/sec; default 17")), ("min_dur",n("Positive minimum cue duration seconds; default 1")),
  ("max_dur",n("Maximum cue duration seconds >= min_dur; default 6")), ("min_gap_frames",n("Nonnegative cue gap in frames; default 2"))
 ], &["transcript_id"]);
 s["space"]=json!("Absolute timeline seconds from RV-8 chosen word/take times; caption layout uses RV-8 presets. No normalized coordinates. No file exports.");s["additionalProperties"]=json!(false);s
}
fn literal(s:&str)->String { if s.starts_with('$'){format!("${s}")}else{s.into()} }
fn text<'a>(a:&'a Value,k:&str,d:&'a str)->Result<&'a str,String> { match a.get(k){None=>Ok(d),Some(v)=>v.as_str().ok_or_else(||format!("narration_section: '{k}' must be a string"))} }
fn num(a:&Value,k:&str,d:f64)->Result<f64,String> { match a.get(k){None=>Ok(d),Some(v)=>v.as_f64().filter(|x|x.is_finite()).ok_or_else(||format!("narration_section: '{k}' must be a finite number"))} }
fn transcript<'a>(ss:&'a Session,a:&Value)->Result<&'a crate::transcript::Transcript,String>{
 let id=text(a,"transcript_id","")?;let t=ss.project.editorial.transcripts.get(id).ok_or("narration_section: transcript_id does not name an existing transcript")?;
 t.validate().map_err(|_|"narration_section: invalid transcript structure".to_string())?;
 let words=t.script_words();if words.is_empty(){return Err("narration_section: transcript has no chosen words".into());}
 if words.iter().any(|w|!w.start.is_finite()||!w.end.is_finite()||w.start<0.0||w.end<=w.start){return Err("narration_section: chosen words need finite positive timeline spans".into());}
 // Stored RV-8 times must match their take binding; otherwise expansion cannot promise sync.
 let mut derived=t.clone();derived.derive_times();
 if words.iter().any(|w|derived.words.iter().find(|x|x.id==w.id).map_or(true,|x|(x.start-w.start).abs()>1e-9||(x.end-w.end).abs()>1e-9)){return Err("narration_section: word times drift from take bindings; recompute with transcript_set_take".into());}
 if words.windows(2).any(|w|w[1].start<w[0].start){return Err("narration_section: chosen words are not in timeline order".into());}
 if a.get("start").is_some() && (num(a,"start",0.0)?-words[0].start).abs()>1e-9{return Err("narration_section: start must match the bound first word; move takes explicitly with transcript_set_take".into());}
 Ok(t)
}
/// Sorted non-overlapping union of exact chosen-word spans, excluding rejected/alternate takes.
/// Pauses remain unducked; touching/overlapping words are merged, not caption dwell spans.
pub fn speech_windows(ss:&Session,a:&Value)->Result<Vec<[f64;2]>,String>{
 let t=transcript(ss,a)?;let mut words=t.script_words();words.sort_by(|a,b|a.start.total_cmp(&b.start));let mut out:Vec<[f64;2]>=vec![];
 for w in words { match out.last_mut(){Some(l) if w.start<=l[1]=>l[1]=l[1].max(w.end),_=>out.push([w.start,w.end])} }Ok(out)
}
pub fn expand(ss:&Session,a:&Value)->Result<Vec<Op>,String>{
 let obj=a.as_object().ok_or("narration_section: params must be an object")?;let schema=schema();
 for k in obj.keys(){if schema["properties"].get(k).is_none(){return Err(format!("narration_section: unknown parameter '{k}'"));}}
 for k in ["transcript_id","track_id","name_prefix","font","music_track"] {if a.get(k).is_some(){text(a,k,"")?;}}
 let t=transcript(ss,a)?;
 let style=text(a,"style","subtitle")?;if !["subtitle","boxed","yellow"].contains(&style){return Err("narration_section: style not allowed. Valid: subtitle, boxed, yellow".into());}
 let mut p=crate::transcript_captions::Params::new(num(a,"min_gap_frames",2.0)?,ss.project.fps);
 for (k,target,lo,hi) in [("max_lines",&mut p.max_lines,1usize,4usize),("max_chars_per_line",&mut p.max_chars,8,120)]{
  if let Some(v)=a.get(k){let x=v.as_u64().filter(|x|*x>=lo as u64&&*x<=hi as u64).ok_or_else(||format!("narration_section: '{k}' must be an integer in {lo}..{hi}"))?;*target=x as usize;}
 }
 p.max_cps=num(a,"max_cps",17.0)?;p.min_dur=num(a,"min_dur",1.0)?;p.max_dur=num(a,"max_dur",6.0)?;p.check()?;
 // Use the exact same caption grouper as the registered RV-8 primitive before returning any ops.
 let ws=t.script_words().iter().map(|w|crate::transcript_captions::W{text:w.display.clone(),start:w.start,end:w.end}).collect::<Vec<_>>();crate::transcript_captions::group(&ws,&p)?;
 let mut ops=vec![];let track=if a.get("track_id").is_some(){let id=text(a,"track_id","")?;let tr=ss.project.tracks.iter().find(|t|t.id==id).ok_or("narration_section: missing caption track")?;if tr.locked{return Err("narration_section: caption track is locked".into());}literal(id)}else{
  ops.push(Op{id:"trk".into(),tool:"add_track".into(),args:json!({})});ops.push(Op{id:"track_name".into(),tool:"set_track".into(),args:json!({"track_id":"$trk.track_id","name":literal(text(a,"name_prefix","narration_section")?)})});"$trk.track_id".into()
 };
 let mut args=json!({"transcript_id":literal(&t.id),"track_id":track,"style":style});
 for k in ["font","max_lines","max_chars_per_line","max_cps","min_dur","max_dur","min_gap_frames"]{if let Some(v)=a.get(k){args[k]=if k=="font"{json!(literal(text(a,k,"")?))}else{v.clone()};}}
 ops.push(Op{id:"captions".into(),tool:"transcript_to_captions".into(),args});
 if a.get("music_track").is_some() {
  let id=text(a,"music_track","")?;let tr=ss.project.tracks.iter().find(|t|t.id==id).ok_or("narration_section: missing music track")?;
  if tr.locked || tr.clips.is_empty(){return Err("narration_section: music track must be unlocked and nonempty".into());}
  if a.get("track_id").and_then(|v|v.as_str())==Some(id){return Err("narration_section: music and caption tracks must differ".into());}
  let attack=num(a,"attack",0.1)?;let release=num(a,"release",0.3)?;
  if !(0.01..=2.0).contains(&attack)||!(0.01..=5.0).contains(&release){return Err("narration_section: attack must be .01..2 and release .01..5 seconds".into());}
  ops.push(Op{id:"ducking".into(),tool:"auto_duck".into(),args:json!({"track":literal(id),"speech_windows":speech_windows(ss,a)?,"amount_db":-18,"attack":attack,"release":release})});
 }else if a.get("attack").is_some()||a.get("release").is_some(){return Err("narration_section: attack/release need music_track".into());}
 Ok(ops)
}
pub fn commands()->Vec<Command>{vec![]}
