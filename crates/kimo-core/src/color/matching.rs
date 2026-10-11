//! Lab Gaussian optimal-transport grade matching. Display-referred sRGB/D65 only.
//! Symmetric covariance transfer, regularized to handle nearly-flat shots.
use super::managed::{mv,Mat};
use crate::effects::{p,Frame,Params};
#[derive(Clone,Debug)]pub struct Stats{pub mean:[f64;3],pub cov:Mat,pub count:usize}
fn mm(a:Mat,b:Mat)->Mat{std::array::from_fn(|i|std::array::from_fn(|j|(0..3).map(|k|a[i][k]*b[k][j]).sum()))}
const XYZ:Mat=[[0.4123907992659595,0.357584339383878,0.1804807884018343],[0.2126390058715104,0.715168678767756,0.0721923153607337],[0.0193308187155918,0.119194779794626,0.950532152249661]];
const RGB:Mat=[[3.2409699419045226,-1.537383177570094,-0.4986107602930034],[-0.9692436362808796,1.8759675015077202,0.0415550574071756],[0.0556300796969937,-0.2039769588889765,1.0569715142428786]];
const WHITE:[f64;3]=[0.9504559270516716,1.,1.0890577507598784];
pub fn to_lab(c:[f64;3])->[f64;3]{let xyz=mv(XYZ,c.map(|v|super::managed::decode(1,v,100.)));let f=|x:f64|if x>216./24389.{x.cbrt()}else{(24389./27.*x+16.)/116.};let z=std::array::from_fn::<_,3,_>(|i|f(xyz[i]/WHITE[i]));[(116.*z[1]-16.)/100.,500.*(z[0]-z[1])/128.,200.*(z[1]-z[2])/128.]}
pub fn from_lab(c:[f64;3])->[f64;3]{let fy=(c[0]*100.+16.)/116.;let z=[fy+c[1]*128./500.,fy,fy-c[2]*128./200.];let fi=|v:f64|if v>6./29.{v.powi(3)}else{(116.*v-16.)/(24389./27.)};mv(RGB,std::array::from_fn(|i|fi(z[i])*WHITE[i])).map(|v|super::managed::encode(1,v,100.))}
pub fn statistics(f:&Frame,region:Option<[f64;4]>)->Result<Stats,String>{
    if f.w==0||f.h==0||f.w.checked_mul(f.h).and_then(|n|n.checked_mul(4))!=Some(f.d.len()){return Err("invalid Lab sample frame shape".into())}let r=region.unwrap_or([0.,0.,1.,1.]);if r.iter().any(|x|!x.is_finite())||r[0]<0.||r[1]<0.||r[2]<=0.||r[3]<=0.||r[0]+r[2]>1.000001||r[1]+r[3]>1.000001{return Err("region must be positive and within 0..1".into())}
    let (x0,y0,x1,y1)=((r[0]*f.w as f64) as usize,(r[1]*f.h as f64) as usize,((r[0]+r[2])*f.w as f64).ceil() as usize,((r[1]+r[3])*f.h as f64).ceil() as usize);
    let mut out=Stats{mean:[0.;3],cov:[[0.;3];3],count:0};let step=((x1-x0)*(y1-y0)/200_000).max(1);
    for y in y0..y1.min(f.h){crate::guard::check();for x in (x0..x1.min(f.w)).step_by(step){let i=(y*f.w+x)*4;let px=&f.d[i..i+4];if !(0.99..=1.).contains(&px[3])||px[..3].iter().any(|v|!v.is_finite()){continue}
        let c=to_lab([px[0] as f64,px[1] as f64,px[2] as f64]);out.count+=1;let d=std::array::from_fn::<_,3,_>(|j|c[j]-out.mean[j]);for j in 0..3{out.mean[j]+=d[j]/out.count as f64}let d2=std::array::from_fn::<_,3,_>(|j|c[j]-out.mean[j]);for i in 0..3{for j in 0..3{out.cov[i][j]+=d[i]*d2[j]}}
    }}
    if out.count<4{return Err("need at least 4 opaque sample pixels".into())}for row in &mut out.cov{for v in row{*v/=out.count as f64}}Ok(out)
}
fn spd_power(mut a:Mat,power:f64)->Mat{
    let mut v=[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]];
    for _ in 0..32{let(mut pi,mut qi)=(0,1);for (p,q) in [(0,1),(0,2),(1,2)]{if a[p][q].abs()>a[pi][qi].abs(){pi=p;qi=q}}let(p,q)=(pi,qi);if a[p][q].abs()<1e-15{break}
        let theta=0.5*(2.*a[p][q]).atan2(a[q][q]-a[p][p]);let(c,s)=(theta.cos(),theta.sin());
        let mut j=[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]];j[p][p]=c;j[q][q]=c;j[p][q]=s;j[q][p]=-s;
        let jt=std::array::from_fn(|i|std::array::from_fn(|k|j[k][i]));a=mm(jt,mm(a,j));v=mm(v,j);
    }
    std::array::from_fn(|i|std::array::from_fn(|j|(0..3).map(|k|v[i][k]*a[k][k].max(1e-12).powf(power)*v[j][k]).sum()))
}
pub fn transport(src:&Stats,dst:&Stats,regularization:f64)->Result<Mat,String>{
    if [src,dst].iter().any(|s|s.count<4||s.mean.iter().chain(s.cov.iter().flatten()).any(|v|!v.is_finite())||(0..3).any(|i|s.cov[i][i]<0.||(0..3).any(|j|(s.cov[i][j]-s.cov[j][i]).abs()>1e-9))){return Err("Lab transport needs finite symmetric covariance and4samples".into())}
    for s in [src,dst]{let c=s.cov;let scale=c.iter().flatten().fold(0f64,|m,v|m.max(v.abs())).max(1e-12);let a:Mat=c.map(|r|r.map(|v|v/scale));let det=a[0][0]*(a[1][1]*a[2][2]-a[1][2]*a[2][1])-a[0][1]*(a[1][0]*a[2][2]-a[1][2]*a[2][0])+a[0][2]*(a[1][0]*a[2][1]-a[1][1]*a[2][0]);if [(0,1),(0,2),(1,2)].iter().any(|&(i,j)|a[i][i]*a[j][j]-a[i][j]*a[j][i]< -1e-10)||det< -1e-10{return Err("Lab transport covariance must be positive semidefinite".into())}}
    if !regularization.is_finite()||!(1e-8..=0.1).contains(&regularization){return Err("regularization outside 1e-8..0.1".into())}
    let reg=|a:Mat|std::array::from_fn(|i|std::array::from_fn(|j|a[i][j]+if i==j{regularization}else{0.}));let cs=reg(src.cov);let ct=reg(dst.cov);let root=spd_power(cs,0.5);let ir=spd_power(cs,-0.5);
    let a=mm(ir,mm(spd_power(mm(root,mm(ct,root)),0.5),ir));if a.iter().flatten().any(|v|!v.is_finite()||v.abs()>100.){return Err("unstable match; use a richer sample or stronger regularization".into())}Ok(a)
}
pub fn apply(pr:&Params,f:&mut Frame,t:f64)->Result<(),String>{super::frame::validate(f)?;
    if f.w==0||f.h==0||f.w.checked_mul(f.h).and_then(|n|n.checked_mul(4))!=Some(f.d.len())||f.d.iter().any(|v|!v.is_finite()){return Err("invalid Lab effect frame".into())}let g=|k:&str,d:f64|p(pr,k,d,t) as f64;let a=std::array::from_fn(|i|std::array::from_fn(|j|g(&format!("a{i}{j}"),if i==j{1.}else{0.})));
    let sm:[f64;3]=std::array::from_fn(|j|g(&format!("s{j}"),0.));let tm:[f64;3]=std::array::from_fn(|j|g(&format!("t{j}"),0.));let strength=g("strength",1.);let compress=g("gamut_compress",1.);if !strength.is_finite()||!compress.is_finite(){return Err("nonfinite match strength/gamut_compress".into())}let strength=strength.clamp(0.,1.);let compress=compress>0.5;
    if a.iter().flatten().chain(sm.iter()).chain(tm.iter()).any(|v|!v.is_finite()||v.abs()>100.){return Err("invalid match transform".into())}
    for(i,px) in f.d.chunks_exact_mut(4).enumerate(){if i%65536==0{crate::guard::check()}let c=[px[0] as f64,px[1] as f64,px[2] as f64];let lab=to_lab(c);let delta=std::array::from_fn(|j|lab[j]-sm[j]);let mut out=mv(a,delta);for j in 0..3{out[j]=lab[j]+(out[j]+tm[j]-lab[j])*strength}let mut rgb=from_lab(out);
        if compress {let lin=rgb.map(|v|super::managed::decode(1,v,100.));rgb=super::managed::gamut_compress(lin).map(|v|super::managed::encode(1,v,100.))}
        for j in 0..3{if !rgb[j].is_finite()||rgb[j].abs()>f32::MAX as f64{return Err("match overflow".into())}px[j]=rgb[j] as f32}
    }Ok(())
}
pub fn catalog()->Vec<super::Entry>{vec![("lab_match","Lab mean/full-covariance optimal transport, regularized. sRGB/D65 only. Normally set by match_grade. Not semantic/subject-aware and not camera color calibration.",&[("strength",1.,"0..1"),("gamut_compress",1.,"bound output by linear chroma compression")])]}
