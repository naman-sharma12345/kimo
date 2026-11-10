"""Synthetic engine proof, measured exports + real-id hostile color tools. Run at repo root."""
import json,subprocess,time,select,pathlib
p=subprocess.Popen(['target/release/kimo-mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=open('/tmp/color-engine-stderr.log','w'),text=True);seq=0
out=pathlib.Path('/downloads');out.mkdir(exist_ok=True)
def rpc(name,**args):
 global seq
 seq+=1;p.stdin.write(json.dumps(dict(jsonrpc='2.0',id=seq,method='tools/call',params=dict(name=name,arguments=args)))+'\n');p.stdin.flush()
 if not select.select([p.stdout],[],[],30)[0]:raise TimeoutError(name)
 r=json.loads(p.stdout.readline())['result'];return r

def call(name,**args):
 r=rpc(name,**args)
 if r.get('isError'):raise RuntimeError(r)
 return json.loads(r['content'][0]['text'])
call('project_set',width=640,height=360,fps=24);tr=call('add_track')['track_id']
a=call('add_clip',track_id=tr,source_type='gradient',color=[0.02,0.01,0.03],color2=[0.8,0.6,0.25],start=0,duration=2)['clip_id']
# RGB hue turn with explicit domain; 33^3 cube generated procedurally, NOT measured film stock.
lut=out/'color-proof.cube';s=['TITLE "synthetic channel cross-talk"','LUT_3D_SIZE 33','DOMAIN_MIN 0 0 0','DOMAIN_MAX 1 1 1']
for b in range(33):
 for g in range(33):
  for r in range(33):s.append(f'{.92*r/32+.08*g/32} {.9*g/32+.1*b/32} {.7*b/32+.3*r/32}')
lut.write_text('\n'.join(s))
call('render_frame',time=1,path=str(out/'color-lut-before.png'))
call('add_effect',clip_id=a,kind='lut',strings=dict(path=str(lut)))
start=time.perf_counter();call('export',path=str(out/'color-lut-proof.mp4'));print('lut_export_seconds',time.perf_counter()-start)
call('render_frame',time=1,path=str(out/'color-lut-after.png'))
start=time.perf_counter();v=call('analyze_color',time=1,clip_id=a);print('scope_seconds',time.perf_counter()-start);print('scope_summary',{k:v[k] for k in ['pixels','min','max','mean','under_range','over_range']})
call('render_scope',time=1,clip_id=a,scope='parade',path=str(out/'color-parade.png'))
# Hostile args on every color tool, including render calls with actual clip id.
n=0
for tool in ['set_color_curve','analyze_color','match_color','pick_white_balance','render_scope']:
 for args in [{},{'clip_id':a,'time':0},{'clip_id':a,'time':1e12},{'clip_id':a,'time':0,'bins':1e12},{'clip_id':a,'time':0,'range_min':1,'range_max':-1},{'clip_id':a,'channel':'m','points':[[0,0],[0,1]]},{'clip_id':a,'channel':'m','points':[[0,0],[1,1e12]]}]:
  rpc(tool,**args);n+=1
for kind,params in [('color_managed',{'from_gamut':1e8}),('point_curves',{'m_count':1e8})]:
 fx=call('add_effect',clip_id=a,kind=kind,params=params)['effect_id'];r=rpc('render_frame',time=0,path=str(out/'hostile-color.png'));assert r.get('isError');call('remove_effect',clip_id=a,effect_id=fx);n+=1
for text in ['LUT_3D_SIZE 1\n0 0 0','LUT_3D_SIZE 999999999999999','LUT_3D_SIZE 2\nNaN 0 0']:
 bad=out/'bad-color.cube';bad.write_text(text);fx=call('add_effect',clip_id=a,kind='lut',strings=dict(path=str(bad)))['effect_id'];r=rpc('render_frame',time=0,path=str(out/'hostile-color.png'));assert r.get('isError');call('remove_effect',clip_id=a,effect_id=fx);n+=1
print('targeted_hostile_calls',n,'server_alive',call('get_project')['width']);p.terminate()
