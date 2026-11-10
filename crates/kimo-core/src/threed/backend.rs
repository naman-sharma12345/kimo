use serde::Serialize;
#[derive(Debug,Serialize)]pub struct Capabilities {pub cpu_reference:bool,pub gpu_feature_compiled:bool,pub gpu_render_implemented:bool,pub fallback_contract:&'static str}
pub fn capabilities()->Capabilities{Capabilities{cpu_reference:true,gpu_feature_compiled:cfg!(feature="gpu3d"),gpu_render_implemented:false,fallback_contract:"Independent CPU raster, not guaranteed llvmpipe. GPU probe scaffold only; render/readback pending."}}
/// Source scaffold exposes async adapter probing without extra executor crate.
/// Caller must supply a bounded executor; no blocking poll/driver claim here.
#[cfg(all(feature="gpu3d",any(target_os="linux",target_os="windows",target_os="macos")))]
pub async fn probe_software(software_only:bool)->Result<String,String>{
 if wgpu::Instance::enabled_backend_features().is_empty(){return Err("no compiled target backend".into());}
 let instance=wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
 let adapter=instance.request_adapter(&wgpu::RequestAdapterOptions{force_fallback_adapter:software_only,compatible_surface:None,..Default::default()}).await.map_err(|e|e.to_string())?;
 Ok(format!("{:?}",adapter.get_info()))
}
/// Readback row-stride plan for a future RGBA32f render attachment.
pub fn readback_layout(w:usize,h:usize)->Result<(usize,usize),String>{if w==0||h==0||w>4096||h>4096{return Err("target1..4096 required".into());}let row=w.checked_mul(16).ok_or("row overflow")?;let padded=row.checked_add(255).ok_or("row overflow")?/256*256;let total=padded.checked_mul(h).ok_or("readback overflow")?;Ok((padded,total))}

#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn readback_layout_rows_pad_to_256(){assert_eq!(readback_layout(1,1).unwrap(),(256,256));
  assert_eq!(readback_layout(16,2).unwrap(),(256,512),"16 px * 16 B = 256 B row, already aligned");
  assert_eq!(readback_layout(17,1).unwrap(),(512,512),"17 px * 16 B = 272 B row pads to 512");
  assert_eq!(readback_layout(4096,4096).unwrap(),(65536,65536*4096),"max target fits without overflow");}
 #[test]fn readback_layout_typed_errors(){assert_eq!(readback_layout(0,1).unwrap_err(),"target1..4096 required");
  assert_eq!(readback_layout(1,4097).unwrap_err(),"target1..4096 required");}
}
