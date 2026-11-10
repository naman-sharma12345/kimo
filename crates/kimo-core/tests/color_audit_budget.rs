use kimo_core::{color::output_audit,effects::Frame,model::Project,guard};
use std::sync::atomic::Ordering::Relaxed;
#[test]fn audit_overlay_preflight_all_frame_copies(){
    let mut p=Project::default();p.color_pipeline.enabled=true;p.color_pipeline.working_gamut=0;
    let f=Frame{w:16,h:16,d:vec![0.25;1024]};let bytes=f.d.len()*4;
    let prior=guard::BUDGET.swap(guard::LIVE.load(Relaxed)+bytes-1,Relaxed);
    let audit=std::panic::catch_unwind(||output_audit::audit(&p,&f));
    guard::BUDGET.store(guard::LIVE.load(Relaxed)+2*bytes-1,Relaxed);
    let overlay=std::panic::catch_unwind(||output_audit::gamut_overlay(&p,&f,1.));
    guard::BUDGET.store(prior,Relaxed);assert!(audit.is_err());assert!(overlay.is_err());
    assert_eq!(output_audit::audit(&p,&f).unwrap()["working_linear"]["visible_pixels"],256);
    assert_eq!(output_audit::gamut_overlay(&p,&f,1.).unwrap().0.d.len(),f.d.len());
}
