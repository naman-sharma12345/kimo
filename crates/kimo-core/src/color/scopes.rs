//! Quantitative float scopes. No clipping before range accounting.
use crate::effects::Frame;
use serde_json::{json,Value};
pub fn data(f:&Frame,bins:usize,min:f64,max:f64)->Result<Value,String>{
    data_domain(f,bins,min,max,[0.2126,0.7152,0.0722],1.,true)
}
pub fn data_domain(f:&Frame,bins:usize,min:f64,max:f64,weights:[f64;3],scale:f64,vector709:bool)->Result<Value,String>{
    if !scale.is_finite()||scale<=0.||weights.iter().any(|v|!v.is_finite())||f.w.checked_mul(f.h).and_then(|v|v.checked_mul(4))!=Some(f.d.len()){return Err("invalid scope domain/frame".into())}
    if !(16..=4096).contains(&bins)||!min.is_finite()||!max.is_finite()||!(max-min).is_finite()||max-min<1e-9{return Err("bins 16..4096 and finite increasing range required".into())}
    crate::guard::reserve((4+f.w.min(256))*bins*8+128*128*8);let mut hist=vec![vec![0u64;bins];4];let mut under=[0u64;4];let mut over=[0u64;4];let mut nonfinite=0u64;let mut invalid_alpha=0u64;let mut overflow=0u64;let mut sum=[0.;4];let mut lo=[f64::INFINITY;4];let mut hi=[f64::NEG_INFINITY;4];let mut count=0u64;
    let columns=f.w.min(256);let mut wave=vec![vec![0u64;bins];columns];let mut vector=vec![vec![0u64;128];128];
    for (i,px) in f.d.chunks_exact(4).enumerate(){if i%65536==0{crate::guard::check()}if px.iter().any(|v|!v.is_finite()){nonfinite+=1;continue}if !(0. ..=1.).contains(&px[3]){invalid_alpha+=1;continue}if px[3]==0.{continue}
        let c=[px[0] as f64*scale,px[1] as f64*scale,px[2] as f64*scale,(weights[0]*px[0] as f64+weights[1]*px[1] as f64+weights[2]*px[2] as f64)*scale];if c.iter().any(|v|!v.is_finite())||(0..4).any(|j|!(sum[j]+c[j]).is_finite()){overflow+=1;continue}count+=1;
        for j in 0..4{sum[j]+=c[j];lo[j]=lo[j].min(c[j]);hi[j]=hi[j].max(c[j]);if c[j]<min{under[j]+=1}else if c[j]>max{over[j]+=1}else{let b=(((c[j]-min)/(max-min)*(bins-1) as f64) as usize).min(bins-1);hist[j][b]+=1;if j==3&&columns>0{let x=(i%f.w)*columns/f.w;wave[x][b]+=1}}}
        // BT.709 CbCr, only applicable to display-referred BT.709 input.
        if vector709{let cb=(c[2]-c[3])/1.8556;let cr=(c[0]-c[3])/1.5748;
        let x=((cb+0.5)*127.).clamp(0.,127.) as usize;let y=((cr+0.5)*127.).clamp(0.,127.) as usize;vector[y][x]+=1;}
    }
    let valid=count>0;Ok(json!({"bins":bins,"range":[min,max],"channel_order":["r","g","b","luminance_or_luma"],"pixels":count,"nonfinite_pixels":nonfinite,"invalid_alpha_pixels":invalid_alpha,"scaled_overflow_pixels":overflow,"min":if valid{Some(lo)}else{None},"max":if valid{Some(hi)}else{None},"mean":if valid{Some(sum.map(|v|v/count as f64))}else{None},"under_range":under,"over_range":over,"histogram":hist,"waveform_columns":columns,"waveform":wave,"vectorscope_709":if vector709{Some(vector)}else{None},"vector_note":"BT709 CbCr only supplied for encoded BT709 display mode; otherwise null. Excursions clamp to grid."}))
}
