"""Test fixture metadata only: demonstrates signalling, not mastering measurements."""
import sys
exec(open('examples/color_engine_proof.py').read().split("call('project_set'")[0])
call('project_set',width=320,height=180,fps=25);asset=call('import_media',path=sys.argv[1])['asset_id'];tr=call('add_track')['track_id'];a=call('add_clip',track_id=tr,source_type='media',asset_id=asset,start=0,duration=1,in_point=10)['clip_id'];call('set_color_pipeline',settings=dict(enabled=True,working_gamut=3,output_gamut=1,output_transfer=6,reference_white=203))
m=dict(red=[.68,.32],green=[.265,.69],blue=[.15,.06],white=[.3127,.329],min_nits=.005,max_nits=1000,max_cll=1000,max_fall=400);print(call('set_hdr_mastering',metadata=m));start=time.perf_counter();print(call('export_high_bit',format='hevc10',path='/downloads/hdr-metadata-proof.mp4'));print('metadata_export_seconds',time.perf_counter()-start)
for k,v in [('max_fall',1001),('max_nits',20000),('red',[1,1]),('min_nits',-1)]:bad=dict(m);bad[k]=v;assert rpc('set_hdr_mastering',metadata=bad).get('isError')
p.terminate()
