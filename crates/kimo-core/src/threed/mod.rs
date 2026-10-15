//! Opt-in MCP-first 3D foundation. Original core math. No legacy render changes.
pub mod mesh;
pub mod obj_export;
pub mod gltf_export;
pub mod spatial;
pub mod flat_sah;
pub mod sdf_graph;
pub mod contour_wrapped;
#[cfg(feature="surface-contour")]
pub mod octree;
pub mod node_ingest;
pub mod scene;
pub mod animate;
pub mod offscreen_wgpu;
pub mod texture_material;
pub mod mikk_adapter;
pub mod gltf_surface;
pub mod textured_bvh;
pub mod textured_fixture;
#[cfg(all(feature="mesh-import",feature="material-textures",feature="flat-sah"))]
pub mod pbr_offscreen;
pub mod sdf;
pub mod pbr;
pub mod ibl;
pub mod cpu;
pub mod backend;
pub mod import;

pub mod primitives;
pub mod perspective;
pub mod vertex_stream;
pub mod ingest;
pub mod lighting_stage; pub mod shadow;
pub mod lit_raster;
pub mod boolean;
pub(crate) mod topo;
pub mod subdivide;
