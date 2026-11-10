"""RV26 isolated-rustc mutation proofs; control survives and every mutant dies."""
import pathlib,subprocess,tempfile,re
R=pathlib.Path(__file__).resolve().parents[1];D=R/'target/debug/deps';J=next(D.glob('libserde_json-*.rlib'))
analysis=(R/'crates/kimo-core/src/perception/analysis.rs').read_text().split('pub struct Request')[0];analysis=analysis.replace('use crate::{commands::Session, effects::Frame};','use crate::effects::Frame;').replace('use serde_json::Value;','')
diff=(R/'crates/kimo-core/src/ext/diff_frames.rs').read_text();diff=diff[diff.index('pub fn compare'):diff.index('fn run')]
cuts=(R/'crates/kimo-core/src/perception/cuts.rs').read_text();cuts=re.sub(r'^//!.*\n','',cuts,flags=re.M)
harness='''extern crate serde_json;use serde_json::{json,Value};mod guard{pub fn check(){}pub fn reserve(_:usize){}}mod effects{pub struct Frame{pub w:usize,pub h:usize}}mod render{pub fn to_rgba8(_: &crate::effects::Frame)->Vec<u8>{vec![]}}
mod perception{pub mod analysis{ANALYSIS}pub mod cuts{CUTS}}
use perception::analysis::{self,Image};DIFF
fn im(v:f64)->Image{let mut a=Image::new(16,16);for c in 0..3{a.rgb[c].fill(v);}a}
fn main(){let a=im(0.);let b=im(16./255.);assert_eq!(compare(&a,&b,0.04,4,4)["metrics"]["changed_ratio"],1.);assert_eq!(compare(&a,&b,0.1,4,4)["metrics"]["changed_ratio"],0.);let mut blue=a.clone();blue.rgb[2].fill(1.);assert!((analysis::luma_delta(&a,&blue)-0.0722).abs()<1e-9);let mut grad=im(0.);for c in 0..3{for i in 0..256{grad.rgb[c][i]=(i%16)as f64/15.;}}assert!(grad.hist().iter().filter(|&&v|v>0.).count()>=8);let hs=vec![a.hist(),a.hist(),b.hist()];let ms=vec![0.,0.,0.2];let sc=vec![0.,0.,1.];let spatial=vec![0.,0.,0.25];assert!(perception::cuts::detect_stats_spatial(&sc,&ms,&hs,0.1,1,false,Some((&spatial,0.6))).0.is_empty());let f=vec![im(0.),im(0.),im(1.),im(1.)];assert_eq!(perception::cuts::detect(&f,"combined",0.35,1,true).0.len(),1);}
'''
mutants=[('control',analysis,diff,cuts),('diff_threshold',analysis,diff.replace('mx > threshold','mx < threshold'),cuts),('channel',analysis.replace('0.0722 * self.rgb[2][i]','0.0722 * self.rgb[0][i]'),diff,cuts),('bin_count',analysis.replace('self.rgb[0][i] * 8.','self.rgb[0][i] * 1.').replace('self.rgb[1][i] * 8.','self.rgb[1][i] * 1.').replace('self.rgb[2][i] * 8.','self.rgb[2][i] * 1.'),diff,cuts),('cut_threshold',analysis,diff,cuts.replace('scores[i] > threshold','scores[i] < threshold')),('spatial_ratio',analysis,diff,cuts.replace('ratios[i] >= min','ratios[i] <= min'))]
with tempfile.TemporaryDirectory()as t:
 for name,a,d,c in mutants:
  f=pathlib.Path(t)/(name+'.rs');f.write_text(harness.replace('ANALYSIS',a).replace('CUTS',c).replace('DIFF',d));exe=f.with_suffix('');subprocess.run(['rustc','--edition=2021','-Awarnings','-C','opt-level=2',str(f),'-L','dependency='+str(D),'--extern','serde_json='+str(J),'-o',str(exe)],check=True);p=subprocess.run([str(exe)],capture_output=True);assert(p.returncode==0)==(name=='control'),(name,p.stderr);print('PASS',name,'survives'if name=='control'else'killed')
