// Original equations match threed::pbr::sample and texture_material channel policy.
// PL1b: directional shadow map (depth pre-pass + PCF). PCF compare-then-average structure
// and comparison-sampler usage adapted from Bevy (github.com/bevyengine/bevy,
// crates/bevy_pbr/src/render/shadow_sampling.wgsl; MIT OR Apache-2.0). Bias constants and
// the bias contract are pinned in threed/shadow.rs and arrive as uniforms (no drift).
struct Light { vector_kind:vec4<f32>, rgb_min:vec4<f32> };
struct Scene { view_proj:mat4x4<f32>, eye:vec4<f32>, ambient:vec4<f32>, time_count:vec4<f32>, shadow_vp:mat4x4<f32>, shadow_params:vec4<f32>, shadow_texel:vec4<f32>, lights:array<Light,64> };
struct Draw { world:mat4x4<f32>, normal:mat4x4<f32> };
struct Material { base:vec4<f32>, factors:vec4<f32>, flags:vec4<f32> };
@group(0) @binding(0) var<uniform> scene:Scene;
@group(0) @binding(1) var shadow_tex: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(1) @binding(0) var<uniform> draw:Draw;
@group(2) @binding(0) var<uniform> mat:Material;
@group(2) @binding(1) var albedo_tex:texture_2d<f32>;
@group(2) @binding(2) var albedo_sampler:sampler;
@group(2) @binding(3) var normal_tex:texture_2d<f32>;
@group(2) @binding(4) var normal_sampler:sampler;
@group(2) @binding(5) var mr_tex:texture_2d<f32>;
@group(2) @binding(6) var mr_sampler:sampler;
@group(2) @binding(7) var ao_tex:texture_2d<f32>;
@group(2) @binding(8) var ao_sampler:sampler;
struct V { @builtin(position) clip:vec4<f32>, @location(0) world:vec3<f32>, @location(1) normal:vec3<f32>, @location(2) uv:vec2<f32>, @location(3) tangent:vec4<f32> };
@vertex fn vs(@location(0) p:vec3<f32>,@location(1) n:vec3<f32>,@location(2) uv:vec2<f32>,@location(3) tangent:vec4<f32>)->V {
 var o:V;let world=draw.world*vec4<f32>(p,1.0);o.clip=scene.view_proj*world;o.world=world.xyz;o.normal=normalize((draw.normal*vec4<f32>(n,0.0)).xyz);
 let t=(draw.world*vec4<f32>(tangent.xyz,0.0)).xyz;o.tangent=vec4<f32>(normalize(t-o.normal*dot(t,o.normal)),tangent.w*mat.flags.y);o.uv=uv;return o;
}
const PI:f32=3.141592653589793;
fn brdf(albedo:vec3<f32>,metal:f32,rough:f32,n:vec3<f32>,l:vec3<f32>,v:vec3<f32>)->vec3<f32>{
 let nl=dot(n,l);let nv=dot(n,v);if nl<=0.0||nv<=0.0{return vec3<f32>(0.0);}let h=normalize(l+v);let nh=clamp(dot(n,h),0.0,1.0);let vh=clamp(dot(v,h),0.0,1.0);let alpha=max(rough*rough,0.001);let a2=alpha*alpha;
 let denominator=nh*nh*(a2-1.0)+1.0;let d=a2/(PI*denominator*denominator);
 // Height-correlated Smith visibility (Heitz 2014; includes 1/(4 nl nv)) - mirrors
 // threed/pbr.rs sample() term-for-term (glTF-Sample-Renderer brdf.glsl, Apache-2.0).
 let ggxv=nl*sqrt(nv*nv*(1.0-a2)+a2);let ggxl=nv*sqrt(nl*nl*(1.0-a2)+a2);let vis=0.5/(ggxv+ggxl);
 let f0=mix(vec3(0.04),albedo,metal);let f=f0+(vec3<f32>(1.0)-f0)*pow(1.0-vh,5.0);
 return (1.0-metal)*(vec3<f32>(1.0)-f)*albedo/PI+f*d*vis;
}
// time_count.z = shadowed light index (<0: none). shadow_texel = (1/w, 1/h, texel_world,
// depth_norm_per_texel); shadow_params = (bias_const, bias_slope, normal_offset, disk_radius_texels),
// all in pinned threed/shadow.rs texel units.
// Rotated-disk PCF: tap positions (D3D standard MSAA sample points) + Gaussian PDF weights +
// per-pixel interleaved-gradient-noise rotation, adapted from Bevy
// (crates/bevy_pbr/src/render/shadow_sampling.wgsl, MIT OR Apache-2.0); mirrors shadow.rs
// visibility_disk so GPU and CPU cannot drift.
const DISK_POS:array<vec2<f32>,8>=array<vec2<f32>,8>(vec2<f32>(0.125,-0.375),vec2<f32>(-0.125,0.375),vec2<f32>(0.625,0.125),vec2<f32>(-0.375,-0.625),vec2<f32>(-0.625,0.625),vec2<f32>(-0.875,-0.125),vec2<f32>(0.375,0.875),vec2<f32>(0.875,-0.875));
const DISK_W:array<f32,8>=array<f32,8>(0.157112,0.157112,0.138651,0.130251,0.114946,0.110269,0.098503,0.093156);
fn ign(x:u32,y:u32)->f32{let v=52.9829189*fract(0.06711056*f32(x)+0.00583715*f32(y));return fract(v);}
fn shadow_visibility(world:vec3<f32>, n:vec3<f32>, nl:f32, frag:vec2<f32>)->f32 {
 let p=world+n*(scene.shadow_texel.z*scene.shadow_params.z);
 let q4=scene.shadow_vp*vec4<f32>(p,1.0);
 let u=(q4.x+1.0)*0.5;let v=(1.0-q4.y)*0.5;var d=q4.z;
 if u<0.0||u>1.0||v<0.0||v>1.0 { return 1.0; }
 let slope=min(sqrt(max(1.0-nl*nl,0.0))/max(nl,1e-3),4.0);
 d=d-(scene.shadow_params.x+scene.shadow_params.y*slope)*scene.shadow_texel.w;
 let angle=ign(u32(frag.x),u32(frag.y))*6.28318530718;let sn=sin(angle);let cs=cos(angle);
 var lit=0.0;var wsum=0.0;
 for(var k=0;k<8;k=k+1){let t=DISK_POS[k];let rt=vec2<f32>(cs*t.x-sn*t.y,sn*t.x+cs*t.y)*scene.shadow_params.w;
  let uv=vec2<f32>(u+rt.x*scene.shadow_texel.x,v+rt.y*scene.shadow_texel.y);
  lit=lit+DISK_W[k]*textureSampleCompareLevel(shadow_tex,shadow_sampler,uv,d);wsum=wsum+DISK_W[k];}
 return lit/wsum;
}
@fragment fn fs(v:V,@builtin(front_facing) front:bool)->@location(0) vec4<f32> {
 // Single-sided visibility supplied via draw cull mode; double-sided frame flips.
 var n=normalize(v.normal);var sign=v.tangent.w;if dot(n,scene.eye.xyz-v.world)<0.0&&mat.flags.x>0.5{n=-n;sign=-sign;}
 let t=normalize(v.tangent.xyz-n*dot(n,v.tangent.xyz));let b=cross(n,t)*sign;
 if mat.flags.z>0.5{var sampled=textureSampleLevel(normal_tex,normal_sampler,v.uv,0.0).xyz*2.0-vec3<f32>(1.0);sampled=vec3<f32>(sampled.xy*mat.factors.w,sampled.z);let mapped=normalize(sampled);n=normalize(t*mapped.x+b*mapped.y+n*mapped.z);}
 let albedo=mat.base.rgb*textureSampleLevel(albedo_tex,albedo_sampler,v.uv,0.0).rgb;
 let mr=textureSampleLevel(mr_tex,mr_sampler,v.uv,0.0);let metal=mat.factors.x*mr.b;let rough=max(0.001,mat.factors.y*mr.g);let ao=1.0+mat.factors.z*(textureSampleLevel(ao_tex,ao_sampler,v.uv,0.0).r-1.0);
 let view=normalize(scene.eye.xyz-v.world);var color=scene.ambient.rgb*albedo*(1.0-metal)*ao/PI;
 for(var i=0u;i<u32(scene.time_count.y);i=i+1u){let light=scene.lights[i];var l=normalize(-light.vector_kind.xyz);var rgb=light.rgb_min.rgb;var vis=1.0;
  if light.vector_kind.w>0.5{let delta=light.vector_kind.xyz-v.world;let distance=length(delta);if distance==0.0{continue;}l=delta/distance;let limited=max(distance,light.rgb_min.w);rgb=rgb/(limited*limited);}
  let nl=max(dot(n,l),0.0);
  if i32(scene.time_count.z)>=0&&i==u32(scene.time_count.z){vis=shadow_visibility(v.world,n,nl,v.clip.xy);} // v.clip in fragment input = framebuffer coord}
  color+=brdf(albedo,metal,rough,n,l,view)*rgb*vis*nl;}
 return vec4<f32>(color,1.0);
}
