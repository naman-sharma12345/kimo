import sys,os,time,subprocess,json; sys.path.insert(0,__import__('os').path.join(__import__('os').path.dirname(__import__('os').path.abspath(__file__)),'..','examples'))
from mcp_client import Kimo
def attempt(k,tool,**a):
    try: r=k.rpc("tools/call",{"name":tool,"arguments":a}); return r["isError"], r["content"][0]["text"][:140]
    except Exception as e: return "DIED",repr(e)
k=Kimo(); k.call("project_set",width=1280,height=720,fps=25); t=k.call("add_track")["track_id"]
print("huge number   ",attempt(k,"add_box_expand",track_id=t,start=1e12))
print("huge canvas   ",attempt(k,"project_set",width=100000,height=100000,fps=25))
print("bad fps       ",attempt(k,"project_set",width=1280,height=720,fps=1e9))
print("huge duration ",attempt(k,"add_hud_frame",track_id=t,start=0,duration=1e7))
print("still alive   ",attempt(k,"project_info") if False else attempt(k,"list_effects"))
k.p.kill()
# memory budget: 200 MB budget, 4K frame (4K frame float RGBA = 130 MB; a render needs several)
env=dict(os.environ,KIMO_MEM_MB="150"); 
k=Kimo.__new__(Kimo); k.p=subprocess.Popen(["target/release/kimo-mcp"],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,env=env); k.i=0; k.rpc("initialize",{})
k.call("project_set",width=3840,height=2160,fps=25); t=k.call("add_track")["track_id"]; k.call("add_clip",track_id=t,source_type="solid",color=[1,0,0],start=0,duration=1)
print("mem budget    ",attempt(k,"render_frame",time=0.5,path="/tmp/sbx.png"))
print("alive after   ",attempt(k,"list_effects"))
k.p.kill()
# time limit: 1 s limit on an export
env=dict(os.environ,KIMO_EXPORT_TIMEOUT_S="1")
k=Kimo.__new__(Kimo); k.p=subprocess.Popen(["target/release/kimo-mcp"],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,env=env); k.i=0; k.rpc("initialize",{})
k.call("project_set",width=1920,height=1080,fps=25); t=k.call("add_track")["track_id"]; k.call("add_clip",track_id=t,source_type="solid",color=[1,0,0],start=0,duration=600)
a=time.time(); print("time limit    ",attempt(k,"export",path="/tmp/sbx.mp4",crf=30), round(time.time()-a,1),"s")
print("alive after   ",attempt(k,"list_effects"))
k.p.kill()
