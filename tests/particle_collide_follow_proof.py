"""collide_follow proof: a blue ellipse slides right at ~167 px/s through a field of static red dots (no gravity, no velocity).
With `follow` each dot is pushed out of the ellipse wherever the ellipse was at the dot's own time, so the ellipse plows dots ahead of it and the corridor behind it holds fewer.
Without follow the collider is frozen at the current frame, so dots in the corridor behind are untouched. Writes only to /tmp."""
import sys,os,subprocess;sys.path.insert(0,os.path.join(os.path.dirname(__file__),'..','examples'))
from mcp_client import Kimo
W,H=640,360; T=1.5
def count(follow):
    k=Kimo(); c=k.call; c('project_set',width=W,height=H,fps=25); t1=c('add_track')['track_id']; t2=c('add_track')['track_id']
    sh=c('add_shape',track_id=t1,shape='ellipse',width=120,height=80,fill=[0,0,1,1],start=0,duration=3)['clip_id']
    c('set_keyframe',clip_id=sh,property='x',time=0,value=-250,ease='linear'); c('set_keyframe',clip_id=sh,property='x',time=3,value=250,ease='linear')
    cfg=dict(shape='rect',x=0.5,y=0.5,width=1.0,height=0.2,rate=250,life=3,speed=0,speed_var=0,sprite='hard',size0=2,size1=2,color0=[1,0,0],color1=[1,0,0],alpha0=1,alpha1=1,seed=9,bounce=0.0,friction=0.0,colliders=[dict(kind='layer',clip_id=sh,follow=follow)])
    c('add_emitter',track_id=t2,start=0,duration=3,config=cfg); p='/tmp/pcf.png'; c('render_frame',time=T,path=p)
    b=subprocess.run(['ffmpeg','-v','error','-i',p,'-f','rawvideo','-pix_fmt','rgb24','-'],capture_output=True,check=True).stdout
    cur=W/2-250+T*500/3; left=cur-60
    corr=sum(1 for y in range(150,210) for x in range(80,int(left)-4) if b[(y*W+x)*3]>128 and b[(y*W+x)*3+2]<80)
    ahead=sum(1 for y in range(150,210) for x in range(int(cur+60)+4,W) if b[(y*W+x)*3]>128 and b[(y*W+x)*3+2]<80)
    inside=sum(1 for y in range(100,260) for x in range(0,W) if b[(y*W+x)*3]>128 and b[(y*W+x)*3+2]<80 and ((x-cur)/(58-(10 if follow else 0)))**2+((y-180)/(38-(10 if follow else 0)))**2<1)  # follow uses 0.1 s bins: the silhouette can be ~8 px off the exact frame
    assert inside==0, f'{inside} resting dots left inside the current silhouette (follow={follow})'
    return corr,ahead
s=count(False); f=count(True)
print('red pixels (corridor behind, ahead of the ellipse): frozen collider',s,' follow',f)
assert f[0]<0.8*s[0], 'follow should clear the corridor behind the ellipse'
print('collide_follow proof OK')
