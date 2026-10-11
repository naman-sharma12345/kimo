//! Explicit float color conversion. No claim of ACES RRT/ODT or OCIO compatibility.
use crate::effects::{p, Frame, Params};
pub type Mat = [[f64; 3]; 3];
const ID: Mat = [[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]];
const BRADFORD: Mat = [[0.8951,0.2664,-0.1614],[-0.7502,1.7135,0.0367],[0.0389,-0.0685,1.0296]];
pub fn mv(m: Mat, x: [f64;3]) -> [f64;3] { m.map(|r| r.iter().zip(x).map(|(a,b)| a*b).sum()) }
fn mm(a: Mat,b: Mat)->Mat { std::array::from_fn(|i|std::array::from_fn(|j|(0..3).map(|k|a[i][k]*b[k][j]).sum())) }
fn inv(a: Mat)->Mat {
    let d=a[0][0]*(a[1][1]*a[2][2]-a[1][2]*a[2][1])-a[0][1]*(a[1][0]*a[2][2]-a[1][2]*a[2][0])+a[0][2]*(a[1][0]*a[2][1]-a[1][1]*a[2][0]);
    std::array::from_fn(|i|std::array::from_fn(|j| {let (r,s)=((j+1)%3,(j+2)%3);let (u,v)=((i+1)%3,(i+2)%3);(a[r][u]*a[s][v]-a[r][v]*a[s][u])/d}))
}
fn xyz(xy:[f64;2])->[f64;3] {[xy[0]/xy[1],1.,(1.-xy[0]-xy[1])/xy[1]]}
/// 0 Rec.709, 1 Rec.2020, 2 P3-D65, 3 ACES AP1, 4 ACES AP0.
pub fn chroma(g:usize)->([[f64;2];3],[f64;2]) {
    match g {
        1=>([[0.708,0.292],[0.170,0.797],[0.131,0.046]],[0.3127,0.3290]),
        2=>([[0.680,0.320],[0.265,0.690],[0.150,0.060]],[0.3127,0.3290]),
        3=>([[0.713,0.293],[0.165,0.830],[0.128,0.044]],[0.32168,0.33767]),
        5=>([[0.684,0.313],[0.221,0.848],[0.0861,-0.102]],[0.3127,0.3290]),
        6=>([[0.730,0.280],[0.140,0.855],[0.100,-0.050]],[0.3127,0.3290]),
        7=>([[0.766,0.275],[0.225,0.800],[0.089,-0.087]],[0.3127,0.3290]),
        4=>([[0.7347,0.2653],[0.,1.],[0.0001,-0.0770]],[0.32168,0.33767]),
        _=>([[0.640,0.330],[0.300,0.600],[0.150,0.060]],[0.3127,0.3290])
    }
}
pub fn rgb_xyz(g:usize)->Mat { let(c,w)=chroma(g);let cols=c.map(xyz);let a=std::array::from_fn(|i|std::array::from_fn(|j|cols[j][i]));let scale=mv(inv(a),xyz(w));std::array::from_fn(|i|std::array::from_fn(|j|a[i][j]*scale[j])) }
pub fn conversion(from:usize,to:usize)->Mat {
    if from==to{return ID}
    let (_,wf)=chroma(from);let(_,wt)=chroma(to);let a=mv(BRADFORD,xyz(wf));let b=mv(BRADFORD,xyz(wt));
    let diag=std::array::from_fn(|i|std::array::from_fn(|j|if i==j {b[i]/a[i]}else{0.}));
    mm(inv(rgb_xyz(to)),mm(mm(inv(BRADFORD),mm(diag,BRADFORD)),rgb_xyz(from)))
}
fn signed_pow(x:f64,e:f64)->f64{x.abs().powf(e).copysign(x)}
/// Transfer ids: 0 linear, 1 sRGB, 2 display gamma 2.4, 3 ACEScct,
/// 4 LogC3 EI800, 5 SLog3, 6 ST2084 PQ, 7 HLG scene OETF, 8 BT709 OETF.
/// PQ is display-linear relative to reference_white nits; HLG is scene-relative (no OOTF).
pub fn decode(s:usize,x:f64,white:f64)->f64 {
    match s {
        1=>if x.abs()<=0.04045{x/12.92}else{((x.abs()+0.055)/1.055).powf(2.4).copysign(x)},
        2=>signed_pow(x,2.4),
        3=>if x<=0.155251141552511{(x-0.0729055341958355)/10.5402377416545}else{2f64.powf(x*17.52-9.72)},
        4=>if x>0.1496582{(10f64.powf((x-0.385537)/0.24719)-0.052272)/5.555556}else{(x-0.092809)/5.367655},
        5=>{let t=x*1023.;if t>=171.2102946929{10f64.powf((t-420.)/261.5)*0.19-0.01}else{(t-95.)*0.01125/(171.2102946929-95.)}},
        6=>{let v=x.clamp(0.,1.).powf(1./78.84375);((v-0.8359375).max(0.)/(18.8515625-18.6875*v)).powf(1./0.1593017578125)*10000./white},
        7=>if x<=0.5{x.max(0.).powi(2)/3.}else{(((x-0.55991073)/0.17883277).exp()+0.28466892)/12.},
        8=>if x.abs()<0.081{x/4.5}else{((x.abs()+0.099)/1.099).powf(1./0.45).copysign(x)},
        _=>x
    }
}
pub fn encode(s:usize,x:f64,white:f64)->f64 {
    match s {
        1=>if x.abs()<=0.0031308{x*12.92}else{(1.055*x.abs().powf(1./2.4)-0.055).copysign(x)},
        2=>signed_pow(x,1./2.4),
        3=>if x<=0.0078125{10.5402377416545*x+0.0729055341958355}else{(x.log2()+9.72)/17.52},
        4=>if x>0.010591{0.24719*(5.555556*x+0.052272).log10()+0.385537}else{5.367655*x+0.092809},
        5=>if x>=0.01125{(420.+((x+0.01)/0.19).log10()*261.5)/1023.}else{(x*(171.2102946929-95.)/0.01125+95.)/1023.},
        6=>{let v=(x*white/10000.).clamp(0.,1.).powf(0.1593017578125);((0.8359375+18.8515625*v)/(1.+18.6875*v)).powf(78.84375)},
        7=>if x<=1./12.{(3.*x.max(0.)).sqrt()}else{0.17883277*(12.*x-0.28466892).ln()+0.55991073},
        8=>if x.abs()<0.018{x*4.5}else{(1.099*x.abs().powf(0.45)-0.099).copysign(x)},
        _=>x
    }
}
/// Compress out-of-gamut chroma towards neutral on a constant-luminance ray.
/// Bounds all channels without independent clipping when luma is in 0..1.
pub fn gamut_compress(c:[f64;3])->[f64;3] { compress_with(c,[0.2126,0.7152,0.0722]) }
pub fn luma_weights(gamut:usize)->[f64;3] { rgb_xyz(gamut)[1] }
fn compress_with(c:[f64;3],weights:[f64;3])->[f64;3] {
    let y=(weights[0]*c[0]+weights[1]*c[1]+weights[2]*c[2]).clamp(0.,1.);let mut s: f64=1.;
    for v in c {let d=v-y;if d>0.{s=s.min((1.-y)/d)}else if d<0.{s=s.min(-y/d)}}
    c.map(|v|(y+(v-y)*s.clamp(0.,1.)).clamp(0.,1.))
}
pub fn catalog()->Vec<super::Entry>{vec![("color_managed","Explicit float gamut + transfer conversion with Bradford white adaptation. Gamut IDs 0 Rec709,1 Rec2020,2 P3D65,3 ACES AP1,4 AP0,5 ARRI WG3,6 Sony SGamut3,7 SGamut3Cine. Transfer IDs 0 linear,1 sRGB,2 gamma2.4,3 ACEScct,4 LogC3 EI800,5 SLog3,6 PQ,7 HLG scene,8 BT709. No ACES RRT/ODT. Tone_map is luminance Reinhard, not an ACES output transform.",&[("from_gamut",0.,"0..7"),("to_gamut",0.,"0..7"),("from_transfer",1.,"0..8"),("to_transfer",1.,"0..8"),("reference_white",100.,"PQ linear 1.0 in nits, 1..10000"),("exposure",0.,"stops -20..20"),("tone_map",0.,"0 none,1 luminance Reinhard"),("gamut_compress",0.,"0 preserve unbounded values,1 bound linear RGB")]) ]}
pub fn apply(pr:&Params,f:&mut Frame,t:f64)->Result<(),String> {super::frame::validate(f)?;
    let get=|k:&str,d:f64|->Result<f64,String>{let v=p(pr,k,d,t) as f64;if !v.is_finite(){return Err(format!("nonfinite {k}"))}Ok(v)};
    let id=|k:&str,max:usize|->Result<usize,String>{let v=get(k,if k.ends_with("transfer"){1.}else{0.})?;if v.fract()!=0.||v<0.||v>max as f64{return Err(format!("invalid {k}"))}Ok(v as usize)};
    let from=id("from_gamut",7)?;let to=id("to_gamut",7)?;let m=conversion(from,to);let weights=luma_weights(to);let a=id("from_transfer",8)?;let b=id("to_transfer",8)?;
    let white=get("reference_white",100.)?;if !(1. ..=10000.).contains(&white){return Err("reference_white outside 1..10000".into())}
    let e=2f64.powf(get("exposure",0.)?.clamp(-20.,20.));let tone=get("tone_map",0.)?>0.5;let compress=get("gamut_compress",0.)?>0.5;
    for(i,px) in f.d.chunks_exact_mut(4).enumerate() {if i%65536==0{crate::guard::check()}
        let mut c=mv(m,[decode(a,px[0] as f64,white),decode(a,px[1] as f64,white),decode(a,px[2] as f64,white)]).map(|v|v*e);
        if tone {let y=(weights[0]*c[0]+weights[1]*c[1]+weights[2]*c[2]).max(0.);c=c.map(|v|v/(1.+y));}
        if compress {c=compress_with(c,weights)}
        for j in 0..3 {let out=encode(b,c[j],white);if !out.is_finite()||out.abs()>f32::MAX as f64{return Err("color conversion overflow".into())}px[j]=out as f32}
    }Ok(())
}

/// Explicit Bradford adaptation in one linear RGB gamut, source and target xy declared.
pub fn white_adaptation(g:usize,source:[f64;2],target:[f64;2])->Result<Mat,String>{if g>7||[source,target].iter().any(|p|p.iter().any(|v|!v.is_finite())||p[0]<=0.||p[1]<=0.||p[0]+p[1]>=1.){return Err("physical white xy requires x>0,y>0,x+y<1".into())}let a=mv(BRADFORD,xyz(source));let b=mv(BRADFORD,xyz(target));if a.iter().any(|v|v.abs()<1e-8){return Err("singular white adaptation".into())}let diag=std::array::from_fn(|i|std::array::from_fn(|j|if i==j{b[i]/a[i]}else{0.}));Ok(mm(inv(rgb_xyz(g)),mm(mm(inv(BRADFORD),mm(diag,BRADFORD)),rgb_xyz(g))))}
