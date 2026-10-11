//! Free-position shape-preserving cubic curves. Endpoints extrapolate linearly.
use crate::effects::{p,Frame,Params};
#[derive(Clone,Debug)]pub struct Curve {x:Vec<f32>,y:Vec<f32>,m:Vec<f32>}
impl Curve {
    pub fn new(points:&[[f32;2]])->Result<Self,String>{
        if !(2..=32).contains(&points.len()){return Err("curve needs 2..32 points".into())}
        if points.iter().flatten().any(|v|!v.is_finite()||v.abs()>16.){return Err("curve coordinates must be finite and within +/-16".into())}
        let x:Vec<_>=points.iter().map(|v|v[0]).collect();let y:Vec<_>=points.iter().map(|v|v[1]).collect();
        let h:Vec<_>=x.windows(2).map(|v|v[1]-v[0]).collect();if h.iter().any(|v|*v<1e-5){return Err("curve x must increase by at least 0.00001".into())}
        let d:Vec<_>=y.windows(2).zip(&h).map(|(v,h)|(v[1]-v[0])/h).collect();let n=x.len();let mut m=vec![0.;n];
        if n==2 {m[0]=d[0];m[1]=d[0]}else{
            let endpoint=|h0:f32,h1:f32,d0:f32,d1:f32|{let a=((2.*h0+h1)*d0-h0*d1)/(h0+h1);if a*d0<=0.{0.}else if d0*d1<0.&&a.abs()>3.*d0.abs(){3.*d0}else{a}};
            m[0]=endpoint(h[0],h[1],d[0],d[1]);m[n-1]=endpoint(h[n-2],h[n-3],d[n-2],d[n-3]);
            for i in 1..n-1{if d[i-1]*d[i]>0.{let w1=2.*h[i]+h[i-1];let w2=h[i]+2.*h[i-1];m[i]=(w1+w2)/(w1/d[i-1]+w2/d[i])}}
        }Ok(Self{x,y,m})
    }
    pub fn at(&self,v:f32)->f32{
        if !v.is_finite(){return f32::NAN}let n=self.x.len();if v<=self.x[0]{return self.y[0]+(v-self.x[0])*self.m[0]}if v>=self.x[n-1]{return self.y[n-1]+(v-self.x[n-1])*self.m[n-1]}
        let i=self.x.partition_point(|x|*x<=v)-1;let h=self.x[i+1]-self.x[i];let t=(v-self.x[i])/h;let(t2,t3)=(t*t,t*t*t);
        (2.*t3-3.*t2+1.)*self.y[i]+(t3-2.*t2+t)*h*self.m[i]+(-2.*t3+3.*t2)*self.y[i+1]+(t3-t2)*h*self.m[i+1]
    }
}
pub fn apply(pr:&Params,f:&mut Frame,t:f64)->Result<(),String>{super::frame::validate(f)?;
    let mut curves=Vec::new();
    for ch in ["m","r","g","b"]{let count=p(pr,&format!("{ch}_count"),2.,t);if !count.is_finite()||count.fract()!=0.||!(2. ..=32.).contains(&count){return Err("invalid curve count".into())}
        let pts:Vec<_>=(0..count as usize).map(|i|[p(pr,&format!("{ch}_x{i}"),i as f64/(count-1.) as f64,t),p(pr,&format!("{ch}_y{i}"),i as f64/(count-1.) as f64,t)]).collect();curves.push(Curve::new(&pts)?);
    }
    for(i,px) in f.d.chunks_exact_mut(4).enumerate(){if i%65536==0{crate::guard::check()}for j in 0..3{let v=curves[j+1].at(curves[0].at(px[j]));if !v.is_finite(){return Err("curve overflow".into())}px[j]=v}}
    Ok(())
}
pub fn catalog()->Vec<super::Entry>{vec![("point_curves","Free-position PCHIP master/RGB curves, 2..32 points per channel. Use set_color_curve. Linear endpoint extrapolation preserves extended-range values; no automatic clipping.",&[("m_count",2.,"2..32"),("r_count",2.,"2..32"),("g_count",2.,"2..32"),("b_count",2.,"2..32")])]}
