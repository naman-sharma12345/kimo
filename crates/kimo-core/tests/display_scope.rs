use kimo_core::commands::{dispatch, Session};
use serde_json::json;
#[test]
fn command_writes_png_preserves_project_and_uses_output_domain() {
    let mut session = Session::new();
    dispatch(
        &mut session,
        "project_set",
        &json!({"width":16,"height":16}),
    )
    .unwrap();
    let track = dispatch(&mut session, "add_track", &json!({})).unwrap()["track_id"].clone();
    dispatch(&mut session,"add_clip",&json!({"track_id":track,"source_type":"solid","color":[1.,0.,0.],"start":0.,"duration":1.})).unwrap();
    let path = std::env::temp_dir().join(format!("kimo-display-scope-{}.png", std::process::id()));
    let args = json!({"time":0.,"scope":"waveform","path":path.to_str().unwrap()});
    let before = serde_json::to_value(&session.project).unwrap();
    let result = dispatch(&mut session, "render_display_scope", &args).unwrap();
    assert_eq!(result["scope"]["visible_pixels"], 256);
    assert!((result["scope"]["max"][3].as_f64().unwrap() - 21.26).abs() < 1e-5);
    assert_eq!(result["scope"]["encoding"], "Bt709");
    assert_eq!(serde_json::to_value(&session.project).unwrap(), before);
    assert_eq!(&std::fs::read(&path).unwrap()[..8], b"\x89PNG\r\n\x1a\n");
    session.project.color_pipeline.enabled = true;
    session.project.color_pipeline.output_gamut = 1;
    session.project.color_pipeline.output_transfer = 6;
    session.project.color_pipeline.tone_map = false;
    let before = serde_json::to_value(&session.project).unwrap();
    let result = dispatch(&mut session, "render_display_scope", &args).unwrap();
    assert_eq!(result["scope"]["gamut"], "REC2020");
    assert_eq!(result["scope"]["encoding"], "Pq");
    assert_eq!(result["scope"]["units"], "NITS");
    assert_eq!(result["scope"]["ceiling"], 10000.);
    assert_eq!(serde_json::to_value(&session.project).unwrap(), before);
    let original = std::fs::read(&path).unwrap();
    for bad in [
        json!({"ceiling":0}),
        json!({"ceiling":"100"}),
        json!({"units":"bad"}),
        json!({"scope":"bad"}),
        json!({"time":-1}),
        json!({"clip_id":32}),
        json!({"hlg_system_gamma":3}),
    ] {
        let mut a = args.clone();
        for (k, v) in bad.as_object().unwrap() {
            a[k] = v.clone();
        }
        assert!(dispatch(&mut session, "render_display_scope", &a).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
    session.project.color_pipeline.output_gamut = 2;
    assert!(dispatch(&mut session, "render_display_scope", &args)
        .unwrap_err()
        .contains("Rec709 or Rec2020"));
    std::fs::remove_file(path).unwrap();
}
