// PL1b depth-only pre-pass for directional shadow maps. Same vertex layout as pbr_gpu.wgsl;
// no fragment stage (depth write only).
struct DScene { shadow_vp:mat4x4<f32> };
struct DDraw { world:mat4x4<f32>, normal:mat4x4<f32> };
@group(0) @binding(0) var<uniform> dscene:DScene;
@group(1) @binding(0) var<uniform> ddraw:DDraw;
@vertex fn vs_depth(@location(0) p:vec3<f32>)->@builtin(position) vec4<f32> {
 return dscene.shadow_vp*(ddraw.world*vec4<f32>(p,1.0));
}
