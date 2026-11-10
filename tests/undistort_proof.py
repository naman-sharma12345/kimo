import numpy as np, subprocess, json
W,H,F,K1,K2=640,360,500.0,-0.15,0.02
cx,cy=W/2,H/2
def tex(x,y):
    r=0.5+0.25*np.sin(x/37.0+y/53.0)+0.25*np.sin(x/23.0-y/31.0)
    g=0.5+0.3*np.sin(x/61.0)+0.2*np.cos(y/29.0)
    b=0.5+0.3*np.sin((x+y)/47.0)+0.2*np.sin(x/17.0)
    return np.stack([r,g,b],-1).clip(0,1)
yy,xx=np.mgrid[0:H,0:W].astype(float)
# distorted pixel -> ideal pixel by fixed-point inversion
xd=(xx+0.5-cx)/F; yd=(yy+0.5-cy)/F; xu,yu=xd.copy(),yd.copy()
for _ in range(60):
    r2=xu*xu+yu*yu; s=1+K1*r2+K2*r2*r2; xu=xd/s; yu=yd/s
D=tex(xu*F+cx-0.5,yu*F+cy-0.5)
ideal=tex(xx,yy)
raw=(D*255+0.5).astype(np.uint8).tobytes()
open("/tmp/dist.rgb","wb").write(raw*24)
subprocess.run(["ffmpeg","-y","-loglevel","error","-f","rawvideo","-pix_fmt","rgb24","-s",f"{W}x{H}","-r","24","-i","/tmp/dist.rgb","-c:v","libx264","-crf","10","-pix_fmt","yuv420p","/tmp/dist.mp4"])
srv=subprocess.Popen(["target/release/kimo-mcp"],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True);m=[0]
def rpc(n,p=None):
    m[0]+=1;srv.stdin.write(json.dumps({"jsonrpc":"2.0","id":m[0],"method":n,"params":p or {}})+"\n");srv.stdin.flush();return json.loads(srv.stdout.readline())
def call(n,a):
    r=rpc("tools/call",{"name":n,"arguments":a})["result"];t=r["content"][0]["text"]
    if r.get("isError"): raise SystemExit(n+": "+t)
    return json.loads(t)
rpc("initialize",{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"p","version":"0"}})
call("project_set",{"width":W,"height":H,"fps":24});a=call("import_media",{"path":"/tmp/dist.mp4"})["asset_id"];tr=call("add_track",{})["track_id"]
c=call("add_clip",{"track_id":tr,"source_type":"media","asset_id":a,"start":0,"duration":1})["clip_id"]
r=call("undistort_clip",{"clip_id":c,"k1":K1,"k2":K2,"focal_px":F,"out_path":"/tmp/undist.mp4"});print(r)
o=np.frombuffer(subprocess.run(["ffmpeg","-loglevel","error","-i","/tmp/undist.mp4","-frames:v","1","-f","rawvideo","-pix_fmt","rgb24","-"],capture_output=True).stdout,np.uint8).reshape(H,W,3)/255.0
dd=np.frombuffer(subprocess.run(["ffmpeg","-loglevel","error","-i","/tmp/dist.mp4","-frames:v","1","-f","rawvideo","-pix_fmt","rgb24","-"],capture_output=True).stdout,np.uint8).reshape(H,W,3)/255.0
valid=(o.sum(-1)>0.02)
print("coverage",valid.mean(),"MAE undistorted vs ideal",np.abs(o-ideal)[valid].mean(),"MAE distorted-input vs ideal",np.abs(dd-ideal).mean())
