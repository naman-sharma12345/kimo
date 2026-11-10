//! Scalar separable box cascade for color halos/mattes. Correct clamp borders.
//! O(pixels), no per-pixel allocations, bounded radius, alpha untouched.
use crate::effects::Frame;
fn axis(src:&[f32],dst:&mut[f32],w:usize,h:usize,r:usize,horizontal:bool){
    let(len,lines)=if horizontal{(w,h)}else{(h,w)};if len==0{return}let index=|line:usize,i:usize|if horizontal{(line*w+i)*4}else{(i*w+line)*4};let win=(2*r+1) as f64;
    for line in 0..lines{let mut sum=[0f64;3];for j in 0..3{sum[j]=src[index(line,0)+j] as f64*(r+1) as f64;for k in 1..=r{sum[j]+=src[index(line,k.min(len-1))+j] as f64}}
        for i in 0..len{let o=index(line,i);for j in 0..3{dst[o+j]=(sum[j]/win) as f32}dst[o+3]=src[o+3];let add=index(line,(i+r+1).min(len-1));let sub=index(line,i.saturating_sub(r));for j in 0..3{sum[j]+=src[add+j] as f64-src[sub+j] as f64}}
    }
}
pub fn blur(f:&mut Frame,radius:usize){if f.w==0||f.h==0||radius==0{return}let r=(radius/2).max(1).min(512);let mut tmp=vec![0.;f.d.len()];for _ in 0..3{axis(&f.d,&mut tmp,f.w,f.h,r,true);axis(&tmp,&mut f.d,f.w,f.h,r,false)}}
