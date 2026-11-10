# Planar tracker proof: synthetic wall plate with a perspective camera move + occluder, ground-truth corner error, then text locked to the sign.
import sys,time,math,json; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
def solve(src,dst):
    A=[]
    for (x,y),(u,v) in zip(src,dst): A.append([x,y,1,0,0,0,-u*x,-u*y,u]); A.append([0,0,0,x,y,1,-v*x,-v*y,v])
    for c in range(8):
        m=max(range(c,8),key=lambda r:abs(A[r][c])); A[c],A[m]=A[m],A[c]; d=A[c][c]; A[c]=[a/d for a in A[c]]
        for r in range(8):
            if r!=c: f=A[r][c]; A[r]=[a-f*b for a,b in zip(A[r],A[c])]
    return [A[i][8] for i in range(8)]+[1]
def ap(h,x,y): w=h[6]*x+h[7]*y+1; return ((h[0]*x+h[1]*y+h[2])/w,(h[3]*x+h[4]*y+h[5])/w)
W,H,FPS,D=1280,720,25,5.0; N=int(D*FPS)
def wallquad(t):
    s=t/D; z=1.05+0.18*math.sin(s*math.pi); ang=0.10*math.sin(s*2*math.pi); cx,cy=640+120*math.sin(s*math.pi*1.5),360+50*math.cos(s*math.pi*2); persp=60*math.sin(s*math.pi*1.2)
    pts=[(0,0),(W,0),(W,H),(0,H)]; out=[]
    for i,(x,y) in enumerate(pts):
        dx,dy=(x-W/2)*z,(y-H/2)*z; rx,ry=dx*math.cos(ang)-dy*math.sin(ang),dx*math.sin(ang)+dy*math.cos(ang); sh=[ -persp,persp,-persp*0.3,persp*0.3][i]
        out.append((cx+rx+sh*(1 if i in(0,3) else -1)*0.5,cy+ry+sh*(0.8 if i<2 else -0.8)))
    return out
sign=[tuple(p) for p in json.load(open('/tmp/wall_sign.json'))]
def gt(i): q=wallquad(i/FPS); h=solve([(0,0),(W,0),(W,H),(0,H)],q); return [ap(h,x,y) for x,y in sign]
if sys.argv[1]=='plate':
    k=Kimo(); C=k.call; C("project_set",width=W,height=H,fps=FPS); a=C("import_media",path="/tmp/wall.png")["asset_id"]; t=C("add_track")["track_id"]
    w=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=D)["clip_id"]
    for i in range(N+1):
        q=wallquad(i/FPS); C("pin_layer",clip_id=w,time=i/FPS,quad=[v for p in q for v in p])
    C("set_motion_blur",clip_id=w,shutter_angle=180)
    t2=C("add_track")["track_id"]; o=C("add_shape",track_id=t2,shape="ellipse",start=1.9,duration=1.3,width=300,height=300,fill=[0.1,0.5,0.9])["clip_id"]
    C("set_keyframe",clip_id=o,property="x",time=0,value=-500); C("set_keyframe",clip_id=o,property="x",time=1.3,value=500); C("set_keyframe",clip_id=o,property="y",time=0,value=-20); C("set_keyframe",clip_id=o,property="y",time=1.3,value=60)
    t0=time.time(); C("export",path="/tmp/wallplate.mp4",crf=16); print("plate render s",round(time.time()-t0,1))
else:
    k=Kimo(); C=k.call; C("project_set",width=W,height=H,fps=FPS); a=C("import_media",path="/tmp/wallplate.mp4")["asset_id"]; t=C("add_track")["track_id"]
    m=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=D)["clip_id"]
    t2=C("add_track")["track_id"]; tx=C("add_text",track_id=t2,text="LOCKED\nTO WALL",start=0,duration=D,size=230,color=[0.85,0.05,0.35])["clip_id"]
    q0=gt(0); r=C("track_planar",clip_id=m,corners=[v/s for p in q0 for v,s in zip(p,(W,H))],target_clip_id=tx,reanchor=bool(__import__("os").environ.get("RA")),light=0.0,occluder_matte=True,return_quads=True)
    errs=[]
    for i,qd in enumerate(r["quads"]):
        g=gt(i); e=[math.hypot(qd[2*j]-g[j][0],qd[2*j+1]-g[j][1]) for j in range(4)]; errs.append(sum(e)/4)
    srt=sorted(errs); print("track s",round(r["seconds"],2),"frames",r["frames"],"lost",r["lost_frames"],r["lost_ranges"],"mean conf",round(r["mean_conf"],3),"min conf",round(r["min_conf"],3))
    print("corner err px: mean %.3f median %.3f p95 %.3f max %.3f"%(sum(errs)/len(errs),srt[len(srt)//2],srt[int(len(srt)*.95)],srt[-1]))
    C("set_motion_blur",clip_id=tx,shutter_angle=180)
    t0=time.time(); C("export",path=sys.argv[2],crf=17); print("comp render s",round(time.time()-t0,1))
