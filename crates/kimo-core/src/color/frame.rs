//! Shared direct-API frame preflight, signed/HDR RGB allowed.
use crate::effects::Frame;
pub fn validate(f:&Frame)->Result<(),String>{
    crate::guard::check();
    if f.w.checked_mul(f.h).and_then(|n|n.checked_mul(4))!=Some(f.d.len()){return Err("color effect requires valid RGBA frame shape".into())}
    for(i,p)in f.d.chunks_exact(4).enumerate(){
        if i%65536==0{crate::guard::check()}
        if p.iter().any(|v|!v.is_finite())||!(0. ..=1.).contains(&p[3]){return Err("color effect requires finite RGB and alpha0..1".into())}
    }
    Ok(())
}
