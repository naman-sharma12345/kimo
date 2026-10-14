use super::kit::*;
pub fn commands()->Vec<Command>{vec![Command{name:"particle_simulate_raw",mutates:false,description:"Raw fixed-step 3D particle/sphere trajectory: explicit initial state, acceleration/drag/attractor/vector force list and swept unit-normal planes. No preset emitter, project mutation or renderer integration. Returns initial plus step states.",schema:obj(vec![("state",json!({"type":"object"})),("forces",json!({"type":"array","maxItems":64})),("planes",json!({"type":"array","maxItems":16})),("dt",n("positive <=1 second fixed step")),("steps",json!({"type":"integer","minimum":0,"maximum":100000}))],&["state","forces","planes","dt","steps"]),run:|_,a|{
 let steps=a.get("steps").and_then(Value::as_u64).filter(|s|*s<=100000).ok_or("steps integer 0..100000")? as u32;
 let state=serde_json::from_value(a.get("state").ok_or("state required")?.clone()).map_err(|e|e.to_string())?;
 let forces:Vec<crate::particle_fields::Force>=serde_json::from_value(a.get("forces").ok_or("forces required")?.clone()).map_err(|e|e.to_string())?;
 let planes:Vec<crate::particle_fields::Plane>=serde_json::from_value(a.get("planes").ok_or("planes required")?.clone()).map_err(|e|e.to_string())?;
 if steps as u64+1>crate::resource_limits::current().max_samples{return Err("trajectory exceeds max_samples".into());}
 Ok(json!({"states":crate::particle_fields::simulate(state,&forces,&planes,num_arg(a,"dt")?,steps)?,"integrator":"fixed-step semi-implicit Euler; swept plane collisions at held step velocity","time_origin":0}))}}]}
