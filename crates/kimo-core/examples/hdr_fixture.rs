//! Fixture runner: raw little-endian f32 RGBA comes from an OUTSIDE-engine oracle.
use kimo_core::{
    color::pipeline::export_high,
    commands::{dispatch, Session},
};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 3 {
        return Err("usage: hdr_fixture <oracle.rgba32f> <output-directory>".into());
    }
    let bytes = std::fs::read(&a[1])?;
    if bytes.len() != 128 * 64 * 16 {
        return Err("fixture requires128x64 RGBA32f".into());
    }
    let rgba: Vec<_> = bytes
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    if rgba.iter().any(|v| !v.is_finite()) || rgba.chunks_exact(4).any(|p| p[3] != 1.0) {
        return Err("opaque finite fixture required".into());
    }
    let dir = std::path::Path::new(&a[2]);
    std::fs::create_dir_all(dir)?;
    let tiff = dir.join("oracle.tiff");
    tiff::encoder::TiffEncoder::new(std::fs::File::create(&tiff)?)?
        .write_image::<tiff::encoder::colortype::RGBA32Float>(128, 64, &rgba)?;
    let mut s = Session::new();
    dispatch(
        &mut s,
        "project_set",
        &json!({"width":128,"height":64,"fps":25}),
    )?;
    let asset = dispatch(&mut s, "import_media", &json!({"path":tiff}))?["asset_id"].clone();
    let track = dispatch(&mut s, "add_track", &json!({}))?["track_id"].clone();
    dispatch(
        &mut s,
        "add_clip",
        &json!({"track_id":track,"source_type":"media","asset_id":asset,"start":0,"duration":0.08,"fit":"stretch"}),
    )?;
    dispatch(
        &mut s,
        "set_color_pipeline",
        &json!({"settings":{"enabled":true,"working_gamut":1,"input_gamut":1,"input_transfer":0,"output_gamut":1,"output_transfer":6,"reference_white":100}}),
    )?;
    export_high(
        &s.project,
        dir.join("pq-lossless.mkv").to_str().unwrap(),
        "ffv1_16",
        0,
    )?;
    dispatch(
        &mut s,
        "set_hdr_mastering",
        &json!({"metadata":{"red":[0.68,0.32],"green":[0.265,0.69],"blue":[0.15,0.06],"white":[0.3127,0.329],"min_nits":0.005,"max_nits":1000,"max_cll":1000,"max_fall":400}}),
    )?;
    export_high(
        &s.project,
        dir.join("pq-hevc.mp4").to_str().unwrap(),
        "hevc10",
        0,
    )?;
    Ok(())
}
