use super::kit::*;
fn read<T: serde::de::DeserializeOwned>(a: &Value, k: &str) -> Result<T, String> { serde_json::from_value(a.get(k).ok_or_else(|| format!("{k} required"))?.clone()).map_err(|e| e.to_string()) }
pub fn commands() -> Vec<Command> {
    vec![Command { name: "render_particle_flow", description: "Render a deterministic seeded particle emitter advected by a 3D curl, potential or Worley flow field. Deterministic seeded particle emitter advected by a curl3/potential3/worley3 field (semi-implicit Euler, gravity, drag, field-velocity coupling), recycled through a preallocated ring pool (no per-frame heap allocation), smoothstep fade-in/out, cubic-eased size bump (small at birth and death, peak mid-life), multi-stop gradient color; camera-v2 perspective depth-splat with distance-attenuated footprint, radial gaussian kernel of configurable hardness, additive or premultiplied-alpha blending, tiled inner loop, z-buffer occlusion against optional mesh depth; PNG via the CPU image encoder. Inputs flow {emitter,field,gravity,drag,coupling,dt}, camera, time, width, height, optional mesh, depth_range, path. Budget: 1..100000 particles, dt (0,1], 2..=8 color stops, target 1..1920x1080. Limits: correctness-first CPU path; no motion blur, trails, or fluid coupling.", mutates: false, schema: obj(vec![("flow", json!({"type":"object"})), ("camera", json!({"type":"object"})), ("time", json!({"type":"number"})), ("width", json!({"type":"integer"})), ("height", json!({"type":"integer"})), ("mesh", json!({"type":"object"})), ("depth_range", json!({"type":"array"})), ("path", s("PNG output"))], &["flow", "camera", "time", "width", "height", "path"]), run: |_, a| {
        let flow: crate::particle_flow::Flow = read(a, "flow")?;
        let camera: crate::camera_projection::Camera = read(a, "camera")?;
        let time = num_arg(a, "time")?;
        let w = num_arg(a, "width")? as usize;
        let h = num_arg(a, "height")? as usize;
        let path = str_arg(a, "path")?;
        if !path.to_lowercase().ends_with(".png") {
            return Err("PNG path required".into());
        }
        let mesh: Option<crate::threed::primitives::Geometry> = if a.get("mesh").map(|m| m.is_null()).unwrap_or(true) { None } else { Some(read(a, "mesh")?) };
        let dr: Vec<f64> = if a.get("depth_range").map(|m| m.is_null()).unwrap_or(true) { vec![0.5, 100.0] } else { read(a, "depth_range")? };
        if dr.len() != 2 {
            return Err("depth_range [near,far]".into());
        }
        let mut sim = crate::particle_flow::Simulation::new(flow)?;
        sim.advance(time)?;
        let (frame, out) = crate::particle_flow::render(&mut sim, &camera, w, h, mesh.as_ref(), [dr[0], dr[1]])?;
        crate::render::write_image(&frame, path)?;
        Ok(json!({"path":path,"active":out.active,"drawn":out.drawn,"occluded_by_mesh":out.occluded_by_mesh,"behind_camera":out.behind_camera,"sim_time":sim.time(),"backend":"cpu_camera_v2_depth_splat"}))
    } }]
}
