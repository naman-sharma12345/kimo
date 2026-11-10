use kimo_core::{commands::{Session,dispatch},ext::narration_section as ns,transcript::{Transcript,Take,TakeSource,TakeStatus,Word}};
use serde_json::{json,Value};
fn c(s:&mut Session,n:&str,a:Value)->Value{dispatch(s,n,&a).unwrap_or_else(|e|panic!("{n}: {e}"))}
fn bytes(s:&Session)->Vec<u8>{serde_json::to_vec(&s.project).unwrap()}
fn fixture()->Session{
 let mut s=Session::new();let mut t=Transcript{id:"tr1".into(),name:"narration".into(),version:1,..Default::default()};
 t.takes.push(Take{id:"take1".into(),source:TakeSource::default(),src_start:0.0,src_end:6.0,timeline_start:Some(10.0),speed:2.0,order:0,status:TakeStatus::Chosen,note:String::new()});
 t.takes.push(Take{id:"take2".into(),source:TakeSource::default(),src_start:0.0,src_end:1.0,timeline_start:Some(30.0),speed:1.0,order:1,status:TakeStatus::Rejected,note:String::new()});
 for (i,word,start,end) in [(1,"Hello",0.0,0.4),(2,"from",0.4,0.8),(3,"Kimo.",0.8,1.2),(4,"Keep",3.0,3.4),(5,"making",3.4,3.8),(6,"films.",3.8,4.2)]{t.words.push(Word::new(&format!("w{i}"),"take1",word,start,end,None));}
 t.words.push(Word::new("w7","take2","Ignored",0.0,0.8,None));t.derive_times();s.project.editorial.transcripts.insert(t.id.clone(),t);s
}
fn args()->Value{json!({"transcript_id":"tr1","font":"DejaVuSans","start":10.0,"min_dur":0.5})}
fn ops(s:&Session,a:&Value)->Value{json!(ns::expand(s,a).unwrap().iter().map(|o|json!({"id":o.id,"tool":o.tool,"args":o.args})).collect::<Vec<_>>())}
#[test]fn expansion_is_pure_and_exact_word_windows_respect_speed_pauses_and_rejected_takes(){
 let s=fixture();let b=bytes(&s);let a=args();assert_eq!(ops(&s,&a),ops(&s,&a));assert_eq!(bytes(&s),b);assert_eq!(ns::speech_windows(&s,&a).unwrap(),vec![[10.0,10.6],[11.5,12.1]]);
 assert!(ops(&s,&a).as_array().unwrap().iter().all(|o|!kimo_core::ext::batch::forbidden(o["tool"].as_str().unwrap())));
}
#[test]fn caption_batch_dry_run_ids_bound_words_edits_one_history_and_undo(){
 let mut s=fixture();let b=bytes(&s);let o=ops(&s,&args());let d=c(&mut s,"batch",json!({"ops":o,"dry_run":true,"return":"full"}));assert_eq!(bytes(&s),b);assert_eq!(s.undo.len(),0);
 let r=c(&mut s,"batch",json!({"ops":o,"expect_revision":d["revision"],"return":"full"}));assert_eq!(d["created"],r["created"]);assert_eq!(s.undo.len(),1);
 let rr=r["results"].as_array().unwrap().iter().find(|o|o["id"]=="captions").unwrap()["result"].clone();let gid=rr["caption_id"].as_str().unwrap();assert_eq!(s.project.editorial.captions[gid].transcript_id.as_deref(),Some("tr1"));assert!(!s.project.editorial.cue_words.is_empty());
 let cid=rr["clip_ids"][0].as_str().unwrap().to_string();assert_eq!(s.project.find_clip(&cid).unwrap().start,10.0);assert!(s.project.editorial.cue_words.values().all(|b|!b.word_ids.contains(&"w7".into())));
 let after=bytes(&s);c(&mut s,"undo",json!({}));assert_eq!(bytes(&s),b);c(&mut s,"redo",json!({}));assert_eq!(bytes(&s),after);
 c(&mut s,"transcript_edit_word",json!({"transcript_id":"tr1","word_id":"w1","display":"Hi"}));assert!(matches!(&s.project.find_clip(&cid).unwrap().source,kimo_core::model::Source::Text{text,..}if text.starts_with("Hi")));
}
#[test]fn invalid_params_drift_tracks_styles_and_text_errors_are_safe(){
 let mut s=fixture();let b=bytes(&s);for(k,v)in[("style",json!("fancy")),("start",json!(5.0)),("max_lines",json!(2.5)),("min_gap_frames",json!(-1)),("min_dur",json!(0)),("font",json!(9)),("transcript_id",json!("absent")),("srt_path",json!("/tmp/no-side-effect.srt"))]{let mut a=args();a[k]=v;assert!(ns::expand(&s,&a).is_err(),"{k}");assert_eq!(bytes(&s),b);}
 let t=c(&mut s,"add_track",json!({}))["track_id"].clone();c(&mut s,"set_track",json!({"track_id":t,"locked":true}));let mut a=args();a["track_id"]=t;assert!(ns::expand(&s,&a).unwrap_err().contains("locked"));
 s.project.editorial.transcripts.get_mut("tr1").unwrap().words[0].start+=0.1;let e=ns::expand(&s,&args()).unwrap_err();assert!(e.contains("drift"));assert!(!e.contains("Hello"));
}
#[test]fn schema_roles_and_existing_caption_track(){
 let mut s=fixture();assert_eq!(ns::VERSION,1);assert!(!ns::KEYWORDS.is_empty());assert!(!ns::SUMMARY.is_empty());let sc=ns::schema();assert_eq!(sc["required"],json!(["transcript_id"]));assert_eq!(sc["additionalProperties"],false);
 for(_,p)in sc["properties"].as_object().unwrap(){assert!(p["type"].is_string());assert!(p["description"].is_string());}
 let t=c(&mut s,"add_track",json!({}))["track_id"].clone();let mut a=args();a["track_id"]=t;let o=ops(&s,&a);assert_eq!(o.as_array().unwrap().len(),1);assert_eq!(o[0]["id"],"captions");c(&mut s,"batch",json!({"ops":o}));assert_eq!(s.project.tracks.len(),1);
}
#[test]fn apply_time_failure_rolls_back_caption_bindings_and_ids(){
 let mut s=fixture();let mut o=ops(&s,&args());o.as_array_mut().unwrap().push(json!({"tool":"delete_clip","args":{"clip_id":"missing"}}));let b=bytes(&s);assert!(dispatch(&mut s,"batch",&json!({"ops":o})).is_err());assert_eq!(bytes(&s),b);assert!(s.undo.is_empty());
}
fn music(s:&mut Session)->String {let t=c(s,"add_track",json!({}))["track_id"].as_str().unwrap().to_string();c(s,"add_shape",json!({"track_id":t,"shape":"rect","start":9.0,"duration":5.0}));t}
#[test]fn relative_minus18_duck_exact_word_windows_preserves_volume_and_dry_run(){
 let mut s=fixture();let t=music(&mut s);let mut a=args();a["music_track"]=json!(t);let o=ops(&s,&a);assert!(o.as_array().unwrap().iter().any(|o|o["id"]=="ducking"&&o["args"]["amount_db"]==-18));let b=bytes(&s);let u=s.undo.len();let d=c(&mut s,"batch",json!({"ops":o,"dry_run":true}));assert_eq!(bytes(&s),b);let r=c(&mut s,"batch",json!({"ops":o,"expect_revision":d["revision"]}));assert_eq!(r["created"],d["created"]);assert_eq!(s.undo.len(),u+1);
 let clip=&s.project.tracks.iter().find(|x|x.id==t).unwrap().clips[0];let p=&clip.audio.duck_gain;let target=10f64.powf(-18.0/20.0);assert!((p.at(1.2)-target).abs()<1e-12);assert!((p.at(2.7)-target).abs()<1e-12);assert_eq!(p.at(2.1),1.0);assert_eq!(p.at(0.0),1.0);assert_eq!(p.at(5.0),1.0);assert_eq!(clip.audio.volume.base,1.0);
 c(&mut s,"undo",json!({}));assert_eq!(bytes(&s),b);
}
#[test]fn windows_primitive_rejects_bad_input_and_handles_overlapping_windows_and_clip_edges(){
 let mut s=fixture();let t=music(&mut s);let b=bytes(&s);for w in [json!([[2,1]]),json!([[1]]),json!([[-1,1]]),json!([["x",1]])]{assert!(dispatch(&mut s,"auto_duck",&json!({"track":t,"speech_windows":w})).is_err());assert_eq!(bytes(&s),b);}
 c(&mut s,"auto_duck",json!({"track":t,"speech_windows":[[8.5,10.0],[9.5,10.5],[13.5,15.0]],"amount_db":-18}));let p=&s.project.tracks.iter().find(|x|x.id==t).unwrap().clips[0].audio.duck_gain;let g=10f64.powf(-18.0/20.0);assert!((p.at(0.0)-g).abs()<1e-12);assert!((p.at(5.0)-g).abs()<1e-12);assert_eq!(p.at(3.0),1.0);
}

