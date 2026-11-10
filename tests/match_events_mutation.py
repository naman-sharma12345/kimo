"""Inclusive tolerance/global greedy mutation proof using pure preallocated sweep."""
import pathlib,subprocess,tempfile
R=pathlib.Path(__file__).resolve().parents[1];s=(R/'crates/kimo-core/src/ext/match_events.rs').read_text();s=s[s.index('pub fn sweep('):s.index('pub fn nearest(')]
greedy='''pub fn sweep(a:&[(usize,f64)],b:&[(usize,f64)],tol:f64)->(Vec<(usize,usize,f64)>,Vec<usize>,Vec<usize>){let mut e=vec![];for &(i,t)in a{for &(j,u)in b{if(u-t).abs()<=tol{e.push(((u-t).abs(),i,j,u-t));}}}e.sort_by(|x,y|x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));let mut ua=vec![false;a.len()];let mut ub=vec![false;b.len()];let mut p=vec![];for(_,i,j,d)in e{if !ua[i]&&!ub[j]{p.push((i,j,d));ua[i]=true;ub[j]=true;}}p.sort_by_key(|x|x.0);(p,vec![],vec![])}'''
h='''fn main(){let a=[(0,0.),(1,10.)];let b=[(0,9.),(1,11.)];let(p,_,_)=sweep(&a,&b,11.);assert_eq!(p[0],(0,0,9.));let(p,_,_)=sweep(&[(0,0.)],&[(0,0.125)],0.125);assert_eq!(p.len(),1);}'''
with tempfile.TemporaryDirectory()as t:
 for name,code in [('control',s),('tolerance',s.replace('off.abs() <= tol','off.abs() < tol')),('global_greedy',greedy)]:
  f=pathlib.Path(t)/(name+'.rs');f.write_text("mod guard{pub fn check(){}}"+code+h);exe=f.with_suffix('');subprocess.run(['rustc','--edition=2021','-C','opt-level=3',str(f),'-o',str(exe)],check=True);r=subprocess.run([str(exe)],capture_output=True);assert(r.returncode==0)==(name=='control'),(name,r.stderr);print('PASS',name,'survives'if name=='control'else'killed')
