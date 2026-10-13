"""A real edit built only through the grouped interface (kimo_search, kimo_describe, group calls). usage: grouped_edit_proof.py SRC.mp4 OUT.mp4"""
import sys,json,time;sys.path.insert(0,__import__('os').path.join(__import__('os').path.dirname(__import__('os').path.abspath(__file__)),'..','examples'));from mcp_client import Kimo
SRC=sys.argv[1] if len(sys.argv)>1 else '/downloads/city-49844-76b1f705.mp4'
k=Kimo(); st={'search':0,'describe':0,'call':0,'errors':0}; errs=[]
def find(q):
    st['search']+=1; r=k.call('kimo_search',query=q,limit=5); return r
def desc(a):
    st['describe']+=1; return k.call('kimo_describe',action=a)
def run(a,**args):
    d=desc(a); g=d['group']; st['call']+=1
    try: return k.call(g,action=a,args=args)
    except RuntimeError as e: st['errors']+=1; errs.append(str(e)[:200]); raise
t0=time.time()
print(find("set canvas size and fps")['matches'][0]['action'])
run('project_set',width=1280,height=720,fps=25)
print(find("import video file")['matches'][0]['action'])
m=run('import_media',path=SRC); mid=m.get('media_id') or m.get('id'); print('media',m)
tr=run('add_track')['track_id']
c=run('add_clip',track_id=tr,media_id=mid,start=0,duration=4,in_point=0) if False else None
tr=run('add_track')['track_id']
c1=run('add_clip',track_id=tr,source_type='media',asset_id=mid['asset_id'] if isinstance(mid,dict) else m['asset_id'],start=0,duration=4,in_point=0,fit='cover')['clip_id']
c2=run('add_clip',track_id=tr,source_type='media',asset_id=m['asset_id'],start=4,duration=4,in_point=4.5,fit='cover')['clip_id']
print(find("whip pan transition")['effects_and_transitions'][0])
run('add_transition',clip_id=c2,kind='whip_left',duration=0.5)
print(find("teal orange grade")['effects_and_transitions'][0])
for c in (c1,c2):
    run('add_effect',clip_id=c,kind='teal_orange'); run('add_effect',clip_id=c,kind='vignette'); run('add_effect',clip_id=c,kind='grain')
t2=run('add_track')['track_id']
print(find("add text title")['matches'][0]['action'])
run('add_text',track_id=t2,text='KIMO GROUPS TEST',start=0.5,duration=3,size=96,anchor='center',x=0.5)
t3=run('add_track')['track_id']
objs=[{"kind":"rbox","size":[260,260,260],"round":20,"pos":[0,0,900],"color":[0.9,0.35,0.2],"rough":0.4,"parent":0,"keys":[{"t":0,"pos":[-1400,0,900],"rot":[0,0,0]},{"t":3,"pos":[1400,0,900],"rot":[0,720,0]}]}]
print(find("3d scene motion blur shutter")['matches'][:3])
run('add_scene3d',track_id=t3,start=4.5,duration=3,config={"objects":objs,"ambient":[0.3,0.3,0.35],"shutter_deg":180,"blur_samples":8,"lights":[{"kind":"directional","dir":[0.5,0.8,-0.3],"color":[1,0.95,0.9],"intensity":1.6}]})
s=time.time(); run('export',path=sys.argv[2] if len(sys.argv)>2 else '/tmp/grouped_edit.mp4'); print('export s',round(time.time()-s,1))
print(st, 'errors:',errs, 'total s',round(time.time()-t0,1))
