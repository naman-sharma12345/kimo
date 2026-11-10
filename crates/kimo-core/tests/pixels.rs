use kimo_core::{
    commands::{dispatch, Session},
    effects::Frame,
    pixels,
};
use serde_json::json;
#[test]
fn native_storage_preserves_every_float_bit() {
    let f = Frame {
        w: 2,
        h: 1,
        d: vec![-1.25, 3.5, 0.1, 1., 2., -0., f32::MIN_POSITIVE, 0.5],
    };
    let b = pixels::encode(&f).unwrap();
    let g = pixels::decode(&b).unwrap();
    assert_eq!(
        f.d.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        g.d.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
    );
}
#[test]
fn read_equals_render_import_roundtrip() {
    let mut s = Session::new();
    dispatch(&mut s, "project_set", &json!({"width":16,"height":16})).unwrap();
    let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    dispatch(
        &mut s,
        "add_clip",
        &json!({"track_id":t,"source_type":"solid","start":0,"duration":1,"color":[0.125,0.3,0.8]}),
    )
    .unwrap();
    let expected =
        kimo_core::render::render_frame(&s.project, 0.5, &kimo_core::render::Cache::new()).unwrap();
    let r = dispatch(&mut s, "read_pixels", &json!({"time":0.5})).unwrap();
    let got: Vec<f32> = serde_json::from_value(r["data"].clone()).unwrap();
    assert_eq!(expected.d, got);
    let r = dispatch(&mut s, "read_pixels", &json!({"time":0.5,"format":"file"})).unwrap();
    let a = dispatch(
        &mut s,
        "import_pixels",
        &json!({"w":16,"h":16,"format":"kimopx","path":r["path"]}),
    )
    .unwrap();
    s.project.tracks.clear();
    let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    dispatch(&mut s,"add_clip",&json!({"track_id":t,"source_type":"media","asset_id":a["asset_id"],"start":0,"duration":1})).unwrap();
    let r = dispatch(&mut s, "read_pixels", &json!({"time":0.5})).unwrap();
    let got: Vec<f32> = serde_json::from_value(r["data"].clone()).unwrap();
    assert_eq!(expected.d, got);
    let r = dispatch(
        &mut s,
        "read_pixels",
        &json!({"time":0.5,"source":{"asset_id":a["asset_id"]}}),
    )
    .unwrap();
    assert_eq!(r["data"], json!(expected.d));
}
#[test]
fn bounded_errors_and_b64_fuzz() {
    for s in ["", "=AAA", "AA=A", "AAAA====", "AB==", "A?AA", "A"] {
        let _ = pixels::b64_decode(s);
    }
    for i in 0..1000 {
        let b = vec![(i % 256) as u8; i % 300];
        let _ = pixels::decode(&b);
    }
    let mut s = Session::new();
    for a in [
        json!({"time":0,"points":[[99999,0]]}),
        json!({"time":0,"max_pixels":1}),
        json!({"time":0,"region":{"x":0,"y":0,"w":0,"h":1}}),
    ] {
        assert!(dispatch(&mut s, "read_pixels", &a).is_err());
    }
}
#[test]
fn roi_transport_and_crop() {
    let f = Frame {
        w: 4,
        h: 3,
        d: (0..12).flat_map(|i| [i as f32, -1., 2., 1.]).collect(),
    };
    let p = std::env::temp_dir().join(format!("roi-{}.kimopx", std::process::id()));
    std::fs::write(&p, pixels::encode(&f).unwrap()).unwrap();
    let mut s = Session::new();
    let a = dispatch(
        &mut s,
        "import_pixels",
        &json!({"w":4,"h":3,"format":"kimopx","path":p,"region":{"x":1,"y":1,"w":2,"h":1}}),
    )
    .unwrap();
    assert_eq!((a["w"].as_u64(), a["h"].as_u64()), (Some(2), Some(1)));
    let r = dispatch(
        &mut s,
        "read_pixels",
        &json!({"time":0,"source":{"asset_id":a["asset_id"]},"points":[[1,0]]}),
    )
    .unwrap();
    assert_eq!(r["data"], json!([6., -1., 2., 1.]));
    let r = dispatch(
        &mut s,
        "read_pixels",
        &json!({"time":0,"source":{"asset_id":a["asset_id"]},"region":{"x":0,"y":0,"w":1,"h":1}}),
    )
    .unwrap();
    assert_eq!(r["data"], json!([5., -1., 2., 1.]));
    std::fs::remove_file(p).unwrap();
}
#[test]
fn max_pixels_knob_error() {
    let mut s = Session::new();
    dispatch(&mut s, "set_limits", &json!({"max_pixels":1})).unwrap();
    let e = kimo_core::commands::dispatch_ex(&mut s, "read_pixels", &json!({"time":0}))
        .err()
        .unwrap();
    assert!(e.message.contains("max_pixels") && e.message.contains("set_limits"));
}
#[test]
fn import_undo_roundtrip() {
    let f = Frame {
        w: 1,
        h: 1,
        d: vec![2., -1., 0.25, 1.],
    };
    let path = std::env::temp_dir().join(format!("pixels-undo-{}.kimopx", std::process::id()));
    std::fs::write(&path, pixels::encode(&f).unwrap()).unwrap();
    let mut s = Session::new();
    let before = serde_json::to_value(&s.project).unwrap();
    let r = dispatch(
        &mut s,
        "import_pixels",
        &json!({"w":1,"h":1,"format":"kimopx","path":path}),
    )
    .unwrap();
    let after = serde_json::to_value(&s.project).unwrap();
    dispatch(&mut s, "undo", &json!({})).unwrap();
    assert_eq!(serde_json::to_value(&s.project).unwrap(), before);
    dispatch(&mut s, "redo", &json!({})).unwrap();
    assert_eq!(serde_json::to_value(&s.project).unwrap(), after);
    assert!(s
        .project
        .assets
        .contains_key(r["asset_id"].as_str().unwrap()));
    std::fs::remove_file(path).unwrap();
}
#[test]
fn storage_project_priority_and_guard_limits() {
    let mut s = Session::new();
    let base = std::env::temp_dir().join(format!("kimo-store-{}", std::process::id()));
    s.project.base_dir = Some(base.clone());
    let dir = kimo_core::assets::store_dir(&s.project, "pixels").unwrap();
    assert_eq!(dir, base.join("assets/pixels"));
    assert!(kimo_core::assets::store_dir(&s.project, "../escape").is_err());
    let mut l = kimo_core::resource_limits::Limits::defaults();
    l.max_pixels = 7;
    l.max_samples = 9;
    let _g = kimo_core::resource_limits::install(l);
    assert_eq!(kimo_core::guard::effective_limits().max_pixels, 7);
    assert_eq!(kimo_core::guard::effective_limits().max_samples, 9);
    std::fs::remove_dir_all(base).unwrap();
}
