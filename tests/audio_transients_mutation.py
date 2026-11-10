"""RV-26 mutation proof: removing min_gap or breaking threshold must kill fixture assertions.
Compiles only the pure module at opt3; no cargo concurrency and no repository edits.
"""
import os,pathlib,subprocess,tempfile
root=pathlib.Path(__file__).resolve().parents[1];deps=root/'target/debug/deps'
libs={n:next(deps.glob('lib'+n+'-*.rlib')) for n in ['serde','serde_json']}
code=(root/'crates/kimo-core/src/perception/audio_events.rs').read_text()
harness='''extern crate serde;extern crate serde_json;
mod guard{pub fn check(){}pub fn reserve(_:usize){}}
#[path="MODULE"]mod events;
fn main(){let mut mono=vec![0f32;48000*3];for i in 0..28{mono[12000+i*4800]=0.9;}let mut p=events::Params::default();p.min_gap_ms=200.;let a=events::analyze(&mono,&p).unwrap();assert!(!a.transients.is_empty());for w in a.transients.windows(2){assert!(w[1].t-w[0].t>=0.2-1e-12);}p.min_gap_ms=80.;let a=events::analyze(&mono,&p).unwrap();assert_eq!(a.count,28);}
'''
with tempfile.TemporaryDirectory() as t:
 t=pathlib.Path(t)
 for name,c in [('control',code),('gap',code.replace('let gap = p.min_gap_ms / 1000.;','let gap = 0.0;')),('threshold',code.replace('let floor = 0.001;','let floor = 2.0;'))]:
  module=t/(name+'.rs');module.write_text(c);src=t/(name+'_main.rs');src.write_text(harness.replace('MODULE',str(module)));exe=t/(name+'_bin')
  subprocess.run(['rustc','--edition=2021','-C','opt-level=3',str(src),'-L','dependency='+str(deps),'--extern','serde='+str(libs['serde']),'--extern','serde_json='+str(libs['serde_json']),'-o',str(exe)],check=True)
  result=subprocess.run([str(exe)],capture_output=True)
  assert (result.returncode==0)==(name=='control'),(name,result.returncode,result.stderr)
  print('PASS',name,'survives' if name=='control' else 'killed')
