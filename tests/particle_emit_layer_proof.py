"""Emit-from-layer proof: shape "layer" spawns particles on a clip's alpha silhouette. Static particles (speed 0, no forces) of red hard sprites
over a blue ellipse: every red pixel must lie inside the ellipse (plus sprite radius) and none outside; edge mode puts them on the outline only. Writes only to /tmp."""
import sys,os,subprocess;sys.path.insert(0,os.path.join(os.path.dirname(__file__),'..','examples'))
from mcp_client import Kimo
W,H=640,360; RX,RY=130,80
def frame(edge):
    k=Kimo(); c=k.call; c('project_set',width=W,height=H,fps=25); t1=c('add_track')['track_id']; t2=c('add_track')['track_id']
    sh=c('add_shape',track_id=t1,shape='ellipse',width=2*RX,height=2*RY,fill=[0,0,1,1],start=0,duration=3)['clip_id']
    cfg=dict(shape='layer',emit_clip=sh,emit_edge=edge,rate=2000,life=3,speed=0,speed_var=0,sprite='hard',size0=3,size1=3,color0=[1,0,0],color1=[1,0,0],alpha0=1,alpha1=1,seed=5,prewarm=True)
    c('add_emitter',track_id=t2,start=0,duration=3,config=cfg); p='/tmp/pel.png'; c('render_frame',time=1.0,path=p)
    return subprocess.run(['ffmpeg','-v','error','-i',p,'-f','rawvideo','-pix_fmt','rgb24','-'],capture_output=True,check=True).stdout
def stats(b):
    ins=out=edge=0
    for y in range(H):
        for x in range(W):
            if b[(y*W+x)*3]>128 and b[(y*W+x)*3+2]<80:
                q=(((x-W/2)/RX)**2+((y-H/2)/RY)**2)**0.5
                if q>1.05: out+=1
                else: ins+=1
                if q>0.9: edge+=1
    return ins,out,edge
a=stats(frame(False)); e=stats(frame(True)); print('area mode (inside, outside, near-edge):',a,' edge mode:',e)
assert a[0]>300 and a[1]<=a[0]*0.02; assert e[0]>100 and e[2]>=0.9*(e[0]+e[1]) and a[2]<0.5*(a[0]+a[1])
print('emit-from-layer proof OK')
