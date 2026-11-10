//! Optional native OCIO 2 CPU configuration API. Self-contained configs only.
//! No changes to global OCIO state, project defaults, renderer, or output views.
use crate::effects::Frame;
use std::{ffi::{CStr, CString, c_char, c_void}, ptr::NonNull, path::Path, io::Read};
const MAX_CONFIG: usize = 4 * 1024 * 1024;
unsafe extern "C" {
    fn kimo_ocio_version() -> *const c_char;
    fn kimo_ocio_load(text:*const c_char, err:*mut c_char, cap:usize)->*mut c_void;
    fn kimo_ocio_config_free(p:*mut c_void);
    fn kimo_ocio_count(p:*const c_void, catalog:i32)->usize;
    fn kimo_ocio_name(p:*const c_void,catalog:i32,index:usize)->*const c_char;
    fn kimo_ocio_processor(p:*const c_void,src:*const c_char,dst:*const c_char,err:*mut c_char,cap:usize)->*mut c_void;
    fn kimo_ocio_display_processor(p:*const c_void,src:*const c_char,display:*const c_char,view:*const c_char,err:*mut c_char,cap:usize)->*mut c_void;
    fn kimo_ocio_cdl(slope:*const f64,offset:*const f64,power:*const f64,sat:f64,err:*mut c_char,cap:usize)->*mut c_void;
    fn kimo_ocio_processor_free(p:*mut c_void);
    fn kimo_ocio_noop(p:*const c_void)->i32;
    fn kimo_ocio_apply(p:*const c_void,rgba:*mut f32,pixels:usize,err:*mut c_char,cap:usize)->i32;
}
fn name(v:&str)->Result<CString,String>{if v.len()>4096 || v.is_empty(){return Err("invalid OCIO name".into())}CString::new(v).map_err(|_|"NUL in OCIO name".into())}
fn diagnostic(v:&[c_char])->String { unsafe {CStr::from_ptr(v.as_ptr()).to_string_lossy().into_owned()} }
fn string(p:*const c_char)->String { unsafe {CStr::from_ptr(p).to_string_lossy().into_owned()} }
pub fn version()->String { string(unsafe{kimo_ocio_version()}) }
#[derive(Debug,PartialEq,Eq)]
pub struct View { pub display:String, pub view:String }
pub struct Config { handle:NonNull<c_void> }
impl Drop for Config { fn drop(&mut self){unsafe{kimo_ocio_config_free(self.handle.as_ptr())}} }
impl Config {
    pub fn from_text(text:&str)->Result<Self,String>{
        if text.len()>MAX_CONFIG || text.is_empty(){return Err("OCIO config size must be 1..4MiB".into())}
        let text=CString::new(text).map_err(|_|"NUL in OCIO config")?;
        let mut err=[0;4096];
        let handle=NonNull::new(unsafe{kimo_ocio_load(text.as_ptr(),err.as_mut_ptr(),err.len())}).ok_or_else(||diagnostic(&err))?;
        Ok(Self{handle})
    }
    pub fn from_file(path:impl AsRef<Path>)->Result<Self,String>{
        if !std::fs::metadata(path.as_ref()).map_err(|e|e.to_string())?.is_file(){return Err("OCIO config must be a regular file".into())}
        let file=std::fs::File::open(path).map_err(|e|e.to_string())?;
        if !file.metadata().map_err(|e|e.to_string())?.is_file(){return Err("OCIO config must be a regular file".into())}
        let mut bytes=Vec::new(); file.take((MAX_CONFIG+1) as u64).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
        if bytes.len()>MAX_CONFIG{return Err("OCIO config exceeds 4MiB".into())}
        Self::from_text(std::str::from_utf8(&bytes).map_err(|_|"OCIO config must be UTF8")?)
    }
    pub fn colorspaces(&self)->Vec<String>{
        (0..unsafe{kimo_ocio_count(self.handle.as_ptr(),0)}).map(|i|string(unsafe{kimo_ocio_name(self.handle.as_ptr(),0,i)})).collect()
    }
    pub fn views(&self)->Vec<View>{
        (0..unsafe{kimo_ocio_count(self.handle.as_ptr(),1)}).map(|i|View{
            display:string(unsafe{kimo_ocio_name(self.handle.as_ptr(),1,i)}),
            view:string(unsafe{kimo_ocio_name(self.handle.as_ptr(),2,i)})}).collect()
    }
    /// Forward display-view pipeline, including configured looks. Not a colorspace sandwich.
    pub fn display_processor(&self,src:&str,display:&str,view:&str)->Result<Processor,String>{
        let (src,display,view)=(name(src)?,name(display)?,name(view)?);let mut err=[0;4096];
        let handle=NonNull::new(unsafe{kimo_ocio_display_processor(self.handle.as_ptr(),src.as_ptr(),display.as_ptr(),view.as_ptr(),err.as_mut_ptr(),err.len())}).ok_or_else(||diagnostic(&err))?;
        Ok(Processor{handle})
    }
    /// Processor owns OCIO's CPU handle and may outlive this Config.
    pub fn processor(&self,src:&str,dst:&str)->Result<Processor,String>{
        let (src,dst)=(name(src)?,name(dst)?);let mut err=[0;4096];
        let handle=NonNull::new(unsafe{kimo_ocio_processor(self.handle.as_ptr(),src.as_ptr(),dst.as_ptr(),err.as_mut_ptr(),err.len())}).ok_or_else(||diagnostic(&err))?;
        Ok(Processor{handle})
    }
}
pub struct Processor { handle:NonNull<c_void> }
impl Drop for Processor {fn drop(&mut self){unsafe{kimo_ocio_processor_free(self.handle.as_ptr())}}}
impl Processor {
    pub fn cdl(c:&super::cdl_xml::Correction)->Result<Self,String>{c.validate()?;let mut err=[0;4096];let handle=NonNull::new(unsafe{kimo_ocio_cdl(c.slope.as_ptr(),c.offset.as_ptr(),c.power.as_ptr(),c.saturation,err.as_mut_ptr(),err.len())}).ok_or_else(||diagnostic(&err))?;Ok(Self{handle})}
    pub fn is_noop(&self)->bool{unsafe{kimo_ocio_noop(self.handle.as_ptr())!=0}}
    /// Transactional: no partial frame writes on transform failure or nonfinite output.
    pub fn apply(&self,f:&mut Frame)->Result<(),String>{
        super::frame::validate(f)?;
        if (f.w as u128)*(f.h as u128)>crate::guard::limits().max_pixels as u128{return Err("OCIO frame exceeds pixel limit".into())}
        if self.is_noop(){return Ok(())}
        crate::guard::reserve(f.d.len().checked_mul(4).ok_or("OCIO frame allocation overflow")?);
        let mut temp=Frame{w:f.w,h:f.h,d:f.d.clone()};
        // CPU processors are immutable here; no dynamic-property edits. Disjoint frame ranges.
        let ptr=crate::par::SyncPtr(temp.d.as_mut_ptr());
        let handle=crate::par::SyncPtr(self.handle.as_ptr());
        let failure=std::sync::Mutex::new(None::<String>);
        crate::par::ranges(temp.w*temp.h,65536,|a,b|{let ptr=ptr;let handle=handle;
            for i in (a..b).step_by(65536){crate::guard::check();let mut err=[0;4096];
                if unsafe{kimo_ocio_apply(handle.0,ptr.0.add(i*4),(b-i).min(65536),err.as_mut_ptr(),err.len())}==0{*failure.lock().unwrap()=Some(diagnostic(&err));break;}
            }
        });
        if let Some(e)=failure.into_inner().map_err(|_|"OCIO worker error")?{return Err(e)}
        super::frame::validate(&temp)?;f.d=temp.d;Ok(())
    }
}
