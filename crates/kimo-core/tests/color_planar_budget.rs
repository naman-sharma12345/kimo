use kimo_core::{color::interchange,effects::Frame,guard};
use std::sync::atomic::Ordering::Relaxed;
#[test]fn planar_pack_unpack_preflight_destination(){
    let f=Frame{w:16,h:16,d:vec![0.25;1024]};let bytes=interchange::pack(&f).unwrap();
    let prior=guard::BUDGET.swap(guard::LIVE.load(Relaxed)+bytes.len()-1,Relaxed);
    let pack=std::panic::catch_unwind(||interchange::pack(&f));
    let unpack=std::panic::catch_unwind(||interchange::unpack(&bytes,16,16));
    guard::BUDGET.store(prior,Relaxed);assert!(pack.is_err());assert!(unpack.is_err());
    assert_eq!(interchange::unpack(&bytes,16,16).unwrap(),f.d);
}
