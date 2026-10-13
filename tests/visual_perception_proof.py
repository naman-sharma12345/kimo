"""RV26 MCP/CLI frame diff image delivery and deterministic plain composition."""
import json,subprocess,os
BIN=os.environ.get('KIMO_BIN','target/debug/kimo-mcp')
p=subprocess.Popen([BIN],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
i=0
def rpc(m,a):
 global i;i+=1;p.stdin.write(json.dumps({'jsonrpc':'2.0','id':i,'method':m,'params':a})+'\n');p.stdin.flush();return json.loads(p.stdout.readline())['result']
def call(n,a):return rpc('tools/call',{'name':n,'arguments':a})
def j(n,a):
 r=call(n,a);assert not r['isError'],r;return json.loads(r['content'][0]['text'])
rpc('initialize',{})
j('project_set',{'width':128,'height':96,'fps':24})
t=j('add_track',{})['track_id']
for i,c in enumerate([[0,0,0],[1,1,1],[1,0,0]]):j('add_clip',{'track_id':t,'source_type':'solid','color':c,'start':i*2,'duration':2})
r=call('diff_frames',{'a':{'time':1},'b':{'time':3},'heatmap':True,'metrics':['mse','ssim']});assert not r['isError'];assert len([c for c in r['content'] if c['type']=='image'])==1;assert r['structuredContent']['path'];print('PASS heatmap MCP image and structuredContent')
path=os.path.join(__import__('tempfile').mkdtemp(prefix='kimo-visual-'),'cli.kimo');j('save_project',{'path':path});r=subprocess.run([os.path.join(os.path.dirname(os.path.abspath(BIN)),'kimo'),'call','diff_frames','--project',path,'--args',json.dumps({'a':{'time':1},'b':{'time':3},'heatmap':True}),'--json'],capture_output=True,text=True);assert r.returncode==0,r.stderr;out=json.loads(r.stdout);assert 'data' not in json.dumps(out);assert out['heatmap']['path'];print('PASS CLI path without inline bytes')
a=j('detect_cuts',{'return_scores':True});assert [x['time'] for x in a['cuts']]==[2,4]
r=j('match_events',{'a':[2,4],'b':[x['time'] for x in a['cuts']]});assert r['stats']['within_tol_ratio']==1;print('PASS detect->match composition')
p.stdin.close();p.wait()
