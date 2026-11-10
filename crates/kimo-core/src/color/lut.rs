//! Bounded .cube parser, domain-aware tetrahedral interpolation.
use crate::effects::{p, Frame, Params};
use std::collections::BTreeMap;
#[derive(Clone)]pub struct Lut{pub n:usize,pub data:Vec<[f32;3]>,pub min:[f32;3],pub max:[f32;3]}
impl Lut{
    pub fn parse(text:&str)->Result<Self,String>{
        if text.len()>32*1024*1024{return Err("LUT exceeds 32 MiB".into())}
        let(mut n,mut data,mut min,mut max)=(0usize,Vec::new(),[0.;3],[1.;3]);let(mut seen_min,mut seen_max)=(false,false);
        for(i,line) in text.lines().enumerate(){if i%4096==0{crate::guard::check()}let line=line.split('#').next().unwrap_or("").trim();if line.is_empty(){continue}let tok:Vec<_>=line.split_whitespace().take(5).collect();
            match tok[0]{
                "TITLE"=>continue,
                "LUT_1D_SIZE"=>return Err("1D .cube not supported by this 3D parser".into()),
                "LUT_3D_SIZE"=>{if n!=0||tok.len()!=2{return Err("invalid/duplicate LUT size".into())}n=tok[1].parse().map_err(|_|"invalid LUT size")?;if !(2..=65).contains(&n){return Err("LUT size must be 2..65".into())}crate::guard::reserve(n*n*n*std::mem::size_of::<[f32;3]>());data=Vec::with_capacity(n*n*n)},
                "DOMAIN_MIN"|"DOMAIN_MAX"=>{if !data.is_empty(){return Err("LUT domain must precede data".into())}let v=parse3(&tok[1..])?;if tok[0]=="DOMAIN_MIN"{if seen_min{return Err("duplicate LUT domain minimum".into())}seen_min=true;min=v}else{if seen_max{return Err("duplicate LUT domain maximum".into())}seen_max=true;max=v}},
                _=>{if n==0{return Err("LUT size must precede data".into())}if data.len()>=n*n*n{return Err("too many LUT rows".into())}data.push(parse3(&tok)?)}
            }
        }
        if n==0||data.len()!=n*n*n{return Err("LUT row count mismatch".into())}if (0..3).any(|j|max[j]-min[j]<1e-6){return Err("invalid LUT domain".into())}Ok(Self{n,data,min,max})
    }
    pub fn sample(&self,c:[f32;3])->[f32;3]{
        let mut i=[0;3];let mut f=[0.;3];for j in 0..3{let x=((c[j]-self.min[j])/(self.max[j]-self.min[j])).clamp(0.,1.)*(self.n-1) as f32;i[j]=(x as usize).min(self.n-2);f[j]=x-i[j] as f32}
        let at=|v:[usize;3]|self.data[(v[2]*self.n+v[1])*self.n+v[0]];
        // Sort fractions to select one of the six tetrahedra along the main diagonal.
        let mut axes=[0,1,2];axes.sort_by(|a,b|f[*b].total_cmp(&f[*a]));let mut a=i;a[axes[0]]+=1;let mut b=a;b[axes[1]]+=1;let d=i.map(|v|v+1);
        let(v0,v1,v2,v3)=(at(i),at(a),at(b),at(d));std::array::from_fn(|j|v0[j]+f[axes[0]]*(v1[j]-v0[j])+f[axes[1]]*(v2[j]-v1[j])+f[axes[2]]*(v3[j]-v2[j]))
    }
}
fn parse3(tok:&[&str])->Result<[f32;3],String>{if tok.len()!=3{return Err("expected three LUT numbers".into())}let mut v=[0.;3];for j in 0..3{v[j]=tok[j].parse::<f32>().map_err(|_|"invalid LUT number")?;if !v[j].is_finite()||v[j].abs()>1e6{return Err("nonfinite/out-of-range LUT value".into())}}Ok(v)}
pub fn apply(pr:&Params,strings:&BTreeMap<String,String>,f:&mut Frame,t:f64)->Result<(),String>{
    super::frame::validate(f)?;
    use std::sync::{Mutex,OnceLock,Arc};static CACHE:OnceLock<Mutex<BTreeMap<String,Arc<Lut>>>>=OnceLock::new();
    let l=if let Some(text)=strings.get("cube_text"){
        if strings.contains_key("path"){return Err("LUT requires path OR cube_text, not both".into())}
        // Generated/persisted data is source evidence, no file dependency or timestamp cache.
        Arc::new(Lut::parse(text)?)
    }else{let path_owned=crate::deps::resolve_str(strings.get("path").ok_or("lut needs strings.path or cube_text")?);let path=&path_owned;let meta=std::fs::metadata(path).map_err(|e|e.to_string())?;if !meta.is_file()||meta.len()>32*1024*1024{return Err("LUT requires regular file <=32MiB".into())}
        let key=format!("{path}:{}:{:?}",meta.len(),meta.modified().ok());let cache=CACHE.get_or_init(Default::default);let mut cache=cache.lock().map_err(|_|"LUT cache poisoned")?;
        if let Some(l)=cache.get(&key){l.clone()}else{let l=Arc::new(Lut::parse(&read_bounded(path)?)?);if cache.len()>=8{cache.clear()}cache.insert(key,l.clone());l}
    };
    let amount=p(pr,"intensity",1.,t);if !amount.is_finite(){return Err("invalid LUT intensity".into())}let amount=amount.clamp(0.,1.);
    let failed=std::sync::atomic::AtomicBool::new(false);
    let ptr=crate::par::SyncPtr(f.d.as_mut_ptr());
    crate::par::ranges(f.w*f.h,32768,|a,b|{let ptr=ptr;for i in a..b{if i%65536==0{crate::guard::check()}let px=unsafe{std::slice::from_raw_parts_mut(ptr.0.add(i*4),4)};
let out=l.sample([px[0],px[1],px[2]]);for j in 0..3{let v=px[j] as f64*(1.-amount as f64)+out[j] as f64*amount as f64;if !v.is_finite()||v.abs()>f32::MAX as f64{failed.store(true,std::sync::atomic::Ordering::Relaxed);continue}px[j]=v as f32}}});
    if failed.load(std::sync::atomic::Ordering::Relaxed){return Err("LUT interpolation overflow".into())}
    Ok(())
}

fn read_bounded(path:&str)->Result<String,String>{use std::io::Read;let mut bytes=Vec::new();std::fs::File::open(path).map_err(|e|e.to_string())?.take(32*1024*1024+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;if bytes.len()>32*1024*1024{return Err("LUT exceeds32MiB while reading".into())}String::from_utf8(bytes).map_err(|e|e.to_string())}
