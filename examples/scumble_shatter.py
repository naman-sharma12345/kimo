import sys; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); k.call("project_set",width=1280,height=720,fps=25)
D=1.9
bg=k.call("import_media",path="/tmp/sc/kb.png")["asset_id"]; fa=k.call("import_media",path="/tmp/sc/finger.png")["asset_id"]
def trk(): return k.call("add_track")["track_id"]
t=trk(); c=k.call("add_clip",track_id=t,source_type="media",asset_id=bg,start=0,duration=D)["clip_id"]
for tm,v in [(0,1.0),(D,1.35)]: k.call("set_keyframe",clip_id=c,property="scale",time=tm,value=v,ease="in")
for tm,v in [(0,0.9),(0.9,0.75),(1.3,0.35),(1.9,0.18)]: k.call("set_keyframe",clip_id=c,property="opacity",time=tm,value=v,ease="in_out")
e=k.call("add_effect",clip_id=c,kind="blur",params={"radius":1.0}); 
try:
    for tm,v in [(0,1),(1.0,6),(1.9,14)]: k.call("set_keyframe",clip_id=c,effect_id=e["effect_id"],property="radius",time=tm,value=v,ease="in")
except Exception as ex: print("blurkf",ex)
t=trk(); c2=k.call("add_clip",track_id=t,source_type="media",asset_id=fa,start=0,duration=1.2)["clip_id"]
for tm,v in [(0,1.0),(1.2,2.3)]: k.call("set_keyframe",clip_id=c2,property="scale",time=tm,value=v,ease="in")
for tm,v in [(0,0),(0.9,0),(1.2,-500)]: k.call("set_keyframe",clip_id=c2,property="y",time=tm,value=v,ease="in")
for tm,v in [(0,1),(0.8,0.9),(1.2,0)]: k.call("set_keyframe",clip_id=c2,property="opacity",time=tm,value=v,ease="in")
t=trk(); k.call("add_emitter",track_id=t,start=0,duration=1.5,preset="sparks",config={"x":0.47,"y":0.74,"rate":90,"speed":520,"speed_var":300,"angle":300,"spread":70,"life":0.7,"color0":[1,0.8,0.45],"color1":[1,0.4,0.1],"sprite":"streak","size0":3,"size1":1,"gravity":400})
t=trk()
sb=k.call("add_shard_burst",track_id=t,start=0,duration=D,config={"count":470,"origin":[0,150,260],"origin_spread":260,"tint":[0.9,0.93,0.97],"brightness":0.85,"refract":7,"speed":1100,"size_min":8,"size_max":40,"toward_camera":0.35,"spread_time":1.3,"dof":0.3,"drag":0.55,"gravity":60,"physics":True,"collide":True,"restitution":0.5,"dispersion":0.6,"focus":720,"fresnel":0.9,"falloff":0.3,"fade_out":0.35,"spin":420,"seed":21})
ze=k.call("add_effect",clip_id=sb["clip_id"],kind="zoom_blur",params={"amount":0.0})
for tm,v in [(0,0.0),(1.2,0.03),(1.85,0.16)]: k.call("set_keyframe",clip_id=sb["clip_id"],effect_id=ze["effect_id"],property="amount",time=tm,value=v,ease="in")
k.call("set_camera_keyframe",property="cam_z",time=0.0,value=0,ease="in"); k.call("set_camera_keyframe",property="cam_z",time=D,value=330,ease="in")
boxes=[(0.62,0.35,0.07,0.15,"shard_04",[0.2,0.8,1]),(0.18,0.10,0.07,0.18,"shard_07",[1,0.9,0.2]),(0.70,0.52,0.05,0.15,"shard_12",[1,0.6,0.1]),(0.41,0.55,0.05,0.09,"shard_02",[1,0.3,0.4]),(0.82,0.30,0.04,0.07,"shard_09",[0.2,1,0.6]),(0.30,0.28,0.09,0.15,"shard_01",[0.7,0.4,1])]
for i,(x,y,w,h,l,col) in enumerate(boxes):
    t=trk(); s0=0.6+0.13*i
    k.call("add_detection_box",track_id=t,start=s0,duration=D-s0-0.15,x=x,y=y,w=w,h=h,label=l,confidence=0.7+0.04*i,color=col)
if len(sys.argv)>1 and sys.argv[1]=="export": k.call("export",path="/tmp/scum.mp4",crf=17)
else:
    for i,tm in enumerate([0.05,0.3,0.6,0.9,1.3,1.8]): k.call("render_frame",time=tm,path=f"/tmp/scum{i}.png")
