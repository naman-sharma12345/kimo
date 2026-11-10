
import asyncio, json, functools, http.server, socketserver, threading, base64
from playwright.async_api import async_playwright
CRATE='/tmp/p1/repo/crates/kimo-gui'
socketserver.ThreadingTCPServer.allow_reuse_address=True
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*a): pass
h=socketserver.ThreadingTCPServer(('127.0.0.1',8541),functools.partial(Q,directory=CRATE)); h.daemon_threads=True
threading.Thread(target=h.serve_forever,daemon=True).start()
PNG='iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=='
evs=[]
for l in open('/tmp/reel/sse_tap.jsonl'):
    e=json.loads(l); e.pop('_rx',None)
    if 'png_b64' in e: e['png_b64']=PNG
    evs.append(e)
async def main():
    async with async_playwright() as p:
        b=await p.chromium.launch(); pg=await b.new_page(viewport={'width':1376,'height':768}); errs=[]
        pg.on('pageerror',lambda e: errs.append(str(e)))
        await pg.goto('http://127.0.0.1:8541/ui/index.html?feed=off'); await pg.wait_for_timeout(800)
        for i in range(0,len(evs),20):
            await pg.evaluate('(es)=>{for(const e of es) window.__kimo.apply(e)}', evs[i:i+20]); await pg.wait_for_timeout(30)
        await pg.wait_for_timeout(1500)
        r=await pg.evaluate("""()=>{const s=window.__kimo.state; const st={}; for(const c of s.calls){st[c.status]=(st[c.status]||0)+1}
          return {callCount:s.callCount, kept:s.calls.length, uniq:new Set(s.calls.map(c=>String(c.id))).size, status:st,
                  dom_rows:document.querySelectorAll('.call[data-call]').length, first_kept:s.calls[0]&&s.calls[0].id, last:s.calls[s.calls.length-1]&&s.calls[s.calls.length-1].name}}""")
        r['pageerrors']=errs; print(json.dumps(r)); await b.close()
asyncio.run(main()); h.shutdown()
