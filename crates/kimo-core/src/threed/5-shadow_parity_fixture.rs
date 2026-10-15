//! PL1b parity: lit_raster shadow map vs textured_bvh analytic shadow rays (96x54).
//! Display transforms differ (ACES vs clamped linear), so parity compares the per-pixel
//! RATIO shadowed/unshadowed on each path (ratio factors out the display transform and the
//! material response; both paths share the same BRDF via lighting_stage). The raster path
//! runs the shipping rotated-disk PCF (radius 4, Bevy D3D taps) vs the reference's hard
//! ray - penumbra pixels absorb into the pinned band below. f32 (raster) vs f64 (raycast) interpolation also lands in the band.
use super::*;
use crate::threed::{self,primitives::{generate,Primitive},vertex_stream::Stream,texture_material::Surface,gltf_surface::Asset,lighting_stage::Light,pbr::Material};use crate::camera_projection::Camera;use threed::shadow;

fn cube_at(size:[f64;3],off:[f64;3])->Stream{let g=generate(&Primitive::Cube{size,subdivisions:1}).unwrap();let mut s=Stream{vertices:g.vertices,indices:g.indices};for v in &mut s.vertices{for j in 0..3{v.position[j]+=off[j] as f32;}}s}
fn scene()->(Vec<Stream>,Camera,Vec<Light>){(vec![cube_at([16.,1.,16.],[0.,-0.5,0.]),cube_at([1.2,1.2,1.2],[0.4,0.6,0.8])],Camera{eye:[0.,6.,-6.],pan:0.,tilt:45.,roll:0.,sensor_height:24.,focal_length:35.,near:0.1},vec![Light::Directional{travel_direction:[0.35,-1.,0.3],radiance:[3.;3]}])}
const TRAVEL:[f64;3]=[0.35,-1.,0.3];

fn map_for(streams:&[Stream],res:usize)->shadow::DirectionalShadowMap{let poss:Vec<Vec<[f64;3]>>=streams.iter().map(|s|s.vertices.iter().map(|v|v.position.map(|x|x as f64)).collect()).collect();let refs:Vec<&[[f64;3]]>=poss.iter().map(|v|&v[..]).collect();let aabb=shadow::scene_aabb(&refs).unwrap();let(view,bounds)=shadow::directional_light_space(TRAVEL,aabb).unwrap();let mut map=shadow::DirectionalShadowMap{view,bounds,width:res,height:res,depth:vec![f64::INFINITY;res*res]};for(i,s)in streams.iter().enumerate(){shadow::raster_depth_into(&poss[i],&s.indices,&mut map).unwrap();}map}

fn asset(streams:&[Stream])->Asset{Asset{streams:streams.to_vec(),surfaces:vec![Surface{albedo:[0.7,0.7,0.7],metallic:0.,roughness:0.9,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:false},Surface{albedo:[0.7,0.4,0.3],metallic:0.,roughness:0.9,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:false}],images:vec![],metadata:serde_json::json!({"fixture":"pl1b parity"})}}

#[test]fn raycast_shadow_visibility_unit(){let(streams,_,_)=scene();let a=asset(&streams);let indices:Vec<crate::threed::flat_sah::Index>=a.streams.iter().map(crate::threed::flat_sah::Index::build).collect::<Result<_,_>>().unwrap();
 // point directly under the cube center is occluded; 3 units west is clear
 assert_eq!(crate::threed::textured_bvh::shadow_visibility(&a,&indices,[0.4,0.01,0.8],[0.,1.,0.],TRAVEL).unwrap(),0.);
 assert_eq!(crate::threed::textured_bvh::shadow_visibility(&a,&indices,[-3.,0.01,0.8],[0.,1.,0.],TRAVEL).unwrap(),1.);}

#[test]fn raycast_render_shadowed_darkens_region_and_none_is_identical(){let(streams,camera,lights)=scene();let a=asset(&streams);
 let unshadowed=crate::threed::textured_bvh::render(&a,&camera,96,54,&lights,[0.;3]).unwrap();
 let identical=crate::threed::textured_bvh::render_shadowed(&a,&camera,96,54,&lights,[0.;3],None).unwrap();
 assert_eq!(unshadowed.frame.d,identical.frame.d,"None must be bit-identical");
 let shadowed=crate::threed::textured_bvh::render_shadowed(&a,&camera,96,54,&lights,[0.;3],Some(0)).unwrap();
 assert_eq!(unshadowed.covered,shadowed.covered);
 let darker=(0..96*54).filter(|&px|shadowed.frame.d[px*4]<unshadowed.frame.d[px*4]-1e-6).count();
 // At 96x54/45deg the cube hides most of its own shadow; measured visible region = 19 px.
 assert!(darker>=10,"raycast shadow region missing: {darker}");
 // monotonicity: shadows only ever darken (no light leak)
 let brighter=(0..96*54).filter(|&px|shadowed.frame.d[px*4]>unshadowed.frame.d[px*4]+1e-6).count();
 assert_eq!(brighter,0,"shadow must never brighten");}

#[test]fn raster_vs_raycast_shadow_ratio_parity_96x54(){let(streams,camera,lights)=scene();let a=asset(&streams);let mat:Vec<Material>=a.surfaces.iter().map(|s|Material{albedo:s.albedo,metallic:s.metallic,roughness:s.roughness}).collect();
 let draws:Vec<(&Stream,&Material)>=streams.iter().zip(mat.iter()).map(|(s,m)|(s,m)).collect();
 let r_un=crate::threed::lit_raster::render_multi(&draws,&camera,96,54,&lights).unwrap();
 let map=map_for(&streams,256);
 let r_sh=crate::threed::lit_raster::render_shadowed(&draws,&camera,96,54,&lights,&[Some(shadow::ShadowMapRef::Directional(&map))]).unwrap();
 let b_un=crate::threed::textured_bvh::render(&a,&camera,96,54,&lights,[0.;3]).unwrap();
 let b_sh=crate::threed::textured_bvh::render_shadowed(&a,&camera,96,54,&lights,[0.;3],Some(0)).unwrap();
 let mut n=0;let(mut within,mut maxd)=(0,0f64);
 for px in 0..96*54{let i=px*4;
  let ru=(r_un.frame.d[i] as f64).max(1e-9);let bu=b_un.frame.d[i] as f64;
  if ru<0.05||bu<0.02{continue;} // too dark for a meaningful ratio (ACES floor / coverage edge)
  let r_ratio=r_sh.frame.d[i] as f64/ru;let b_ratio=b_sh.frame.d[i] as f64/bu;
  let d=(r_ratio-b_ratio).abs();maxd=maxd.max(d);n+=1;if d<=0.10{within+=1;}}
 assert!(n>800,"parity sample too small: {n}");
 let frac=within as f64/n as f64;
 println!("PARITY-STATS n={n} within0.10={within} frac={frac:.4} maxdiff={maxd:.4}");
 // Pinned band (measured on this fixture, shipping disk PCF radius 4): frac=0.9880
 // within 0.10, maxdiff=0.772. The disk penumbra is WIDER than a hard ray's edge, so
 // more boundary pixels differ from the reference (each by less: max fell 0.881->0.772
 // vs the old 3x3 grid). Shadow interior and far-lit pixels stay within the band; the
 // disagreement lives entirely on the penumbra, where a fractional-visibility filter and
 // a hard ray legitimately differ. IGN rotation is deterministic per pixel (stable pins).
 assert!(frac>=0.985,"parity band violated: {frac:.3} within 0.10 (max {maxd:.3})");
 assert!(maxd<=0.95,"parity max regressed beyond the penumbra effect: {maxd:.3}");}
