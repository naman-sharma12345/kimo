# simulate_physics demo: 14 layers (discs and polygons, spinning) drop into the frame, collide, bounce and settle. Motion-blurred.
import sys,time,random,math; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
W,H,FPS,D=1280,720,25,5.0; random.seed(4)
k=Kimo(); C=k.call; C("project_set",width=W,height=H,fps=FPS)
bg=C("add_track")["track_id"]; C("add_clip",track_id=bg,source_type="solid",color=[0.07,0.08,0.12],start=0,duration=D)
bodies=[]; pal=[[0.95,0.3,0.3],[0.2,0.7,0.95],[0.98,0.8,0.2],[0.5,0.9,0.5],[0.8,0.4,0.95]]
for i in range(14):
    r=random.randint(38,78); t=C("add_track")["track_id"]
    sh=C("add_shape",track_id=t,shape=random.choice(["ellipse","polygon","polygon"]),sides=random.choice([5,6,8]),start=0,duration=D,width=r*2,height=r*2,fill=pal[i%5],stroke_width=6,stroke_color=[1,1,1])["clip_id"]
    C("set_motion_blur",clip_id=sh,shutter_angle=180)
    bodies.append(dict(clip_id=sh,x=random.randint(-450,450),y=-420-i*95,vx=random.randint(-250,250),vy=0,radius_px=r,spin_deg_s=random.randint(-300,300)))
r=C("simulate_physics",bodies=bodies,gravity=1900,restitution=0.55,friction=0.4,walls="LRB")
t0=time.time(); C("export",path=sys.argv[1],crf=17); print("render s",round(time.time()-t0,1),r)
