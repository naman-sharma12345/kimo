use super::kit::*;
fn read<T: serde::de::DeserializeOwned>(a: &Value, k: &str) -> Result<T, String> { serde_json::from_value(a.get(k).ok_or_else(|| format!("{k} required"))?.clone()).map_err(|e| e.to_string()) }
pub fn commands() -> Vec<Command> {
    vec![Command { name: "render_particle_flow_motion", description: "Styled particle flow: velocity-stretched splats and ribbon trails on top of render_particle_flow. Styled variant of render_particle_flow: oriented splats stretched along screen-space velocity proportional to speed and shutter seconds (gaussian falloff preserved on major/minor axes), ribbon trails from the fixed 6-step history ring with alpha falloff for particles above trail_speed, HDR f32 accumulation (no clamp), optional reuse of the existing multi-scale bloom effect, then exposure and an exponential Reinhard tone map (fixes dark linear output). Optional lights: up to 8 directional (world direction, half-lambert wrap) + 8 point (inverse-square with smooth radius cutoff) on sphere-impostor splat normals plus ambient, for a soft volumetric look; flow.field_accel adds the sampled field as a direct force before the stretch read. Lights shade the mesh only through occlusion (mesh keeps its normal colors). Optional obstacle: when the flat-sah feature is built and obstacle=true, the mesh also deflects particles by reflection about interpolated vertex normals with restitution (normal) and friction (tangential slide, no ping-pong); 3D surface, not screen-depth. Inputs flow, camera, time, width, height, style {shutter,trail_speed,exposure,tonemap,bloom}, optional mesh, depth_range, damping, path. Budget: 1..100000 particles, streak major axis <=192px, target 1..1920x1080. Limits: correctness-first CPU path; trails are screen-space polylines not true ribbons; no temporal coherence claim; near-degenerate obstacle geometry (pole slivers) can tunnel.", mutates: false, schema: obj(vec![("flow", json!({"type":"object"})), ("camera", json!({"type":"object"})), ("time", json!({"type":"number"})), ("width", json!({"type":"integer"})), ("height", json!({"type":"integer"})), ("style", json!({"type":"object"})), ("lights", json!({"type":"object"})), ("mesh", json!({"type":"object"})), ("obstacle", json!({"type":"boolean"})), ("restitution", json!({"type":"number"})), ("friction", json!({"type":"number"})), ("depth_range", json!({"type":"array"})), ("path", s("PNG output"))], &["flow", "camera", "time", "width", "height", "path"]), run: |_, a| {
        let flow: crate::particle_flow::Flow = read(a, "flow")?;
        let camera: crate::camera_projection::Camera = read(a, "camera")?;
        let time = num_arg(a, "time")?;
        let w = num_arg(a, "width")? as usize;
        let h = num_arg(a, "height")? as usize;
        let path = str_arg(a, "path")?;
        if !path.to_lowercase().ends_with(".png") {
            return Err("PNG path required".into());
        }
        let style: crate::particle_flow::Style = if a.get("style").map(|m| m.is_null()).unwrap_or(true) { Default::default() } else { read(a, "style")? };
        let mesh: Option<crate::threed::primitives::Geometry> = if a.get("mesh").map(|m| m.is_null()).unwrap_or(true) { None } else { Some(read(a, "mesh")?) };
        let obstacle = a.get("obstacle").and_then(|v| v.as_bool()).unwrap_or(false);
        let restitution = opt_num(a, "restitution", 0.5);
        let friction = opt_num(a, "friction", 0.7);
        #[cfg(not(feature = "flat-sah"))]
        let _ = (restitution, friction);
        let lights: Option<crate::particle_flow::Lights> = if a.get("lights").map(|m| m.is_null()).unwrap_or(true) { None } else { Some(read(a, "lights")?) };
        let dr: Vec<f64> = if a.get("depth_range").map(|m| m.is_null()).unwrap_or(true) { vec![0.5, 100.0] } else { read(a, "depth_range")? };
        if dr.len() != 2 {
            return Err("depth_range [near,far]".into());
        }
        let mut sim = crate::particle_flow::Simulation::new(flow)?;
        #[cfg(feature = "flat-sah")]
        if obstacle {
            let g = mesh.as_ref().ok_or("obstacle requires mesh")?;
            let stream = crate::threed::vertex_stream::Stream { vertices: g.vertices.clone(), indices: g.indices.clone() };
            let index = crate::threed::flat_sah::Index::build(&stream)?;
            crate::particle_flow::advance_deflect(&mut sim, time, &index, &stream, restitution, friction)?;
        }
        #[cfg(not(feature = "flat-sah"))]
        if obstacle {
            return Err("obstacle deflection requires the flat-sah feature".into());
        }
        if !obstacle {
            sim.advance(time)?;
        }
        let (frame, out) = crate::particle_flow::render_styled(&mut sim, &camera, w, h, mesh.as_ref(), [dr[0], dr[1]], &style, lights.as_ref())?;
        crate::render::write_image(&frame, path)?;
        Ok(json!({"path":path,"active":out.active,"drawn":out.drawn,"occluded_by_mesh":out.occluded_by_mesh,"behind_camera":out.behind_camera,"sim_time":sim.time(),"backend":"cpu_camera_v2_depth_splat_hdr"}))
    } }]
}
