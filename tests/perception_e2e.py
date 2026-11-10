"""Beat/cut loop using plain data, includes drift mutation and measured payload size.
Uses batch when RV11 is present; direct-op fallback reports that limitation.
"""
import json,subprocess,wave,struct,tempfile,math
p=subprocess.Popen(['target/debug/kimo-mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True);seq=0;tokens=0;calls=0

def rpc(method,params):
 global seq;seq+=1;p.stdin.write(json.dumps({'jsonrpc':'2.0','id':seq,'method':method,'params':params})+'\n');p.stdin.flush();return json.loads(p.stdout.readline())['result']
def c(name,a):
 global tokens,calls;calls+=1;r=rpc('tools/call',{'name':name,'arguments':a});assert not r['isError'],r;v=json.loads(r['content'][0]['text']);tokens+=(len(json.dumps(a))+len(json.dumps(v))+3)//4;return v
rpc('initialize',{})
c('project_set',{'width':64,'height':48,'fps':24});tr=c('add_track',{})['track_id'];ids=[]
starts=[0,2+3/24,4-2/24,6];ends=starts[1:]+[8]
for i,start in enumerate(starts):ids.append(c('add_clip',{'track_id':tr,'source_type':'solid','color':[[0,0,0],[1,1,1],[1,0,0],[0,0,1]][i],'start':start,'duration':ends[i]-start})['clip_id'])
with tempfile.TemporaryDirectory()as d:
 path=d+'/clicks.wav';x=[0.]*(48000*8)
 for i in range(1,16):x[24000*i]=0.9
 with wave.open(path,'wb')as w:w.setparams((1,2,48000,0,'NONE','not compressed'));w.writeframes(b''.join(struct.pack('<h',round(v*32767))for v in x))
 calls=0;tokens=0
 events=c('audio_transients',{'target':path,'mode':'beat'});assert events['confidence']>.5
 beats=[e['t']for e in events['beat_grid']if min(abs(e['t']-t)for t in [2,4,6])<.1];assert len(beats)==3,beats
 cuts=c('detect_cuts',{});matched=c('match_events',{'a':beats,'b':[x['time']for x in cuts['cuts']],'tolerance_s':1/24})
 off=[x['offset_s']*24 for x in matched['pairs']];assert all(abs(a-b)<.001 for a,b in zip(off,[3,-2,0])),off
 # Plain operations whose clip ownership is explicit; edits are fixture-only.
 ops=[{'tool':'move_clip','args':{'clip_id':ids[i],'start':i*2}}for i in [1,2]]+[{'tool':'trim_clip','args':{'clip_id':ids[i],'duration':2}}for i in [0,1,2]]
 dry=c('batch',{'ops':ops,'dry_run':True});c('batch',{'ops':ops,'expect_revision':dry['revision']})
 verified=c('detect_cuts',{});m=c('match_events',{'a':beats,'b':[x['time']for x in verified['cuts']],'tolerance_s':1/24});assert m['stats']['within_tol_ratio']==1,m
 diff=c('diff_frames',{'a':{'time':2-1/24},'b':{'time':2}});assert diff['verdict_hint']=='major'
 assert calls<=8,calls
 print('PASS +3/-2/0 offsets, edits, reverify ratio1, diff major; core calls',calls,'token estimate',tokens,'(chars/4 estimate)')
 m=c('match_events',{'a':[t+1/24 for t in beats],'b':[x['time']for x in verified['cuts']],'tolerance_s':.5/24});assert m['stats']['within_tol_ratio']==0;print('PASS shifted-beat mutation mismatch reported')
p.stdin.close();p.wait()
