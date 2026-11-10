"""Color contract smoke over real footage. Optional OCIO requires explicit --ocio and installed2.5.1."""
import argparse,json,time,pathlib
from mcp_client import Kimo
ap=argparse.ArgumentParser();ap.add_argument('source');ap.add_argument('--ocio',action='store_true');ap.add_argument('--out',default='/downloads/color-contract-smoke');a=ap.parse_args();out=pathlib.Path(a.out);out.mkdir(parents=True,exist_ok=True)
k=Kimo();rows=[]
def verify_delivery(result,primaries,transfer):
 video=[s for s in result['streams'] if s.get('codec_type')=='video'];assert len(video)==1
 v=video[0];assert (v['width'],v['height'],v['pix_fmt'],v['bits_per_raw_sample'])==(320,180,'gbrp16le','16')
 assert (v['color_primaries'],v['color_transfer'],v['color_space'],v['color_range'])==(primaries,transfer,'gbr','pc')
 assert any(s.get('codec_type')=='audio' and s.get('codec_name')=='pcm_s16le' for s in result['streams'])
 assert all(not gap['absent_or_unspecified_color_fields'] for gap in result['tag_gaps'])
def record(name,**args):
 start=time.perf_counter();r=k.call(name,**args);rows.append(dict(tool=name,seconds=time.perf_counter()-start,result=r));return r
try:
 record('project_set',width=320,height=180,fps=25);asset=record('import_media',path=str(pathlib.Path(a.source).resolve()))['asset_id'];tr=record('add_track')['track_id'];c=record('add_clip',track_id=tr,source_type='media',asset_id=asset,start=0,in_point=3,duration=.4)['clip_id']
 assert record('inspect_color_pipeline')['render_domain']=='legacy_display_referred';record('render_frame',time=.2,path=str(out/'legacy.png'))
 record('set_color_pipeline',settings=dict(enabled=True,linear_resample=True,working_gamut=3,input_transfer=8));assert record('inspect_color_pipeline')['render_domain']=='working_scene_linear';record('render_float_exr',time=.2,path=str(out/'working.exr'));record('inspect_float_media',path=str(out/'working.exr'));record('analyze_color',time=.2);record('audit_color_output',time=.2)
 record('export_high_bit',path=str(out/'sdr-rgb16.mkv'),format='ffv1_16');verify_delivery(record('inspect_color_delivery',path=str(out/'sdr-rgb16.mkv')),'bt709','iec61966-2-1')
 if a.ocio:record('compare_aces2_reference',time=.2);record('render_aces2_reference',time=.2,path=str(out/'reference.png'))
 for transfer,tag in [(6,'pq'),(7,'hlg')]:
  record('set_color_pipeline',settings=dict(enabled=True,working_gamut=3,input_transfer=8,output_gamut=1,output_transfer=transfer,reference_white=203));record('export_high_bit',path=str(out/f'{tag}-rgb16.mkv'),format='ffv1_16');verify_delivery(record('inspect_color_delivery',path=str(out/f'{tag}-rgb16.mkv')),'bt2020','smpte2084' if transfer==6 else 'arib-std-b67')
finally:k.p.terminate();k.p.wait();(out/'evidence.json').write_text(json.dumps(rows,indent=2))
print(json.dumps([dict(tool=r['tool'],seconds=round(r['seconds'],4)) for r in rows],indent=2))
