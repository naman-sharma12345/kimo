# Stress test (3D room, sign on the left wall): camera moves forward and overtakes a sign on the left; sign goes edge-on and leaves frame. Ground truth from a 3D pinhole projection.
import sys,time,math,json; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
W,H,FPS,D=1280,720,25,5.5; N=int(D*FPS); F=900.0
def cam_z(t): s=t/D; return 1.0+3.6*(0.4*s+0.6*s*s)
def proj(X,Y,Z,cz): d=Z-cz; return (W/2+F*X/d, H/2+F*Y/d)
SIGN=[(-1.98,-0.45,5.0),(-1.98,-0.45,6.6),(-1.98,0.45,6.6),(-1.98,0.45,5.0)]
def seg(z0,z1):
    return {"L":[(-2,-1,z0),(-2,-1,z1),(-2,1,z1),(-2,1,z0)],"R":[(2,-1,z1),(2,-1,z0),(2,1,z0),(2,1,z1)],"F":[(-2,1,z0),(2,1,z0),(2,1,z1),(-2,1,z1)][::1],"C":[(-2,-1,z1),(2,-1,z1),(2,-1,z0),(-2,-1,z0)]}
def quad(P,t):
    z=cam_z(t)
    if min(p[2]-z for p in P)<0.25: return [(-9000,-9000),(-8990,-9000),(-8990,-8990),(-9000,-8990)]
    return [proj(x,y,zz,z) for x,y,zz in P]
def inframe(q): xs=[p[0] for p in q]; ys=[p[1] for p in q]; return max(0,min(W,max(xs))-max(0,min(xs)))*max(0,min(H,max(ys))-max(0,min(ys)))
def width_ratio(t): q=quad(SIGN,t); return (math.hypot(q[1][0]-q[0][0],q[1][1]-q[0][1]))
k=Kimo(); C=k.call; C("project_set",width=W,height=H,fps=FPS)
if sys.argv[1]=='plate':
    A={k:C("import_media",path=p)["asset_id"] for k,p in (("L","/tmp/r_left.png"),("R","/tmp/r_right.png"),("F","/tmp/r_floor.png"),("C","/tmp/r_ceil.png"),("B","/tmp/r_back.png"),("S","/tmp/sign.png"))}
    layers=[]; tb=C("add_track")["track_id"]; bk=C("add_clip",track_id=tb,source_type="media",asset_id=A["B"],start=0,duration=D)["clip_id"]
    BACK=[(-2,-1,14),(2,-1,14),(2,1,14),(-2,1,14)]; layers.append((bk,BACK))
    for z0 in range(0,14,2):
        for k,P in seg(z0,z0+2).items():
            tt=C("add_track")["track_id"]; layers.append((C("add_clip",track_id=tt,source_type="media",asset_id=A[k],start=0,duration=D)["clip_id"],P))
    ts=C("add_track")["track_id"]; sg=C("add_clip",track_id=ts,source_type="media",asset_id=A["S"],start=0,duration=D)["clip_id"]; layers.append((sg,SIGN))
    for cl,P in layers: C("pin_layer",clip_id=cl,keys=[{"time":i/FPS,"quad":[v for p in quad(P,i/FPS) for v in p]} for i in range(N+1)])
    for cl,_ in layers: C("set_motion_blur",clip_id=cl,shutter_angle=180)
    t0=time.time(); C("export",path="/tmp/overtake_plate.mp4",crf=16); print("plate render s",round(time.time()-t0,1))
else:
    a=C("import_media",path="/tmp/overtake_plate.mp4")["asset_id"]; t=C("add_track")["track_id"]
    m=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=D)["clip_id"]
    t2=C("add_track")["track_id"]; tx=C("add_text",track_id=t2,text="LOCKED",start=0,duration=D,size=330,color=[0.85,0.05,0.35],stroke_width=14)["clip_id"]
    q0=quad(SIGN,0); r=C("track_planar",clip_id=m,corners=[v/s for p in q0 for v,s in zip(p,(W,H))],target_clip_id=tx,reanchor=bool(__import__("os").environ.get("RA")),return_quads=True)
    print("track s",round(r["seconds"],2),"lost frames",r["lost_frames"],r["lost_ranges"])
    print("frame  sign_w_px  vis_area%  corner_err_px  conf")
    for i,qd in enumerate(r["quads"]):
        g=quad(SIGN,i/FPS); e=sum(math.hypot(qd[2*j]-g[j][0],qd[2*j+1]-g[j][1]) for j in range(4))/4
        if i%6==0 or i==len(r["quads"])-1: print("%4d %9.1f %9.1f %12.2f %9.2f"%(i,width_ratio(i/FPS),100*inframe(g)/(W*H*0.0+max(1,abs((g[1][0]-g[0][0])*(g[3][1]-g[0][1])))),e,r["conf"][i]))
    C("set_motion_blur",clip_id=tx,shutter_angle=180); t0=time.time(); C("export",path=sys.argv[2],crf=17); print("comp render s",round(time.time()-t0,1))
