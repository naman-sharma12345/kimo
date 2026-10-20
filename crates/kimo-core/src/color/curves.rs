//! Free-position shape-preserving cubic curves. Endpoints extrapolate linearly. Periodic Fritsch-Carlson hue curves.
use crate::effects::{p,Frame,Params};
#[derive(Clone,Debug)]pub struct Curve {x:Vec<f32>,y:Vec<f32>,m:Vec<f32>,ident:bool,flat:bool}
/// Fritsch-Carlson tangents; periodic wraps the point after the last to first+360.
fn fc(x:&[f64],y:&[f64],periodic:bool)->Vec<f64>{
    let n=x.len();let ni=if periodic{n}else{n-1};
    let d:Vec<f64>=(0..ni).map(|k|if k+1<n{(y[k+1]-y[k])/(x[k+1]-x[k])}else{(y[0]-y[k])/(x[0]+360.-x[k])}).collect();
    let mut m:Vec<f64>=(0..n).map(|k|if !periodic&&k==0{d[0]}else if !periodic&&k==n-1{d[n-2]}else{let(a,b)=(if k==0{d[ni-1]}else{d[k-1]},d[k]);if a*b<=0.{0.}else{(a+b)/2.}}).collect();
    for k in 0..ni{let k1=(k+1)%n;if d[k]==0.{m[k]=0.;m[k1]=0.}else{let a=m[k]/d[k];let b=m[k1]/d[k];let s=a*a+b*b;if s>9.{let tau=3./s.sqrt();m[k]=tau*a*d[k];m[k1]=tau*b*d[k]}}}
    m
}
fn check(points:&[[f32;2]])->Result<(Vec<f32>,Vec<f32>,Vec<f32>),String>{
    if !(2..=32).contains(&points.len()){return Err("curve needs 2..32 points".into())}
    if points.iter().flatten().any(|v|!v.is_finite()||v.abs()>16.){return Err("curve coordinates must be finite and within +/-16".into())}
    let x:Vec<_>=points.iter().map(|v|v[0]).collect();let y:Vec<_>=points.iter().map(|v|v[1]).collect();
    let h:Vec<_>=x.windows(2).map(|v|v[1]-v[0]).collect();if h.iter().any(|v|*v<1e-5){return Err("curve x must increase by at least 0.00001".into())}Ok((x,y,h))
}
impl Curve {
    pub fn new(points:&[[f32;2]])->Result<Self,String>{
        let(x,y,h)=check(points)?;let ident=points.iter().all(|v|v[1]==v[0]);
        let d:Vec<_>=y.windows(2).zip(&h).map(|(v,h)|(v[1]-v[0])/h).collect();let n=x.len();let mut m=vec![0.;n];
        if n==2 {m[0]=d[0];m[1]=d[0]}else{
            let endpoint=|h0:f32,h1:f32,d0:f32,d1:f32|{let a=((2.*h0+h1)*d0-h0*d1)/(h0+h1);if a*d0<=0.{0.}else if d0*d1<0.&&a.abs()>3.*d0.abs(){3.*d0}else{a}};
            m[0]=endpoint(h[0],h[1],d[0],d[1]);m[n-1]=endpoint(h[n-2],h[n-3],d[n-2],d[n-3]);
            for i in 1..n-1{if d[i-1]*d[i]>0.{let w1=2.*h[i]+h[i-1];let w2=h[i]+2.*h[i-1];m[i]=(w1+w2)/(w1/d[i-1]+w2/d[i])}}
        }Ok(Self{x,y,m,ident,flat:false})
    }
    /// Fritsch-Carlson multiplier curve with flat ends (lum-vs-sat); identity = all y == 1.
    pub fn new_fc(points:&[[f32;2]])->Result<Self,String>{
        let(x,y,_)=check(points)?;let ident=y.iter().all(|v|*v==1.);
        let m=fc(&x.iter().map(|v|*v as f64).collect::<Vec<_>>(),&y.iter().map(|v|*v as f64).collect::<Vec<_>>(),false).iter().map(|v|*v as f32).collect();
        Ok(Self{x,y,m,ident,flat:true})
    }
    pub fn at(&self,v:f32)->f32{
        if !v.is_finite(){return f32::NAN}let n=self.x.len();
        let v=if self.flat{if self.ident{return 1.}v.clamp(self.x[0],self.x[n-1])}else{if self.ident{return v}v};
        if v<=self.x[0]{return self.y[0]+(v-self.x[0])*self.m[0]}if v>=self.x[n-1]{return self.y[n-1]+(v-self.x[n-1])*self.m[n-1]}
        let i=self.x.partition_point(|x|*x<=v)-1;let h=self.x[i+1]-self.x[i];let t=(v-self.x[i])/h;let(t2,t3)=(t*t,t*t*t);
        (2.*t3-3.*t2+1.)*self.y[i]+(t3-2.*t2+t)*h*self.m[i]+(-2.*t3+3.*t2)*self.y[i+1]+(t3-t2)*h*self.m[i+1]
    }
    pub fn is_ident(&self)->bool{self.ident}
}
/// Periodic Fritsch-Carlson curve over hue degrees [0,360).
#[derive(Clone,Debug)]pub struct HueCurve {x:Vec<f64>,y:Vec<f64>,m:Vec<f64>,ident:bool,neutral:f64}
impl HueCurve {
    pub fn new(points:&[[f32;2]],neutral:f64)->Result<Self,String>{
        if !(2..=32).contains(&points.len()){return Err("hue curve needs 2..32 points".into())}
        if points.iter().any(|v|!v[0].is_finite()||!v[1].is_finite()||!(0. ..360.).contains(&v[0])){return Err("hue curve x must be in [0,360) and y finite".into())}
        let x:Vec<f64>=points.iter().map(|v|v[0] as f64).collect();let y:Vec<f64>=points.iter().map(|v|v[1] as f64).collect();
        if x.windows(2).any(|v|v[1]-v[0]<0.001){return Err("hue curve x must increase by at least 0.001".into())}
        let ident=y.iter().all(|v|*v==neutral);let m=fc(&x,&y,true);Ok(Self{x,y,m,ident,neutral})
    }
    pub fn at(&self,h:f64)->f64{
        if self.ident{return self.neutral}if !h.is_finite(){return f64::NAN}let n=self.x.len();let x0=self.x[0];
        let mut u=(h-x0).rem_euclid(360.)+x0;if u>=x0+360.{u=x0}
        let i=self.x.partition_point(|x|*x<=u)-1;let(x1,y1)=if i+1<n{(self.x[i+1],self.y[i+1])}else{(x0+360.,self.y[0])};let m1=self.m[(i+1)%n];
        let h=x1-self.x[i];let t=(u-self.x[i])/h;let(t2,t3)=(t*t,t*t*t);
        (2.*t3-3.*t2+1.)*self.y[i]+(t3-2.*t2+t)*h*self.m[i]+(-2.*t3+3.*t2)*y1+(t3-t2)*h*m1
    }
    pub fn is_ident(&self)->bool{self.ident}
}
pub fn hue_sat(pts:&[[f32;2]])->Result<HueCurve,String>{if pts.iter().any(|v|!(0. ..=4.).contains(&v[1])){return Err("hue_sat y must be in 0..4".into())}HueCurve::new(pts,1.)}
pub fn hue_hue(pts:&[[f32;2]])->Result<HueCurve,String>{if pts.iter().any(|v|!(-180. ..=180.).contains(&v[1])){return Err("hue_hue y must be in -180..180 degrees".into())}HueCurve::new(pts,0.)}
pub fn lum_sat(pts:&[[f32;2]])->Result<Curve,String>{if pts.iter().any(|v|!(-1. ..=2.).contains(&v[0])||!(0. ..=4.).contains(&v[1])){return Err("lum_sat x must be in -1..2 and y in 0..4".into())}Curve::new_fc(pts)}
/// Validate hue/lum curve points for param prefix hs/hh/ls.
pub fn validate_extra(prefix:&str,pts:&[[f32;2]])->Result<(),String>{match prefix{"hs"=>hue_sat(pts).map(|_|()),"hh"=>hue_hue(pts).map(|_|()),"ls"=>lum_sat(pts).map(|_|()),_=>Err("invalid curve prefix".into())}}
pub fn apply(pr:&Params,f:&mut Frame,t:f64)->Result<(),String>{super::frame::validate(f)?;
    let mut curves=Vec::new();
    for ch in ["m","r","g","b"]{let count=p(pr,&format!("{ch}_count"),2.,t);if !count.is_finite()||count.fract()!=0.||!(2. ..=32.).contains(&count){return Err("invalid curve count".into())}
        let pts:Vec<_>=(0..count as usize).map(|i|[p(pr,&format!("{ch}_x{i}"),i as f64/(count-1.) as f64,t),p(pr,&format!("{ch}_y{i}"),i as f64/(count-1.) as f64,t)]).collect();curves.push(Curve::new(&pts)?);
    }
    let extra=|ch:&str,hue:bool,ny:f64|->Result<Option<Vec<[f32;2]>>,String>{let count=p(pr,&format!("{ch}_count"),0.,t);if !count.is_finite()||count.fract()!=0.||!(count==0.||(2. ..=32.).contains(&count)){return Err(format!("invalid {ch} curve count"))}if count==0.{return Ok(None)}
        Ok(Some((0..count as usize).map(|i|[p(pr,&format!("{ch}_x{i}"),if hue{i as f64*360./count as f64}else{i as f64/(count-1.) as f64},t),p(pr,&format!("{ch}_y{i}"),ny,t)]).collect()))};
    let hs=match extra("hs",true,1.)?{Some(v)=>Some(hue_sat(&v)?).filter(|c|!c.ident),None=>None};
    let hh=match extra("hh",true,0.)?{Some(v)=>Some(hue_hue(&v)?).filter(|c|!c.ident),None=>None};
    let ls=match extra("ls",false,1.)?{Some(v)=>Some(lum_sat(&v)?).filter(|c|!c.ident),None=>None};
    let hue_on=hs.is_some()||hh.is_some()||ls.is_some();
    for(i,px) in f.d.chunks_exact_mut(4).enumerate(){if i%65536==0{crate::guard::check()}
        for j in 0..3{let mut v=px[j];if !curves[0].ident{v=curves[0].at(v)}if !curves[j+1].ident{v=curves[j+1].at(v)}if !v.is_finite(){return Err("curve overflow".into())}px[j]=v}
        if hue_on{let(r,g,b)=(px[0] as f64,px[1] as f64,px[2] as f64);let y=0.2126*r+0.7152*g+0.0722*b;let cb=(b-y)/1.8556;let cr=(r-y)/1.5748;let rho=(cb*cb+cr*cr).sqrt();
            if rho!=0.{let mut h=cr.atan2(cb).to_degrees();if h<0.{h+=360.}if h>=360.{h-=360.}
                let k=(hs.as_ref().map_or(1.,|c|c.at(h))*ls.as_ref().map_or(1.,|c|c.at(y as f32) as f64)).max(0.);let dh=hh.as_ref().map_or(0.,|c|c.at(h));
                let(cb2,cr2)=if dh==0.{(cb*k,cr*k)}else{let a=(h+dh).to_radians();(rho*k*a.cos(),rho*k*a.sin())};
                let r2=y+1.5748*cr2;let b2=y+1.8556*cb2;let g2=(y-0.2126*r2-0.0722*b2)/0.7152;
                for(j,v) in [r2,g2,b2].into_iter().enumerate(){let v=v as f32;if !v.is_finite(){return Err("curve overflow".into())}px[j]=v}}}
    }
    Ok(())
}
pub fn catalog()->Vec<super::Entry>{vec![("point_curves","Free-position PCHIP master/RGB curves (2..32 pts, linear extrapolation, no clipping) plus Fritsch-Carlson hue_sat/hue_hue/lum_sat (Rec709 chroma, luma kept). Use set_color_curve.",&[("m_count",2.,"2..32"),("r_count",2.,"2..32"),("g_count",2.,"2..32"),("b_count",2.,"2..32"),("hs_count",0.,"hue-vs-sat points, 0 = off"),("hh_count",0.,"hue-vs-hue points, 0 = off"),("ls_count",0.,"lum-vs-sat points, 0 = off")])]}
