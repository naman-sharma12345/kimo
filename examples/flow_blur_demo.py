# Optical-flow motion blur on footage: original vs flow-blurred (amount 1.0), plus flow colour-wheel.
import sys,time; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
src=sys.argv[1]; out=sys.argv[2]; blur=float(sys.argv[3]) if len(sys.argv)>3 else 1.0
k=Kimo(); C=k.call; C("project_set",width=1280,height=720,fps=25)
a=C("import_media",path=src)["asset_id"]; t=C("add_track")["track_id"]
c=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=2.0)["clip_id"]
print(C("render_flow",clip_id=c,time=0.8,path="/tmp/flowviz.png"))
C("set_flow_blur",clip_id=c,amount=blur,quality=5)
t0=time.time(); C("export",path=out,crf=17); print("render s",round(time.time()-t0,1))
C("render_frame",time=0.8,path="/tmp/fb_blur.png")
