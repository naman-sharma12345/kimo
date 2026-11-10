"""Layer-silhouette collider proof: red particles fall on a blue ellipse shape layer. Without the collider many red pixels land on the
ellipse interior; with a `layer` collider none do. Writes only to /tmp."""
import sys,os,subprocess;sys.path.insert(0,os.path.join(os.path.dirname(__file__),'..','examples'))
from mcp_client import Kimo
W,H=640,360
def frame(coll):
    k=Kimo(); c=k.call; c('project_set',width=W,height=H,fps=25); t1=c('add_track')['track_id']; t2=c('add_track')['track_id']
    sh=c('add_shape',track_id=t1,shape='ellipse',width=260,height=160,fill=[0,0,1,1],start=0,duration=3)['clip_id']
    cfg=dict(shape='line',x=0.5,y=0.02,width=0.4,height=0,rate=400,life=3,speed=60,speed_var=20,angle=90,spread=15,gravity=700,sprite='hard',size0=4,size1=4,color0=[1,0,0],color1=[1,0,0],alpha0=1,alpha1=1,seed=7,bounce=0.5)
    if coll: cfg['colliders']=[dict(kind='layer',clip_id=sh)]
    c('add_emitter',track_id=t2,start=0,duration=3,config=cfg); p='/tmp/plc.png'; c('render_frame',time=1.5,path=p)
    return subprocess.run(['ffmpeg','-v','error','-i',p,'-f','rawvideo','-pix_fmt','rgb24','-'],capture_output=True,check=True).stdout
def red_inside(b):
    n=0
    for y in range(H):
        for x in range(W):
            dx,dy=(x-W/2)/(130-8),(y-H/2)/(80-8)
            if dx*dx+dy*dy<1 and b[(y*W+x)*3]>128 and b[(y*W+x)*3+2]<80: n+=1
    return n
free=red_inside(frame(False)); hit=red_inside(frame(True)); print('red pixels on the ellipse interior: no collider',free,'layer collider',hit)
assert free>100 and hit<=free*0.03; print('layer collider proof OK')
