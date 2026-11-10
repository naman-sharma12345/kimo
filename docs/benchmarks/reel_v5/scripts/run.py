
import asyncio, base64, json, os, subprocess, sys, threading, time, functools, http.server, socketserver
sys.path.insert(0,'/tmp/p1/repo/examples'); sys.path.insert(0,'/tmp/reel')
from mcp_client import Kimo
import build
from playwright.async_api import async_playwright
R='/tmp/reel'; CRATE='/tmp/p1/repo/crates/kimo-gui'
TCP,SSE,HTTP=7531,7532,8531
FPS=15
events=open(f'{R}/run_events.log','w')
def ev(msg):
    events.write(f"{time.time():.3f} {msg}\n"); events.flush()
relay=subprocess.Popen([sys.executable,'-u','/tmp/p1/repo/crates/kimo-mcp/tools/spectator_sse_relay.py','--tcp',str(TCP),'--sse',str(SSE)],stdout=open(f'{R}/relay.log','w'),stderr=subprocess.STDOUT)
socketserver.ThreadingTCPServer.allow_reuse_address=True
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*a): pass
httpd=socketserver.ThreadingTCPServer(('127.0.0.1',HTTP),functools.partial(Q,directory=CRATE)); httpd.daemon_threads=True
threading.Thread(target=httpd.serve_forever,daemon=True).start()
time.sleep(1.0)
latest=[None]; stop=[False]
ff=subprocess.Popen(['timeout','-k','5','2400','ffmpeg','-y','-loglevel','error','-f','image2pipe','-framerate',str(FPS),'-c:v','mjpeg','-i','-','-vf','format=yuv420p','-c:v','libx264','-preset','veryfast','-crf','18','-r',str(FPS),f'{R}/gui_raw.mp4'],stdin=subprocess.PIPE)
def writer():
    t0=time.time(); n=0
    while not stop[0]:
        if latest[0] is not None:
            ff.stdin.write(latest[0]); n+=1
        nxt=t0+(n+1)/FPS if latest[0] is not None else time.time()+1/FPS
        if latest[0] is None: t0=time.time(); n=0
        d=nxt-time.time()
        if d>0: time.sleep(d)
    ff.stdin.close()
async def main():
    async with async_playwright() as p:
        b=await p.chromium.launch()
        ctx=await b.new_context(viewport={'width':1376,'height':768},device_scale_factor=1)
        pg=await ctx.new_page(); errs=[]
        pg.on('pageerror',lambda e:(errs.append(str(e)),ev(f'PAGEERROR {e}')))
        await pg.goto(f'http://127.0.0.1:{HTTP}/ui/index.html?feed=sse&sse={SSE}')
        cdp=await ctx.new_cdp_session(pg)
        async def onframe(f):
            latest[0]=base64.b64decode(f['data'])
            try: await cdp.send('Page.screencastFrameAck',{'sessionId':f['sessionId']})
            except Exception: pass
        cdp.on('Page.screencastFrame',lambda f: asyncio.ensure_future(onframe(f)))
        await cdp.send('Page.startScreencast',{'format':'jpeg','quality':88,'maxWidth':1376,'maxHeight':768,'everyNthFrame':1})
        await asyncio.sleep(1.0)
        wt=threading.Thread(target=writer,daemon=True); wt.start()
        ev('REC_START')
        await asyncio.sleep(3.0)
        await pg.screenshot(path=f'{R}/shot_idle.png')
        def drive():
            os.environ['KIMO_SPECTATOR']=f'tcp://127.0.0.1:{TCP}'
            k=Kimo(exe='/home/user/tab/kimo/k5-kimo-mcp-bin',timeout=2400)
            ev('ENGINE_UP '+json.dumps(k.server_info))
            KEY={'trim_clip','split_clip','move_clip','add_node','set_grade_output','set_color_curve','get_scopes','render_scope','import_media','add_audio_clip','set_audio','add_audio_effect','set_audio_pan','set_audio_volume_key','measure_loudness','normalize_loudness','project_set','add_adjustment_layer'}
            class Pace:
                def __init__(s): pass
            b=build.B(k,1920,1080,fps=30,log=open(f'{R}/calls.log','w'))
            orig=b.c
            def c(tool,**a):
                ev(f'CALL {tool}')
                r=orig(tool,**a)
                time.sleep(1.5 if tool in KEY else 0.55)
                return r
            b.c=c
            if os.environ.get('SMOKE'):
                b.c('project_set',width=1920,height=1080,fps=30); tr=b.c('add_track')['track_id']; cc=b.c('add_clip',track_id=tr,source_type='gradient',start=0,duration=5,color=[0.05,0.02,0.1],color2=[0.3,0.1,0.4]); b.c('add_text',track_id=b.c('add_track')['track_id'],text='SMOKE',start=0,duration=5,size=200,font='InterDisplay-Black'); b.c('add_effect',clip_id=cc['clip_id'],kind='vignette',params={'amount':0.5})
            else:
                build.build(b,f'{R}/final_raw.mp4')
            ev(f'BUILD_DONE n={b.n} fail={json.dumps(b.fail)}')
            time.sleep(2.0)
            if os.environ.get('SMOKE'): k.close(); return
            ev('EXPORT_START')
            b.n+=1
            try:
                r=k.call('export',path=f'{R}/final_raw.mp4',crf=21,timeout=2400); ev('EXPORT_DONE '+json.dumps(r))
            except Exception as e:
                ev(f'EXPORT_FAIL {e}')
            json.dump({'calls':b.n,'fail':b.fail},open(f'{R}/drive_result.json','w'))
            time.sleep(4.0)
            k.close()
        shots=[]
        async def snapper():
            i=0
            while not stop[0]:
                await asyncio.sleep(20); i+=1
                try: await pg.screenshot(path=f'{R}/snap_{i:03d}.png')
                except Exception: pass
        st=asyncio.ensure_future(snapper())
        await asyncio.to_thread(drive)
        await asyncio.sleep(2.0)
        ev('REC_STOP errs=%d'%len(errs))
        stop[0]=True; st.cancel()
        await cdp.send('Page.stopScreencast')
        wt.join(5)
        await b.close()
try:
    asyncio.run(main())
finally:
    stop[0]=True
    try: relay.kill()
    except Exception: pass
ff.wait(60)
relay.kill(); httpd.shutdown()
ev('ALL_DONE')
