use kimo_core::{color,effects::{Frame,Params},guard};
use std::sync::atomic::Ordering::Relaxed;
#[test]fn cancelled_color_calls_do_not_mutate(){
    type Effect=fn(&Params,&mut Frame,f64)->Result<(),String>;
    let effects:[Effect;8]=[color::linear::apply,color::managed::apply,color::curves::apply,color::adaptation::apply,color::gamut_ray::apply,color::shoulder::apply,color::matching::apply,color::cdl::apply];
    for apply in effects{
        let mut f=Frame{w:2,h:2,d:vec![0.5;16]};let before=f.d.clone();let tok=guard::new_token();tok.store(true,Relaxed);let g=guard::install_token(&tok);
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||apply(&Params::new(),&mut f,0.)));
        drop(g);assert!(result.is_err());assert_eq!(f.d,before);
    }
    let tok=guard::new_token();tok.store(true,Relaxed);let g=guard::install_token(&tok);
    let result=std::panic::catch_unwind(||color::lut::Lut::parse("LUT_3D_SIZE 2\n"));
    drop(g);assert!(result.is_err());
}
