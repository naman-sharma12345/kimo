//! RV-12 lower_third primitive expansion, dry-run, deterministic ids, history and safety.
use kimo_core::{commands::{dispatch,Session},ext::lower_third,keyframe::Ease,model::Source};
use serde_json::{json,Value};
fn call(s:&mut Session,n:&str,a:Value)->Value { dispatch(s,n,&a).unwrap_or_else(|e|panic!("{n}: {e}")) }
fn bytes(s:&Session)->Vec<u8> { serde_json::to_vec(&s.project).unwrap() }
fn params()->Value { json!({"title":"Ada","subtitle":"Engineer","duration":4.0,"start":7.0,"font":"DejaVuSans","width":600,"height":180,"title_size":32,"subtitle_size":22,"x":-700,"y":300}) }
fn ops(s:&Session,a:&Value)->Value { json!(lower_third::expand(s,a).unwrap().iter().map(|o|json!({"id":o.id,"tool":o.tool,"args":o.args})).collect::<Vec<_>>()) }
#[test] fn pure_expansion_dry_run_revision_real_ids_and_undo_redo() {
 let mut s=Session::new();let p=params();let b=bytes(&s);let u=s.undo.len();let o=ops(&s,&p);assert_eq!(b,bytes(&s));assert_eq!(o,ops(&s,&p));
 assert!(o.as_array().unwrap().iter().all(|v|!kimo_core::ext::batch::forbidden(v["tool"].as_str().unwrap())));
 let d=call(&mut s,"batch",json!({"ops":o,"dry_run":true,"return":"full"}));assert_eq!(b,bytes(&s));assert_eq!(u,s.undo.len());assert!(d["warnings"].as_array().unwrap().is_empty());
 let r=call(&mut s,"batch",json!({"ops":o,"expect_revision":d["revision"],"return":"full"}));assert_eq!(d["created"],r["created"]);assert_eq!(r["created"]["clip_ids"].as_array().unwrap().len(),4);assert_eq!(s.undo.len(),u+1);
 let after=bytes(&s);call(&mut s,"undo",json!({}));assert_eq!(bytes(&s),b);call(&mut s,"redo",json!({}));assert_eq!(bytes(&s),after);
}
#[test] fn easing_is_on_departing_keys_and_times_are_clip_local() {
 let mut s=Session::new();let p=params();let o=ops(&s,&p);call(&mut s,"batch",json!({"ops":o}));
 for c in &s.project.tracks[0].clips {
  assert_eq!(c.start,7.0);assert_eq!(c.duration,4.0);
  for prop in ["opacity","y"] {let k=&c.props[prop].keys;assert_eq!(k.iter().map(|k|k.t).collect::<Vec<_>>(),vec![0.0,0.4,3.6,4.0]);assert_eq!(k[0].ease,Ease::OutQuart);assert_eq!(k[1].ease,Ease::Hold);assert_eq!(k[2].ease,Ease::In);}
  assert_eq!(c.props["opacity"].at(0.0),0.0);assert_eq!(c.props["opacity"].at(2.0),1.0);assert_eq!(c.props["opacity"].at(4.0),0.0);
 }
}
#[test] fn primitive_individual_apply_equals_batch_exact_bytes() {
 let a=params();let mut s=Session::new();let o=lower_third::expand(&s,&a).unwrap();let mut ids=std::collections::HashMap::new();
 fn subst(a:&Value,ids:&std::collections::HashMap<String,Value>)->Value {
  match a { Value::String(v) if v.starts_with("$$")=>json!(&v[1..]),Value::String(v) if v.starts_with('$')=>{let (op,k)=v[1..].split_once('.').unwrap();ids[op][k].clone()},Value::Array(a)=>json!(a.iter().map(|v|subst(v,ids)).collect::<Vec<_>>()),Value::Object(a)=>Value::Object(a.iter().map(|(k,v)|(k.clone(),subst(v,ids))).collect()),_=>a.clone() }
 }
 for op in o { let r=call(&mut s,&op.tool,subst(&op.args,&ids));ids.insert(op.id,r); }
 let mut b=Session::new();let o=ops(&b,&a);call(&mut b,"batch",json!({"ops":o}));assert_eq!(bytes(&s),bytes(&b));
}
#[test] fn optional_subtitle_existing_track_and_literal_dollars() {
 let mut s=Session::new();let t=call(&mut s,"add_track",json!({}))["track_id"].clone();let mut a=params();a["track_id"]=t;a["subtitle"]=json!("");a["title"]=json!("$Ada");let o=ops(&s,&a);
 assert!(!o.as_array().unwrap().iter().any(|v|v["tool"]=="add_track"));call(&mut s,"batch",json!({"ops":o}));assert_eq!(s.project.tracks.len(),1);assert_eq!(s.project.tracks[0].clips.len(),3);
 assert!(s.project.tracks[0].clips.iter().any(|c|matches!(&c.source,Source::Text{text,..} if text=="$Ada")));
}
#[test] fn styling_colors_alpha_layout_and_custom_easing_are_preserved() {
 let mut s=Session::new();let mut a=params();a["accent"]=json!([0.1,0.2,0.3,0.6]);a["background"]=json!([0.2,0.3,0.4,0.5]);a["enter_ease"]=json!("out_expo");a["exit_ease"]=json!("in_expo");a["slide"]=json!(20.0);
 let o=ops(&s,&a);call(&mut s,"batch",json!({"ops":o}));let clips=&s.project.tracks[0].clips;assert_eq!(clips[0].props["fill_alpha"].base,0.5);assert_eq!(clips[1].props["fill_alpha"].base,0.6);
 assert!(matches!(&clips[1].source,Source::Shape{fill,..} if *fill==[0.1,0.2,0.3]));assert_eq!(clips[0].props["y"].at(0.0),320.0);assert_eq!(clips[0].props["y"].at(1.0),300.0);assert_eq!(clips[0].props["y"].keys[0].ease,Ease::OutExpo);assert_eq!(clips[0].props["y"].keys[2].ease,Ease::InExpo);
}
#[test] fn invalid_params_fit_missing_tracks_and_locks_never_mutate() {
 let mut s=Session::new();let b=bytes(&s);
 for (k,v) in [("title",json!(" ")),("duration",json!(0)),("enter",json!(3.7)),("exit",json!(-1)),("start",json!(-1)),("width",json!(0)),("padding",json!(800)),("accent",json!([2,0,0])),("title_size",json!("big")),("enter_ease",json!("bogus")),("track_id",json!("missing")),("typo",json!(1)),("title",json!("X".repeat(200)))] {
  let mut a=params();a[k]=v;assert!(lower_third::expand(&s,&a).is_err(),"{k}");assert_eq!(b,bytes(&s));
 }
 let t=call(&mut s,"add_track",json!({}))["track_id"].clone();call(&mut s,"set_track",json!({"track_id":t,"locked":true}));let mut a=params();a["track_id"]=t;let b=bytes(&s);assert!(lower_third::expand(&s,&a).unwrap_err().contains("locked"));assert_eq!(bytes(&s),b);
}
#[test] fn schema_metadata_roles_and_track_name_are_consistent() {
 assert_eq!(lower_third::VERSION,1);assert!(!lower_third::KEYWORDS.is_empty());assert!(!lower_third::SUMMARY.is_empty());
 let schema=lower_third::schema();assert_eq!(schema["type"],"object");assert_eq!(schema["required"],json!(["title"]));assert_eq!(schema["additionalProperties"],false);assert!(schema["space"].as_str().unwrap().contains("clip-local"));
 for (_,p) in schema["properties"].as_object().unwrap() {assert!(p["type"].is_string());assert!(p["description"].is_string());}
 let mut s=Session::new();let mut a=params();a["name_prefix"]=json!("$speaker");let o=ops(&s,&a);assert_eq!(o[0]["id"],"trk");assert_eq!(o[0]["tool"],"add_track");
 for role in ["bar","accent","title","subtitle"] {assert!(o.as_array().unwrap().iter().any(|x|x["id"]==role));}
 call(&mut s,"batch",json!({"ops":o}));assert_eq!(s.project.tracks[0].name,"$speaker");
}
#[test] fn defaults_rehearse_and_bad_easing_has_allowed_values_not_title() {
 let mut s=Session::new();let o=ops(&s,&json!({"title":"Ada","subtitle":"Engineer","font":"DejaVuSans"}));let b=bytes(&s);
 let d=call(&mut s,"batch",json!({"ops":o,"dry_run":true}));assert_eq!(d["ok"],true);assert_eq!(bytes(&s),b);
 let mut a=params();a["title"]=json!("PRIVATE TITLE DO NOT ECHO");a["enter_ease"]=json!("wobble");let m=lower_third::expand(&s,&a).unwrap_err();assert!(m.contains("Valid:"));assert!(!m.contains("PRIVATE TITLE"));
}

#[test] fn title_rgba_survives_animated_opacity_and_overlapping_lines_fail() {
 let mut s=Session::new();let mut a=params();a["title_color"]=json!([0.4,0.5,0.6,0.3]);a["subtitle_color"]=json!([1,1,1,0.2]);let o=ops(&s,&a);call(&mut s,"batch",json!({"ops":o}));assert_eq!(s.project.tracks[0].clips[2].props["opacity"].at(2.0),0.3);assert_eq!(s.project.tracks[0].clips[3].props["opacity"].at(2.0),0.2);
 let mut a=params();a["title"]=json!("Ada\\nEngineer\\nSpeaker");a["title_size"]=json!(40);assert!(lower_third::expand(&s,&a).is_err());
}
