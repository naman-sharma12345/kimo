//! PL2 parity: lit_raster point-cube shadow map vs textured_bvh analytic point shadow rays
//! (96x54). Same ratio methodology as shadow_parity_fixture (the per-pixel shadowed/unshadowed
//! ratio factors out the display transform and the shared BRDF): the raster path runs the
//! shipping rotated-disk PCF (radius 4, Bevy D3D taps) vs the reference's hard ray - penumbra
//! pixels absorb into the pinned band below. Point-specific disagreement lives at cube-face
//! seams, where a disk tap crossing a face edge samples off-map (=lit) while the shadow ray
//! sees continuous geometry, and in the ray-offset-vs-1e-4-epsilon receiver shift (the parity
//! band absorbs it, as in the directional fixture). The near clip stays out of the fixture:
//! the nearest caster is ~1 unit from the light, far above min_distance.
use crate::threed::{self,primitives::{generate,Primitive},vertex_stream::Stream,texture_material::Surface,gltf_surface::Asset,lighting_stage::Light,pbr::Material};use crate::camera_projection::Camera;use threed::shadow;

fn cube_at(size:[f64;3],off:[f64;3])->Stream{let g=generate(&Primitive::Cube{size,subdivisions:1}).unwrap();let mut s=Stream{vertices:g.vertices,indices:g.indices};for v in &mut s.vertices{for j in 0..3{v.position[j]+=off[j] as f32;}}s}
const LIGHT_POS:[f64;3]=[0.4,3.5,0.8];
fn scene()->(Vec<Stream>,Camera,Vec<Light>){(vec![cube_at([16.,1.,16.],[0.,-0.5,0.]),cube_at([1.2,1.2,1.2],[0.4,0.6,0.8])],Camera{eye:[0.,6.,-6.],pan:0.,tilt:45.,roll:0.,sensor_height:24.,focal_length:35.,near:0.1},vec![Light::Point{position:LIGHT_POS,intensity:[40.;3],min_distance:0.05}])}

fn point_map_for(streams:&[Stream],res:usize)->shadow::PointCubeShadowMap{let poss:Vec<Vec<[f64;3]>>=streams.iter().map(|s|s.vertices.iter().map(|v|v.position.map(|x|x as f64)).collect()).collect();let refs:Vec<&[[f64;3]]>=poss.iter().map(|v|&v[..]).collect();let aabb=shadow::scene_aabb(&refs).unwrap();let(views,near,far)=shadow::point_cube_light_space(LIGHT_POS,aabb,0.05).unwrap();let mut map=shadow::PointCubeShadowMap{position:LIGHT_POS,views,near,far,size:res,depth:vec![f64::INFINITY;6*res*res]};for(i,s)in streams.iter().enumerate(){shadow::raster_cube_depth_into(&poss[i],&s.indices,&mut map).unwrap();}map}

fn asset(streams:&[Stream])->Asset{Asset{streams:streams.to_vec(),surfaces:vec![Surface{albedo:[0.7,0.7,0.7],metallic:0.,roughness:0.9,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:false},Surface{albedo:[0.7,0.4,0.3],metallic:0.,roughness:0.9,base:None,metal_rough:None,occlusion:None,normal:None,ao_strength:1.,normal_scale:1.,double_sided:false}],images:vec![],metadata:serde_json::json!({"fixture":"pl2 point parity"})}}

#[test]fn raycast_point_shadow_visibility_unit(){let(streams,_,_)=scene();let a=asset(&streams);let indices:Vec<crate::threed::flat_sah::Index>=a.streams.iter().map(crate::threed::flat_sah::Index::build).collect::<Result<_,_>>().unwrap();
 // ground point directly under the cube center: the ray to the light passes through the cube
 assert_eq!(crate::threed::textured_bvh::point_shadow_visibility(&a,&indices,[0.4,0.01,0.8],[0.,1.,0.],LIGHT_POS).unwrap(),0.);
 // 3 units west: the ray clears the cube on its way up
 assert_eq!(crate::threed::textured_bvh::point_shadow_visibility(&a,&indices,[-3.,0.01,0.8],[0.,1.,0.],LIGHT_POS).unwrap(),1.);
 // receiver coincident with the light: lit by contract (nothing between)
 assert_eq!(crate::threed::textured_bvh::point_shadow_visibility(&a,&indices,LIGHT_POS,[0.,1.,0.],LIGHT_POS).unwrap(),1.);}

