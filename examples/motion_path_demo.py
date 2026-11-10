# Spatial motion paths + auto-orient: arrows riding a figure-8 / wave / spiral, constant speed, eased progress, motion blur, echo trail.
import sys, time; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); C=k.call; C("project_set",width=1280,height=720,fps=25); D=4.0
bg=C("add_track")["track_id"]; C("add_clip",track_id=bg,source_type="gradient",c1=[0.05,0.06,0.12],c2=[0.12,0.08,0.2],start=0,duration=D) if False else C("add_clip",track_id=bg,source_type="solid",color=[0.06,0.06,0.1],start=0,duration=D)
def arrow(col,size=70):
    t=C("add_track")["track_id"]; c=C("add_shape",track_id=t,shape="polygon",sides=3,start=0,duration=D,width=size,height=size,fill=col)["clip_id"]; return c
a=arrow([1,0.45,0.1]); C("set_motion_path",clip_id=a,shape="figure8",cx=0.5,cy=0.5,rx=0.34,ry=0.7,auto_orient=True,orient_offset=90,orient_smooth=0.4,animate={"start":0,"end":D,"from":0,"to":2,"ease":"linear"})
C("set_motion_blur",clip_id=a,shutter_angle=240); C("set_echo",clip_id=a,count=8,spacing=0.04,decay=0.6,mode="add")
b=arrow([0.2,0.9,1.0],46); C("set_motion_path",clip_id=b,shape="wave",cx=0.5,cy=0.82,rx=0.45,ry=0.08,auto_orient=True,orient_offset=90,animate={"start":0,"end":D,"from":0,"to":1,"ease":"in_out"}); C("set_motion_blur",clip_id=b,shutter_angle=180)
c=arrow([0.7,1.0,0.3],40); C("set_motion_path",clip_id=c,shape="spiral",cx=0.5,cy=0.5,rx=0.25,ry=0.4,auto_orient=True,orient_offset=90,animate={"start":0,"end":D,"from":0,"to":1,"ease":"out_quart"}); C("set_motion_blur",clip_id=c,shutter_angle=180)
d=arrow([1,0.3,0.6],50); C("set_motion_path",clip_id=d,points=[[0.1,0.2],[0.3,0.1],[0.5,0.35],[0.8,0.15],[0.92,0.3]],smooth=True,auto_orient=True,orient_offset=90,animate={"start":0,"end":D,"from":0,"to":1,"ease":"linear"})
C("set_expression",clip_id=d,property="path",expression={"op":"wiggle","freq":2,"amp":0.0}) if False else None
a_=time.time(); print(C("export",path="/tmp/mpath.mp4",crf=17),"render s",round(time.time()-a_,1))
C("render_frame",time=1.2,path="/tmp/mp_a.png")
