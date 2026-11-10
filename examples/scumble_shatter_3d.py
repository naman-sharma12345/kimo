import sys; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); k.call("project_set",width=1280,height=720,fps=25)
D=1.9
def trk(): return k.call("add_track")["track_id"]
import random; random.seed(5)
objs=[{"kind":"null"}]
for r in range(9):
    for c in range(-7,8):
        objs.append({"kind":"rbox","size":[118,60,118],"round":18,"pos":[c*138+(r%2)*55+random.uniform(-4,4),215,120+r*125],"color":[0.12,0.13,0.15],"rough":0.28,"parent":0})
objs.append({"kind":"plane","size":[4000,0,4000],"pos":[0,247,900],"color":[0.06,0.07,0.085],"rough":0.5,"parent":0})
objs.append({"kind":"rbox","size":[330,95,240],"round":24,"pos":[0,190,140],"color":[0.9,0.4,0.12],"rough":0.35,"parent":0,"keys":[{"t":0,"pos":[0,180,140]},{"t":0.3,"pos":[0,196,140]}]})
objs.append({"kind":"capsule","size":[108,1100,0],"pos":[0,-330,140],"color":[0.74,0.46,0.36],"rough":0.5,"parent":0,"rot":[0,0,2],"keys":[{"t":0,"pos":[0,-350,140]},{"t":0.3,"pos":[0,-322,140]},{"t":1.9,"pos":[0,-300,140]}]})
objs.append({"kind":"rbox","size":[2000,60,700],"round":14,"pos":[-100,-120,1500],"emissive":[0.55,0.68,0.85],"color":[0.3,0.35,0.4],"rough":0.9,"parent":0})
t=trk(); c=k.call("add_scene3d",track_id=t,start=0,duration=D,config={"objects":objs,"ambient":[0.05,0.06,0.08],"dof":0.5,"focus":520,"shadows":True,"ao":1.0,"env":1.0,"lights":[{"kind":"directional","dir":[0.35,0.7,-0.4],"color":[0.55,0.72,1.0],"intensity":1.6},{"kind":"directional","dir":[-0.6,0.5,0.7],"color":[1.0,0.8,0.65],"intensity":0.55}]})["clip_id"]
for tm,v in [(0,1.0),(1.0,0.7),(1.5,0.25),(1.9,0.1)]: k.call("set_keyframe",clip_id=c,property="opacity",time=tm,value=v,ease="in_out")
t=trk(); k.call("add_emitter",track_id=t,start=0,duration=1.5,preset="sparks",config={"x":0.47,"y":0.74,"rate":90,"speed":520,"speed_var":300,"angle":300,"spread":70,"life":0.7,"color0":[1,0.8,0.45],"color1":[1,0.4,0.1],"sprite":"streak","size0":3,"size1":1,"gravity":400})
t=trk()
sb=k.call("add_shard_burst",track_id=t,start=0,duration=D,config={"count":470,"origin":[0,170,140],"origin_spread":170,"tint":[0.9,0.93,0.97],"brightness":0.85,"refract":7,"speed":1100,"size_min":5,"size_max":24,"toward_camera":0.35,"spread_time":1.3,"dof":0.3,"drag":0.55,"gravity":60,"physics":True,"collide":True,"restitution":0.5,"dispersion":0.6,"focus":720,"fresnel":0.9,"falloff":0.3,"fade_out":0.35,"spin":420,"seed":21})
ze=k.call("add_effect",clip_id=sb["clip_id"],kind="zoom_blur",params={"amount":0.0})
for tm,v in [(0,0.0),(1.2,0.03),(1.85,0.16)]: k.call("set_keyframe",clip_id=sb["clip_id"],effect_id=ze["effect_id"],property="amount",time=tm,value=v,ease="in")
k.call("set_camera_keyframe",property="cam_z",time=0.0,value=300,ease="in"); k.call("set_camera_keyframe",property="cam_z",time=D,value=800,ease="in")
k.call("set_camera_keyframe",property="cam_y",time=0.0,value=100,ease="in_out"); k.call("set_camera_keyframe",property="cam_y",time=D,value=20,ease="in_out")
k.call("set_camera_keyframe",property="cam_fov",time=0.0,value=42,ease="linear")
boxes=[(0.62,0.35,0.07,0.15,"shard_04",[0.2,0.8,1]),(0.18,0.10,0.07,0.18,"shard_07",[1,0.9,0.2]),(0.70,0.52,0.05,0.15,"shard_12",[1,0.6,0.1]),(0.41,0.55,0.05,0.09,"shard_02",[1,0.3,0.4]),(0.82,0.30,0.04,0.07,"shard_09",[0.2,1,0.6]),(0.30,0.28,0.09,0.15,"shard_01",[0.7,0.4,1])]
for i,(x,y,w,h,l,col) in enumerate(boxes):
    t=trk(); s0=0.6+0.13*i
    k.call("add_detection_box",track_id=t,start=s0,duration=D-s0-0.15,x=x,y=y,w=w,h=h,label=l,confidence=0.7+0.04*i,color=col)
if len(sys.argv)>1 and sys.argv[1]=="export": k.call("export",path="/tmp/scum4.mp4",crf=17)
else:
    for i,tm in enumerate([0.05,0.3,0.6,0.9,1.3,1.8]): k.call("render_frame",time=tm,path=f"/tmp/scum{i}.png")
