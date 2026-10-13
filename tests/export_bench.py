# export speed + correctness bench: full vs range export frame md5 at crf 0. Needs /tmp/export_bench_src.png (any 1920x1080 image) and, for the pre-change comparison, /tmp/ex_full.mp4. Writes only /tmp.
import sys,time,subprocess;sys.path.insert(0,__import__('os').path.join(__import__('os').path.dirname(__import__('os').path.abspath(__file__)),'..','examples'));from mcp_client import Kimo
k=Kimo();C=k.call;C('project_set',width=1920,height=1080,fps=30)
bg=C('import_media',path='/tmp/export_bench_src.png')['asset_id']
t0=C('add_track')['track_id'];b=C('add_clip',track_id=t0,source_type='media',asset_id=bg,start=0,duration=4)['clip_id']
C('add_effect',clip_id=b,kind='vignette',params={'amount':0.5});C('add_effect',clip_id=b,kind='grain',params={'amount':0.05})
t1=C('add_track')['track_id']
for i in range(6): C('add_text',track_id=t1,text='LAYER %d kimo export bench'%i,start=0.2*i,duration=3.5-0.2*i,x=0.1*i-0.2,y=0.1*i-0.25)
C('add_speed_lines',track_id=t1,start=0.5,duration=2.0) if False else None
import os
t=time.time();C('export',path='/tmp/ex_full3.mp4',crf=0);tf=time.time()-t
t=time.time();C('export',path='/tmp/ex_rng.mp4',crf=0,start=1.0,end=2.0);tr=time.time()-t
print('full 4s: %.1fs  range 1s: %.1fs'%(tf,tr))
def md5s(p,ss=None,n=None):
    cmd=['ffmpeg','-v','error']+(['-ss',str(ss)] if ss else [])+['-i',p]+(['-frames:v',str(n)] if n else [])+['-f','framemd5','-']
    return [l.split(',')[-1].strip() for l in subprocess.run(cmd,capture_output=True,text=True).stdout.splitlines() if l and l[0]!='#']
a=md5s('/tmp/ex_full3.mp4')[30:60];b=md5s('/tmp/ex_rng.mp4')
print('range frames',len(b),'identical to full export frames 30-59:',a==b)
print('new full export identical to pre-change export:',md5s('/tmp/ex_full.mp4')==md5s('/tmp/ex_full3.mp4'))
