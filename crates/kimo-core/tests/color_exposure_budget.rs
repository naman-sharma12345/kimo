use kimo_core::{color::exposure,effects::Frame,guard};
use std::sync::atomic::Ordering::Relaxed;
#[test]fn exposure_preflights_full_quantile_capacity(){
    let f=Frame{w:16,h:16,d:vec![1.;1024]};
    let prior=guard::BUDGET.swap(guard::LIVE.load(Relaxed)+256*8-1,Relaxed);
    let analysis=std::panic::catch_unwind(||exposure::analyze(&f,0,100.,0.18,1000.));
    let comparison=std::panic::catch_unwind(||exposure::compare(&f,&f,0,100.,0.5));
    guard::BUDGET.store(prior,Relaxed);
    assert!(analysis.is_err());assert!(comparison.is_err());
    assert_eq!(exposure::analyze(&f,0,100.,0.18,1000.).unwrap()["visible_pixels"],256);
    assert!(exposure::compare(&f,&f,0,100.,0.5).unwrap()["suggested_exposure_stops"].as_f64().unwrap().abs()<1e-12);
}
