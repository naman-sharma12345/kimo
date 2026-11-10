exec(open('examples/color_engine_proof.py').read().split("call('project_set'")[0])
call('project_set',width=1280,height=720,fps=25);asset=call('import_media',path='/downloads/video-c61877f1.mp4')['asset_id']
for i in range(4):
 tr=call('add_track')['track_id'];call('add_clip',track_id=tr,source_type='media',asset_id=asset,start=0,in_point=3,duration=2)
start=time.perf_counter();r=rpc('render_frame',time=.5,path='/downloads/cache4-720.png');print('four_layers_seconds',time.perf_counter()-start,r);p.terminate()
