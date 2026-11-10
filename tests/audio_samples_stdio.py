"""Planar API over real MCP stdio, undo/redo and roundtrip without home writes."""
import os,tempfile,json,subprocess
with tempfile.TemporaryDirectory()as d:
 env=dict(os.environ,KIMO_ASSET_DIR=d);p=subprocess.Popen([os.environ.get('KIMO_BIN','target/debug/kimo-mcp')],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True);seq=0
 def rpc(m,a):
  global seq;seq+=1;p.stdin.write(json.dumps({'jsonrpc':'2.0','id':seq,'method':m,'params':a})+'\n');p.stdin.flush();return json.loads(p.stdout.readline())['result']
 def c(n,a):
  r=rpc('tools/call',{'name':n,'arguments':a});assert not r['isError'],r;return json.loads(r['content'][0]['text'])
 rpc('initialize',{});before=c('get_project',{});a=c('import_samples',{'rate':48000,'channels':2,'data':[[.125,-.25,.5],[.75,-.5,.25]]});project=c('get_project',{});assert project!=before;c('undo',{});assert c('get_project',{})==before;c('redo',{});assert c('get_project',{})==project
 r=c('read_samples',{'target':'file:'+a['path'],'stage':'source'});assert r['data']==[[.125,-.25,.5],[.75,-.5,.25]];assert r['hash']==a['hash'];assert r['layout']=='planar';print('PASS MCP planar exact/hash import one-undo-step redo')
 r=rpc('tools/call',{'name':'import_samples','arguments':{'rate':48000,'channels':2,'data':[[None],[0]]}});assert r['isError']and r['structuredContent']['code']=='invalid_samples';print('PASS MCP structured raw error')
 p.stdin.close();p.wait()
