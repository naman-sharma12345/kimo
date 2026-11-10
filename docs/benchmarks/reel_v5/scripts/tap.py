
import json, urllib.request, sys, time
out=open('/tmp/reel/sse_tap.jsonl','w')
req=urllib.request.urlopen('http://127.0.0.1:7532/events',timeout=3600)
n=0
for raw in req:
    line=raw.decode('utf-8','replace').strip()
    if not line.startswith('data: '): continue
    try: e=json.loads(line[6:])
    except Exception: out.write(json.dumps({'_bad':line[:200]})+'\n'); continue
    if 'png_b64' in e: e['png_b64']=len(e['png_b64'])
    e['_rx']=time.time(); out.write(json.dumps(e)+'\n'); out.flush()
