import sys,time,json;sys.path.insert(0,'/home/sandbox/kimo/examples');from mcp_client import Kimo
k=Kimo();C=k.call;C('project_set',width=1280,height=720,fps=25)
a=C('import_media',path=(sys.argv[1] if len(sys.argv)>1 else '/downloads/city-49844-76b1f705.mp4'))['asset_id'];t=C('add_track')['track_id'];c=C('add_clip',track_id=t,source_type='media',asset_id=a,start=0,duration=4,fit='cover')['clip_id']
W,H=1280,720
def n(q): return [v/(W if i%2==0 else H) for i,v in enumerate(q)]
tq=n([411,353,770,303,790,464,424,557]);cq=n([430,399,744,340,749,477,442,579])
t0=time.time();r=C('validate_planar_track',clip_id=c,corners=tq,context_corners=cq,anchors=3,threshold_px=3);print('wall',round(time.time()-t0,1))
pf=r.pop('per_frame');print(json.dumps(r)[:900]);json.dump(pf,open('/tmp/vt_pf.json','w'))
print([ (x['frame'],(round(x['max_target_corner_px'],1) if x['tested'] else None)) for x in pf[::10]])
