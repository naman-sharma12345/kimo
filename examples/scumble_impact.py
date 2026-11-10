# Detection-box landing impact measured from the SCUMBLE reference (3.72-4.52 s): oversize shrink-in, echo ghosts, flash fill.
import sys; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); k.call("project_set",width=1280,height=720,fps=25)
t=k.call("add_track")["track_id"]; k.call("add_hud_frame",track_id=t,start=0,duration=0.8,tc_start=4.32)
t=k.call("add_track")["track_id"]
k.call("add_detection_box",track_id=t,start=0.0,duration=0.8,x=0.27,y=0.22,w=0.17,h=0.6,label="giant_1",confidence=0.98,color=[0.25,0.85,0.45],caption="a giant on the bridge",impact=True,fill_opacity=0.12)
for i in range(20): k.call("render_frame",time=i*0.04,path=f"/tmp/imp/k_{i+1:02d}.png")