#[test]fn raycast_render_shadowed_point_darkens_region_and_none_is_identical(){let(streams,camera,lights)=scene();let a=asset(&streams);
 let unshadowed=crate::threed::textured_bvh::render(&a,&camera,96,54,&lights,[0.;3]).unwrap();
 let identical=crate::threed::textured_bvh::render_shadowed(&a,&camera,96,54,&lights,[0.;3],None).unwrap();
 assert_eq!(unshadowed.frame.d,identical.frame.d,"None must be bit-identical");
 let shadowed=crate::threed::textured_bvh::render_shadowed(&a,&camera,96,54,&lights,[0.;3],Some(0)).unwrap();
 assert_eq!(unshadowed.covered,shadowed.covered);
 let darker=(0..96*54).filter(|&px|shadowed.frame.d[px*4]<unshadowed.frame.d[px*4]-1e-6).count();
 assert!(darker>=10,"raycast point shadow region missing: {darker}");
 let brighter=(0..96*54).filter(|&px|shadowed.frame.d[px*4]>unshadowed.frame.d[px*4]+1e-6).count();
 assert_eq!(brighter,0,"shadow must never brighten");}

#[test]fn raster_vs_raycast_point_shadow_ratio_parity_96x54(){let(streams,camera,lights)=scene();let a=asset(&streams);let mat:Vec<Material>=a.surfaces.iter().map(|s|Material{albedo:s.albedo,metallic:s.metallic,roughness:s.roughness}).collect();
 let draws:Vec<(&Stream,&Material)>=streams.iter().zip(mat.iter()).map(|(s,m)|(s,m)).collect();
 let r_un=crate::threed::lit_raster::render_multi(&draws,&camera,96,54,&lights).unwrap();
 let map=point_map_for(&streams,256);
 let r_sh=crate::threed::lit_raster::render_shadowed(&draws,&camera,96,54,&lights,&[Some(shadow::ShadowMapRef::PointCube(&map))]).unwrap();
 let b_un=crate::threed::textured_bvh::render(&a,&camera,96,54,&lights,[0.;3]).unwrap();
 let b_sh=crate::threed::textured_bvh::render_shadowed(&a,&camera,96,54,&lights,[0.;3],Some(0)).unwrap();
 let mut n=0;let(mut within,mut maxd)=(0,0f64);
 for px in 0..96*54{let i=px*4;
  let ru=(r_un.frame.d[i] as f64).max(1e-9);let bu=b_un.frame.d[i] as f64;
  if ru<0.05||bu<0.02{continue;} // too dark for a meaningful ratio (ACES floor / coverage edge)
  let r_ratio=r_sh.frame.d[i] as f64/ru;let b_ratio=b_sh.frame.d[i] as f64/bu;
  let d=(r_ratio-b_ratio).abs();maxd=maxd.max(d);n+=1;if d<=0.10{within+=1;}else if std::env::var("PARITY_DUMP").is_ok(){let kind=if b_ratio>0.5{"ref-lit/raster-darker"}else{"ref-shadowed/raster-lighter"};println!("DISAGREE px={},{} {kind} r_ratio={r_ratio:.3} b_ratio={b_ratio:.3}",px%96,px/96);}}
 assert!(n>800,"parity sample too small: {n}");
 let frac=within as f64/n as f64;
 println!("POINT-PARITY-STATS n={n} within0.10={within} frac={frac:.4} maxdiff={maxd:.4}");
 // Pinned band (measured on this fixture, shipping disk PCF radius 4 + ray offset +
 // gradient-scaled tap bias): frac=0.9919 within 0.10, maxdiff=0.9270. Remaining
 // disagreement lives on the penumbra ring (a fractional-visibility disk vs a hard ray
 // legitimately differs) and single-tap face-seam taps that sample off-map (=lit) where
 // the ray sees continuous geometry; umbra interior and far-lit ground stay in band.
 assert!(frac>=0.99,"parity band violated: {frac:.3} within 0.10 (max {maxd:.3})");
 assert!(maxd<=0.95,"parity max regressed beyond the penumbra effect: {maxd:.3}");}
