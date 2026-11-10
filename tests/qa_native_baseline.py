import subprocess,json,hashlib,time,os,tempfile
BIN=os.environ.get("KIMO_BIN","target/gate/kimo-mcp")
PROJECT=os.environ["KIMO_QA_PROJECT"]
OUT=tempfile.mkdtemp(prefix="kimo-qa-native-")
FRAME=OUT+"/native5.png"; SHEET=OUT+"/sheet5.png"
p=subprocess.Popen([BIN],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True);i=0
def rpc(method,params):
 global i;i+=1;p.stdin.write(json.dumps(dict(jsonrpc='2.0',id=i,method=method,params=params))+'\n');p.stdin.flush();return json.loads(p.stdout.readline())
def call(n,a):
 r=rpc('tools/call',dict(name=n,arguments=a))['result'];assert not r['isError'],r;return json.loads(r['content'][0]['text'])
rpc('initialize',{})
call('load_project',dict(path=PROJECT))
t=time.time();call('render_frame',dict(time=5,path=FRAME,inline='never'));print('native5_seconds',time.time()-t,flush=True)
sha=hashlib.sha256(open(FRAME,'rb').read()).hexdigest();print('native5_sha256',sha,'baseline_equal',sha=='ace7fc03852e549edf8b5c12db8454c2ffdd8bd650ef2f3439efa00b6b110de9',flush=True)
assert sha=='ace7fc03852e549edf8b5c12db8454c2ffdd8bd650ef2f3439efa00b6b110de9'
r=call('contact_sheet',dict(times=[5],tile_px=320,labels='none',path=SHEET));print('sheet',json.dumps(r),flush=True)
p.stdin.close();p.wait()

# Independent integer area filter, exactly the documented contact_sheet contract.
def raw(path):
 q=subprocess.run(["ffmpeg","-v","error","-i",path,"-f","rawvideo","-pix_fmt","rgba","-"],capture_output=True,check=True);return q.stdout
import struct
fw,fh=struct.unpack(">II",open(FRAME,"rb").read()[16:24]);src=raw(FRAME);sheet=raw(SHEET);tw,th=r["tile"]["w"],r["tile"]["h"];ref=bytearray(tw*th*4)
for y in range(th):
 y0=y*fh//th;y1=min(fh,max((y+1)*fh//th,y0+1))
 for x in range(tw):
  x0=x*fw//tw;x1=min(fw,max((x+1)*fw//tw,x0+1));n=(x1-x0)*(y1-y0);o=(y*tw+x)*4
  for k in range(3):ref[o+k]=(sum(src[(yy*fw+xx)*4+k] for yy in range(y0,y1) for xx in range(x0,x1))+n//2)//n
  ref[o+3]=255
q=r["tiles"][0];sw=r["width"];got=b"".join(sheet[((q["y"]+y)*sw+q["x"])*4:((q["y"]+y)*sw+q["x"]+tw)*4] for y in range(th))
assert got==ref,"native t=5 contact_sheet differs from downscaled golden by >=1 pixel"
print("contact_sheet native t=5: zero differing pixels; output",OUT)
