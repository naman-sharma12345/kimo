"""Particle colliders and vortex proof. Writes only to /tmp.
Sparks fall on a circle collider: with the collider no particle ends up deep inside it; without it many do.
Default (no colliders, no vortex) renders bit-identical to a config that spells those fields out as off."""
import sys,os,subprocess,hashlib;sys.path.insert(0,os.path.join(os.path.dirname(__file__),'..','examples'))
from mcp_client import Kimo
W,H=640,360
def frame(cfg,t=1.5):
    k=Kimo(); c=k.call; c('project_set',width=W,height=H,fps=25); tr=c('add_track')['track_id']
    c('add_emitter',track_id=tr,start=0,duration=3,config=cfg); p='/tmp/pc.png'; c('render_frame',time=t,path=p)
    return subprocess.run(['ffmpeg','-v','error','-i',p,'-f','rawvideo','-pix_fmt','rgb24','-'],capture_output=True,check=True).stdout
base=dict(shape='line',x=0.5,y=0.02,width=0.5,height=0,rate=400,life=3,speed=60,speed_var=20,angle=90,spread=20,gravity=700,sprite='hard',size0=4,size1=4,color0=[1,0,0],color1=[1,0,0],alpha0=1,alpha1=1,seed=7)
a=frame(base); b=frame(dict(base,colliders=[],vortex=0.0)); print('default == explicit off:',a==b); assert a==b
circ=dict(kind='circle',x=0.5,y=0.6,r=0.2)
def inside(buf):
    n=0
    for y in range(H):
        for x in range(W):
            dx,dy=x-0.5*W,y-0.6*H
            if dx*dx+dy*dy<(0.2*H-6)**2 and buf[(y*W+x)*3]>128 and buf[(y*W+x)*3+1]<80: n+=1
    return n
free=inside(a); hit=inside(frame(dict(base,colliders=[circ],bounce=0.5)))
print('red pixels deep inside the circle: without collider',free,'with collider',hit); assert free>200 and hit<=free*0.02
v=frame(dict(base,vortex=900,vortex_x=0.5,vortex_y=0.5)); print('vortex changes the frame:',v!=a); assert v!=a
print('particle collide proof OK')
