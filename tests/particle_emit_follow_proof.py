"""emit_follow proof: a blue ellipse slides right at 100 px/s; static red particles (life 1 s) are emitted from it.
Without emit_follow every particle spawns on the silhouette as it is NOW, so their centroid sits on the ellipse.
With emit_follow each spawns on the silhouette at its own birth time, so the cloud trails behind (centroid left of the ellipse by about speed x half the life). Writes only to /tmp."""
import sys,os,subprocess;sys.path.insert(0,os.path.join(os.path.dirname(__file__),'..','examples'))
from mcp_client import Kimo
W,H=640,360; T=1.5
def frame(follow):
    k=Kimo(); c=k.call; c('project_set',width=W,height=H,fps=25); t1=c('add_track')['track_id']; t2=c('add_track')['track_id']
    sh=c('add_shape',track_id=t1,shape='ellipse',width=120,height=80,fill=[0,0,1,1],start=0,duration=3)['clip_id']
    c('set_keyframe',clip_id=sh,property='x',time=0,value=-150,ease='linear'); c('set_keyframe',clip_id=sh,property='x',time=3,value=150,ease='linear')
    cfg=dict(shape='layer',emit_clip=sh,emit_follow=follow,rate=1500,life=1.0,speed=0,speed_var=0,sprite='hard',size0=2,size1=2,color0=[1,0,0],color1=[1,0,0],alpha0=1,alpha1=1,seed=5)
    c('add_emitter',track_id=t2,start=0,duration=3,config=cfg); p='/tmp/pef.png'; c('render_frame',time=T,path=p)
    b=subprocess.run(['ffmpeg','-v','error','-i',p,'-f','rawvideo','-pix_fmt','rgb24','-'],capture_output=True,check=True).stdout
    xs=[x for y in range(H) for x in range(W) if b[(y*W+x)*3]>128 and b[(y*W+x)*3+2]<80]; blue=[x for y in range(H) for x in range(W) if b[(y*W+x)*3+2]>128 and b[(y*W+x)*3]<80]
    return sum(xs)/len(xs), len(xs), (sum(blue)/len(blue) if blue else None)
a=frame(False); b=frame(True)
print('centroid x of red particles: static silhouette %.1f (n=%d), follow %.1f (n=%d); ellipse centre now x=%.1f'%(a[0],a[1],b[0],b[1],a[2] if a[2] else -1))
cur=W/2-150+100*T
assert abs(a[0]-cur)<12, 'static mode should sit on the current silhouette'
assert cur-b[0]>25, 'follow mode should trail the moving layer'
print('emit_follow proof OK')
