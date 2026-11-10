# Text on path (circle, wave, figure8) with animated start + random-order reveal animator.
import sys,time; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); C=k.call; C("project_set",width=1280,height=720,fps=25); D=4.0
bg=C("add_track")["track_id"]; C("add_clip",track_id=bg,source_type="solid",color=[0.04,0.05,0.1],start=0,duration=D)
def txt(text,size,col,shape,**kw):
    t=C("add_track")["track_id"]; c=C("add_text",track_id=t,text=text,start=0,duration=D,size=size,color=col)["clip_id"]; C("set_text_path",clip_id=c,shape=shape,**kw); return c
a=txt("KIMO  TEXT ON A PATH  KIMO  TEXT ON A PATH  ",64,[1,0.75,0.2],"ellipse",cx=0.5,cy=0.5,rx=0.22,ry=0.4,start=0.0,animate={"start":0,"end":D,"from":0,"to":0.18,"ease":"linear"})
b=txt("WAVE RIDER  WAVE RIDER  WAVE",52,[0.3,0.85,1],"wave",cx=0.5,cy=0.5,rx=0.45,ry=0.12,start=0.03)
C("add_text_animator",clip_id=b,preset="reveal",unit="char",random=True,seed=7,duration=2.0)
c=txt("RANDOM ORDER",96,[1,0.3,0.6],"arc",cx=0.5,cy=0.9,rx=0.3,ry=0.18,start=0.2)
C("add_text_animator",clip_id=c,preset="pop",unit="char",random=True,seed=3,duration=2.5,delay=0.5)
t0=time.time(); C("export",path=sys.argv[1],crf=17); print("render s",round(time.time()-t0,1))
C("render_frame",time=1.5,path="/tmp/tp.png")
