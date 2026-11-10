//! Additional QA invariants. Fixed-seed generation keeps every red reproducible.
use kimo_core::{transcript_parse::{parse,parse_with,Opts,OVERLAP_TOL},transcript_captions::{self as cap,W,Params},commands::{Session,dispatch}};
use serde_json::json;
#[test] fn timing_overlap_property() {
 for seed in 0..500u32 { let overlap=(seed%101) as f64/100.; let b=format!("word,start,end\na,0,1\nb,{},2\n",1.-overlap);
 let strict=parse("csv",b.as_bytes()); if overlap>OVERLAP_TOL {assert_eq!(strict.unwrap_err().code,"overlap");} else {assert!(strict.is_ok());}
 let p=parse_with("csv",b.as_bytes(),Opts{lossy_utf8:false,clamp_overlaps:true}).unwrap(); assert!(p.words[0].end<=p.words[1].start+OVERLAP_TOL); assert_eq!(!p.warnings.is_empty(),overlap>OVERLAP_TOL);
 }
}
#[test] fn subtitle_text_goldens_and_entity_roundtrip() {
 let c=vec![(1.25,2.5,"A & B <literal>".into())];
 assert_eq!(cap::to_srt(&c),"1\n00:00:01,250 --> 00:00:02,500\nA & B <literal>\n\n");
 let v=cap::to_vtt(&c); assert_eq!(v,"WEBVTT\n\n00:00:01.250 --> 00:00:02.500\nA &amp; B &lt;literal>\n\n");
 let p=parse("vtt",v.as_bytes()).unwrap(); assert_eq!(p.words.iter().map(|w|w.text.as_str()).collect::<Vec<_>>(),vec!["A","&","B","<literal>"]);
}
#[test] fn caption_group_never_overlaps_generated_adjacent_sentences() {
 for n in 1..100 { let dt=n as f64/10000.; let w=vec![W{text:"One.".into(),start:0.,end:dt},W{text:"Two.".into(),start:dt,end:2.*dt}];let p=Params::new(2.,25.);let c=cap::group(&w,&p).unwrap();for q in c.windows(2){assert!(q[0].end<=q[1].start,"dt={dt} {c:?}");}}
}
#[test] fn preview_parameter_edges_never_panic() {
 let mut s=Session::new();dispatch(&mut s,"project_set",&json!({"width":16,"height":16,"fps":25})).unwrap();
 for (i,v) in [json!(-1e300),json!(1e300),json!(-1),json!(0),json!(1),json!("bad"),json!(null),json!({}),json!([])].into_iter().enumerate(){
 let p=format!("/tmp/qa-frame-{}-{i}.png",std::process::id());let r=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||dispatch(&mut s,"render_frame",&json!({"time":v,"path":p})))); assert!(r.is_ok(),"render_frame {v}");
 let r=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||dispatch(&mut s,"contact_sheet",&json!({"times":[v],"path":p,"tile_px":v,"cols":v,"n":v}))));assert!(r.is_ok(),"contact_sheet {v}");let _=std::fs::remove_file(p);
 }
 for v in [json!(0),json!(-1),json!(1e300),json!("bad"),json!(null)]{let r=dispatch(&mut s,"transcript_edit_word",&json!({"from_index":v,"to_index":v,"display":"x"}));assert!(r.is_err());}
}

#[test] fn vtt_entities_decode_once_after_tags_and_sanitize() {
 let body=b"WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n<b>x</b> &amp;lt; &lt;b&gt; &nbsp; &lrm; &rlm; &unknown;\n";
 let p=parse("vtt",body).unwrap(); assert_eq!(p.words.iter().map(|w|w.text.as_str()).collect::<Vec<_>>(),vec!["x","&lt;","<b>","&unknown;"]);
}
#[test] fn subtitle_roundtrip_timing_property() {
 for i in 0..200 {let a=i as f64/10.;let b=a+0.25;let cues=vec![(a,b,"hello".into())];for (fmt,body) in [("srt",cap::to_srt(&cues)),("vtt",cap::to_vtt(&cues))]{let p=parse(fmt,body.as_bytes()).unwrap();assert!((p.words[0].start-a).abs()<0.000501);assert!((p.words[0].end-b).abs()<0.000501);}}
}
#[test] fn parser_pathologicals_are_structured_and_never_panic() {
 let bodies=vec!["word,start,end\nx,-1,0\n".to_string(),"word,start,end\nx,1,1\n".into(),"word,start,end\nx,2,1\n".into(),format!("word,start,end\n{},0,1\n","x".repeat(10_001)),format!("word,start,end\n{},0,1\n","🧑🏽‍💻中é".repeat(3_000)),"word,start,end\nx,0,1\ny,0.5,2\n".into(),"1\n00:00:00,000 --> 00:00:00,000\n🧑🏽‍💻 中文 café\n".into()];
 for body in bodies {for fmt in ["auto","srt","vtt","csv","json"] {for opts in [Opts::default(),Opts{lossy_utf8:true,clamp_overlaps:true}] {
 let result=std::panic::catch_unwind(||parse_with(fmt,body.as_bytes(),opts));assert!(result.is_ok(),"format {fmt}");if let Err(e)=result.unwrap(){assert!(!e.code.is_empty()&&!e.message.is_empty());}
 }}}
}
