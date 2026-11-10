use kimo_core::{color::ocio,effects::Frame,model::Project,guard};
use std::sync::atomic::Ordering::Relaxed;
#[test]fn persistent_processor_preflights_each_input_buffer(){
    if std::env::var("KIMO_TEST_OCIO").is_err(){return}
    let mut p=Project::default();p.color_pipeline.enabled=true;
    let mut processor=ocio::Processor::new(&p,16,16,"sdr").unwrap();
    let f=Frame{w:16,h:16,d:vec![0.18;1024]};
    let prior=guard::BUDGET.swap(guard::LIVE.load(Relaxed)+f.d.len()*4-1,Relaxed);
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||processor.apply(&f)));
    guard::BUDGET.store(prior,Relaxed);assert!(result.is_err());
    assert!(processor.apply(&f).unwrap().d.iter().all(|v|v.is_finite()));
}
