# Mesh tracker proof: a waving flag (known per-vertex ground truth), tracked, then text pinned to the deforming surface.
import sys,time,math,json; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
W,H,FPS,D=1280,720,25,4.0; N=int(D*FPS); NX,NY=9,6
def verts(t):
    out=[]; cx=640+60*math.sin(t*0.9); cy=360+20*math.sin(t*1.3)
    for j in range(NY):
        for i in range(NX):
            u,v=i/(NX-1),j/(NY-1); x=cx+(u-0.5)*760*(1-0.06*math.sin(t*0.9)*(u-0.5)); y=cy+(v-0.5)*400
            amp=34*u**1.2; ph=2*math.pi*(u*1.4-t*0.9); y+=amp*math.sin(ph)+ (v-0.5)*10*math.sin(ph+1); x-=amp*0.35*(1-math.cos(ph))
            out+= [x,y]
    return out
k=Kimo(); C=k.call; C("project_set",width=W,height=H,fps=FPS)
if sys.argv[1]=='plate':
    bg=C("add_track")["track_id"]; C("add_clip",track_id=bg,source_type="solid",color=[0.55,0.7,0.85],start=0,duration=D)
    a=C("import_media",path="/tmp/flag.png")["asset_id"]; t=C("add_track")["track_id"]; f=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=D)["clip_id"]
    C("set_mesh_pin",clip_id=f,nx=NX,ny=NY,keys=[{"time":i/FPS,"verts":verts(i/FPS)} for i in range(N)]); C("set_motion_blur",clip_id=f,shutter_angle=120)
    t0=time.time(); C("export",path="/tmp/flagplate.mp4",crf=16); print("plate render s",round(time.time()-t0,1))
else:
    a=C("import_media",path="/tmp/flagplate.mp4")["asset_id"]; t=C("add_track")["track_id"]; m=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=D)["clip_id"]
    t2=C("add_track")["track_id"]; tx=C("add_text",track_id=t2,text="LOCKED\nON FLAG",start=0,duration=D,size=190,color=[1,0.85,0.1]); tx=tx["clip_id"]
    v0=verts(0); q=[v0[0],v0[1],v0[(NX-1)*2],v0[(NX-1)*2+1],v0[((NY-1)*NX+NX-1)*2],v0[((NY-1)*NX+NX-1)*2+1],v0[(NY-1)*NX*2],v0[(NY-1)*NX*2+1]]
    r=C("track_mesh",clip_id=m,corners=[q[i]/(W if i%2==0 else H) for i in range(8)],nx=NX,ny=NY,target_clip_id=tx,verts0=[v0[i]/(W if i%2==0 else H) for i in range(len(v0))],return_verts=True)
    errs=[]; 
    for i,fv in enumerate(r["verts"]):
        g=verts(i/FPS); errs.append(sum(math.hypot(fv[2*k]-g[2*k],fv[2*k+1]-g[2*k+1]) for k in range(NX*NY))/(NX*NY))
    s=sorted(errs); print("mesh track s",round(r["seconds"],1),"frames",r["frames"],"mean conf",round(r["mean_conf"],2)); print("mean vertex err px: mean %.2f median %.2f p95 %.2f max %.2f"%(sum(errs)/len(errs),s[len(s)//2],s[int(len(s)*.95)],s[-1]))
    fv=r["verts"][-1]; g=verts((len(r["verts"])-1)/FPS)
    for j in range(NY): print(" ".join("%5.1f"%math.hypot(fv[2*(j*NX+i)]-g[2*(j*NX+i)],fv[2*(j*NX+i)+1]-g[2*(j*NX+i)+1]) for i in range(NX)))
    print([round(e,1) for e in errs[::10]])
    t0=time.time(); C("export",path=sys.argv[2],crf=17); print("comp render s",round(time.time()-t0,1))
