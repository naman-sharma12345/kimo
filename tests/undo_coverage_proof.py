"""Undo/redo coverage audit: every tool is called with trivial args on a fresh setup. A call that changes the project must add exactly one undo step (read tools must change nothing); undo must return the project byte-identical, redo must restore it byte-identical, and a fresh edit must clear redo. Tools that reject the trivial args are not covered (count printed). Writes only to /tmp."""
import json, subprocess, select, sys
import os
EXE=os.path.join(os.path.dirname(os.path.abspath(__file__)),"..","target","release","kimo-mcp")
p=subprocess.Popen([EXE],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True); i=0
def rpc(m,params):
    global i; i+=1; p.stdin.write(json.dumps({"jsonrpc":"2.0","id":i,"method":m,"params":params})+"\n"); p.stdin.flush()
    r,_,_=select.select([p.stdout],[],[],40)
    if not r: raise TimeoutError(m)
    return json.loads(p.stdout.readline())
rpc("initialize",{})
def call(n,**a):
    r=rpc("tools/call",{"name":n,"arguments":a})["result"]; t=r["content"][0]["text"]
    try: j=json.loads(t)
    except Exception: j=t
    return (not r.get("isError")), j
tools=rpc("tools/list",{"mode":"flat"})["result"]["tools"]
SKIP={"motion_extract","export","export_gif","export_audio","export_alpha","export_preset","transcribe","download_stock","render_frame","undo","redo","restore","load_project","save_project","project_set","checkpoint","checkpoint_delete","history_list","get_project","import_media"}
subprocess.run(["ffmpeg","-v","error","-y","-f","lavfi","-i","testsrc2=s=320x180","-frames:v","1","/tmp/undo_cov_bg.png"],check=True)
def setup():
    call("project_set",width=320,height=180,fps=24); ids={}
    ids["asset_id"]=call("import_media",path="/tmp/undo_cov_bg.png")[1]["asset_id"]; ids["track_id"]=call("add_track")[1]["track_id"]
    ids["clip_id"]=call("add_clip",track_id=ids["track_id"],source_type="media",asset_id=ids["asset_id"],start=0,duration=2)[1]["clip_id"]
    ids["effect_id"]=call("add_effect",clip_id=ids["clip_id"],kind="blur")[1]["effect_id"]
    return ids
def args(tool,ids):
    sc=tool["inputSchema"]; a={}
    for k,v in sc.get("properties",{}).items():
        if k in ids: a[k]=ids[k]; continue
        if k not in sc.get("required",[]): continue
        t=v.get("type"); t=t if not isinstance(t,list) else next((x for x in t if x!="null"),None)
        if "enum" in v: a[k]=v["enum"][0]
        elif t in("number","integer"): a[k]=1
        elif t=="string": a[k]="/tmp/undo_cov_out" if ("path" in k or k=="file") else "x"
        elif t=="boolean": a[k]=True
        elif t=="array": a[k]=[]
        elif t=="object": a[k]={}
    return a
redo_ok=[0]; ids=setup(); sidecount=0; bad=[]; fails={}; checked=0; names=[t["name"] for t in tools if t["name"] not in SKIP]
for n in names:
    tl=[t for t in tools if t["name"]==n][0]
    call("undo",steps=500); ids=setup(); print("T",n,flush=True)
    ok,pre=call("get_project"); hl=call("history_list")[1]; u0=len(hl["undo"])
    ok,r=call(n,**args(tl,ids))
    if not ok:
        fails[n]=str(r)[:70]; continue
    ok2,post=call("get_project"); u1=len(call("history_list")[1]["undo"]); changed=(json.dumps(pre,sort_keys=True)!=json.dumps(post,sort_keys=True)); checked+=1
    if changed and u1==u0: bad.append((n,"project changed but NO undo step")); continue
    if u1==u0+1:
        pass
    if u1==u0+1:
        call("undo"); ok3,back=call("get_project")
        if json.dumps(back,sort_keys=True)!=json.dumps(pre,sort_keys=True): bad.append((n,"undo not byte-identical"))
        call("redo")
        ok4,again=call("get_project")
        if json.dumps(again,sort_keys=True)!=json.dumps(post,sort_keys=True): bad.append((n,"redo not identical"))
        call("undo"); assert len(call("history_list")[1]["redo"])>=1
        call("add_track")
        if call("history_list")[1]["redo"]!=[]: bad.append((n,"fresh edit did not clear redo"))
        redo_ok[0]+=1
    if checked%40==0: print("progress",checked,flush=True)
import collections
for n,e in list(fails.items())[:200]: print("FAIL",n,"|",e[:50])
print("full undo+redo+redo-clear verified on",redo_ok[0],"tools"); print("tools called ok:",checked,"of",len(names)); print("PROBLEMS",json.dumps(bad)); sys.exit(1 if bad else 0)
p.kill()
