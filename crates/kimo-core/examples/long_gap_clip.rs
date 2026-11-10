//! End-to-end command/decode proof: long_gap_clip video-path [end-frame]
use kimo_core::commands::{dispatch, Session};
use serde_json::json;
fn main() {
    let a: Vec<_> = std::env::args().collect();
    let mut s = Session::new();
    dispatch(
        &mut s,
        "project_set",
        &json!({"width":320,"height":240,"fps":25}),
    )
    .unwrap();
    let asset = dispatch(&mut s, "import_media", &json!({"path":a[1]})).unwrap();
    let track = dispatch(&mut s, "add_track", &json!({})).unwrap();
    let clip=dispatch(&mut s,"add_clip",&json!({"track_id":track["track_id"],"source_type":"media","asset_id":asset["asset_id"],"start":0,"duration":asset["duration"]})).unwrap();
    let before = serde_json::to_value(&s.project).unwrap();
    let mut args = json!({"clip_id":clip["clip_id"],"bbox":[118./320.,57./240.,82./320.,98./240.]});
    if let Some(f) = a.get(2) {
        args["end_frame"] = json!(f.parse::<usize>().unwrap());
    }
    if a.get(3).map(|v| v == "memory").unwrap_or(false) {
        args["appearance_memory"] = json!(true);
    }
    let now = std::time::Instant::now();
    let mut result = dispatch(&mut s, "track_long_gap", &args).unwrap();
    assert_eq!(before, serde_json::to_value(&s.project).unwrap());
    assert!(s.undo.len() > 0);
    result["seconds"] = json!(now.elapsed().as_secs_f64());
    println!("{result}");
}