#[test]fn empty_windows_clear_ducking_and_merge_report_has_counts_and_unity_outside(){
 let mut s=fixture();let t=music(&mut s);let r=c(&mut s,"auto_duck",json!({"track":t,"speech_windows":[[10,11],[10.5,11.5]],"amount_db":-18}));assert_eq!(r["merged_count"],1);assert_eq!(r["windows_used"],json!([[10.0,11.5]]));assert!(r["keyframes_created"].as_u64().unwrap()>0);
 let r=c(&mut s,"auto_duck",json!({"track":t,"speech_windows":[]}));assert_eq!(r["windows_used"],json!([]));assert_eq!(r["keyframes_created"],0);let p=&s.project.tracks.iter().find(|x|x.id==t).unwrap().clips[0].audio.duck_gain;assert!(p.keys.is_empty());assert_eq!(p.base,1.0);
}
#[test]fn windows_ignore_under_and_threshold_clamp_target_and_missing_under_keeps_error(){
 let mut s=fixture();let t=music(&mut s);let before=bytes(&s);let err=dispatch(&mut s,"auto_duck",&json!({"track":t})).unwrap_err();assert!(err.contains("under"));assert_eq!(bytes(&s),before);
 let r=c(&mut s,"auto_duck",json!({"track":t,"under":"nonexistent ignored","threshold_db":"ignored","speech_windows":[[1,2],[8,10],[13,16]]}));assert_eq!(r["under_ignored"],true);assert_eq!(r["threshold_ignored"],true);assert_eq!(r["windows_clamped"],3);assert_eq!(r["windows_used"],json!([[9.0,10.0],[13.0,14.0]]));
 let r=c(&mut s,"auto_duck",json!({"track":t,"speech_windows":[]}));assert_eq!(r["envelope_cleared"],true);
}
#[test]fn target_clip_gaps_drop_windows_and_clamping_reports_true_intersections(){
 let mut s=fixture();let t=music(&mut s);let id=s.project.tracks.iter().find(|x|x.id==t).unwrap().clips[0].id.clone();c(&mut s,"trim_clip",json!({"clip_id":id,"duration":1.0}));c(&mut s,"add_shape",json!({"track_id":t,"shape":"rect","start":13.0,"duration":1.0}));
 let r=c(&mut s,"auto_duck",json!({"track":t,"speech_windows":[[9.5,13.5],[11.0,12.0]],"amount_db":-18}));assert_eq!(r["windows_used"],json!([[9.5,10.0],[13.0,13.5]]));assert_eq!(r["windows_clamped"],1);assert_eq!(r["merged_count"],1);
 let r=c(&mut s,"auto_duck",json!({"track":t,"speech_windows":[[11.0,12.0]]}));assert_eq!(r["windows_clamped"],1);assert_eq!(r["envelope_cleared"],true);
}
#[test]fn invalid_window_index_error_is_structured_and_depth_is_within_half_db(){
 let mut s=fixture();let t=music(&mut s);let e=kimo_core::commands::dispatch_ex(&mut s,"auto_duck",&json!({"track":t,"speech_windows":[[10,11],[2,1]]})).unwrap_err();assert!(e.message.contains("speech_windows[1]"));
 c(&mut s,"auto_duck",json!({"track":t,"speech_windows":[[10,11]],"amount_db":-18}));let clip=&s.project.tracks.iter().find(|x|x.id==t).unwrap().clips[0];assert!((20.0*clip.audio.duck_gain.at(1.5).log10()+18.0).abs()<0.5);assert_eq!(clip.audio.duck_gain.at(2.5),1.0);
}
#[test]fn concurrent_sessions_caption_rehearsals_never_share_srt_scratch_files(){
 let barrier=std::sync::Arc::new(std::sync::Barrier::new(8));
 let workers=(0..8).map(|_|{let b=barrier.clone();std::thread::spawn(move||{let mut errors=vec![];for _ in 0..10{let mut s=fixture();let before=bytes(&s);let o=ops(&s,&args());b.wait();let r=dispatch(&mut s,"batch",&json!({"ops":o,"dry_run":true}));if r.is_err(){errors.push(format!("{r:?}"));}if bytes(&s)!=before{errors.push("dry run mutated state".into());}}assert!(errors.is_empty(),"concurrent failures: {errors:?}");})}).collect::<Vec<_>>();
 let results=workers.into_iter().map(|t|t.join()).collect::<Vec<_>>();assert!(results.iter().all(|r|r.is_ok()),"one or more concurrent sessions failed");
}
