use kimo_core::color::managed::{decode,encode,conversion,mv};
#[test]fn seeded_signed_hdr_gamut_transfer_roundtrip(){
    let mut seed=0x4d595df4u64;let mut samples=0;
    for from in 0..8{for to in 0..8{for _ in 0..64{
        let mut rgb=[0.;3];for v in &mut rgb{seed=seed.wrapping_mul(6364136223846793005).wrapping_add(1);*v=((seed>>32) as u32 as f64/u32::MAX as f64)*16.25-0.25;}
        let target=mv(conversion(from,to),rgb);let restored=mv(conversion(to,from),target);
        for j in 0..3{assert!((restored[j]-rgb[j]).abs()<1e-10,"gamut{from}->{to}");}
        for transfer in [0,1,2,3,4,5,8]{for value in target{let back=decode(transfer,encode(transfer,value,203.),203.);assert!((back-value).abs()<1e-6,"transfer{transfer} value{value} back{back}");samples+=1;}}
    }}}
    assert_eq!(samples,86016);
}
#[test]fn pq_hlg_monotone_declared_domain(){
    for transfer in [6,7]{for white in [1.,100.,203.,1000.,10000.]{let mut last=f64::NEG_INFINITY;
        for i in 0..=4096{let code=i as f64/4096.;let linear=decode(transfer,code,white);assert!(linear.is_finite()&&linear>=last,"transfer{transfer} white{white}");last=linear;
            let back=encode(transfer,linear,white);if transfer==6&&i==0{assert_eq!(linear,0.);assert!((back-0.8359375f64.powf(78.84375)).abs()<1e-15);continue}assert!((back-code).abs()<4e-8,"transfer{transfer} white{white} code{code}");
        }
    }}
}
