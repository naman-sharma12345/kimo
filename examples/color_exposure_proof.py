exec(open('examples/color_engine_proof.py').read().split("call('project_set'")[0])
call('project_set',width=640,height=360,fps=25);a=call('import_media',path='/downloads/video-c61877f1.mp4')['asset_id'];tr=call('add_track')['track_id'];call('add_clip',track_id=tr,source_type='media',asset_id=a,start=0,in_point=3,duration=2);call('set_color_pipeline',settings=dict(enabled=True,working_gamut=3,input_transfer=8))
start=time.perf_counter();v=call('analyze_exposure',time=.5);print('analyze_seconds',time.perf_counter()-start);print(v)
start=time.perf_counter();call('render_exposure_false_color',time=.5,path='/downloads/exposure-real.png');print('falsecolor_seconds',time.perf_counter()-start);p.terminate()
