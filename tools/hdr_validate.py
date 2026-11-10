#!/usr/bin/env python3
"""Independent BT.2100 PQ fixture and bounded whole-clip decoded signal audit.
No engine imports. No metadata rewriting. Python stdlib + ffmpeg/ffprobe only.
"""
import argparse, array, json, math, pathlib, struct, subprocess, sys

def pq_encode(nits):
    p=(max(0,min(10000,nits))/10000)**(2610/16384)
    return ((3424/4096+2413/128*p)/(1+2392/128*p))**(2523/32)

def pq_decode(code):
    if not 0 <= code <= 1:
        return None
    p=code**(32/2523)
    return 10000*(max(p-3424/4096,0)/(2413/128-2392/128*p))**(16384/2610)

def reference_pixels():
    # 16-pixel blocks avoid chroma-edge ambiguity; black, diffuse, highlight,
    # nominal peak and deliberate beyond-PQ/negative clipping probes.
    ramp=[0,.005,.1,1,10,100,203,1000]
    for y in range(64):
        for x in range(128):
            if y<32:
                v=ramp[x//16]; yield (v,v,v)
            else:
                swatches=[(1000,0,0),(0,1000,0),(0,0,1000),(100,203,500),
                          (4000,4000,4000),(10000,10000,10000),(-1,-1,-1),(12000,12000,12000)]
                yield swatches[x//16]

def run(args, **kwargs):
    return subprocess.run(args,check=True,timeout=180,**kwargs)

def probe(path):
    return json.loads(run(['ffprobe','-v','error','-select_streams','v:0','-show_streams','-show_frames',
        '-read_intervals','%+#1','-of','json',str(path)],capture_output=True).stdout)

def audit(path, crop=None, expected=None, max_frames=500):
    info=probe(path); stream=info['streams'][0]
    if stream.get('color_primaries')!='bt2020' or stream.get('color_transfer')!='smpte2084':
        raise ValueError('audit requires explicit BT2020/PQ tags; refuses inference')
    w,h=stream['width'],stream['height']
    if w*h>4096*2160: raise ValueError('audit pixel budget exceeded')
    yuv=stream.get('pix_fmt')=='yuv420p10le'
    if yuv and (stream.get('color_space')!='bt2020nc' or stream.get('color_range')!='tv' or w%2 or h%2):
        raise ValueError('only declared narrow-range BT2020NC 10bit420 YUV supported')
    if not yuv and stream.get('pix_fmt') not in ('gbrp16le','gbrp16be'):
        raise ValueError('unsupported pixel format; no silent RGB conversion')
    if not yuv and stream.get('color_space')!='gbr': raise ValueError('RGB matrix tag required')
    x0,y0,cw,ch=crop or (0,0,w,h)
    if min(x0,y0)<0 or min(cw,ch)<=0 or x0+cw>w or y0+ch>h: raise ValueError('invalid active crop')
    n=w*h; size=(n*3 if yuv else n*6)
    if size*(max_frames+1)>1024**3:raise ValueError("decoded byte budget exceeds1GiB; lower --max-frames")
    cmd=['ffmpeg','-v','error','-i',str(path),'-map','0:v:0','-vsync','0','-frames:v',str(max_frames+1),
         '-f','rawvideo','-pix_fmt','yuv420p10le' if yuv else 'gbrp16le','-']
    # Tempfile prevents pipe deadlocks; cap decoded frames and wall clock.
    import tempfile
    errors=[]; neutral_errors=[]; interior_neutral_errors=[]; counts={'rgb_below_zero':0,'rgb_above_one':0,'yuv_outside_nominal':0,'reference_clipped_channels':0}
    maxcll=maxfall=0.; frames=0; boundary_hits=0
    with tempfile.TemporaryFile() as out:
        run(cmd,stdout=out,stderr=subprocess.PIPE);out.seek(0)
        while True:
            raw=out.read(size)
            if not raw: break
            if len(raw)!=size: raise ValueError('partial decoded frame')
            frames+=1
            if frames>max_frames: raise ValueError('whole clip exceeds frame budget; audit NOT complete')
            words=array.array('H');words.frombytes(raw)
            if sys.byteorder!='little':words.byteswap()
            total=0.
            if yuv: counts['yuv_outside_nominal']+=sum(not 64<=v<=940 for v in words[:n])+sum(not 64<=v<=960 for v in words[n:])
            for y in range(y0,y0+ch):
                for x in range(x0,x0+cw):
                    i=y*w+x
                    if yuv:
                        yy=(words[i]-64)/876; j=(y//2)*(w//2)+x//2
                        cb=(words[n+j]-512)/896;cr=(words[n+n//4+j]-512)/896
                        rgb=(yy+1.4746*cr,yy-(.0593*1.8814/.6780)*cb-(.2627*1.4746/.6780)*cr,yy+1.8814*cb)
                    else:rgb=(words[2*n+i]/65535,words[i]/65535,words[n+i]/65535)
                    counts['rgb_below_zero']+=sum(v<0 for v in rgb);counts['rgb_above_one']+=sum(v>1 for v in rgb)
                    boundary_hits+=sum(v<=0 or v>=1 for v in rgb)
                    # Track excursions separately. Saturate only for EOTF
                    # measurement, matching PQ reference display nominal domain.
                    linear=[pq_decode(max(0,min(1,v))) for v in rgb]
                    peak=max(linear);maxcll=max(maxcll,peak);total+=peak
                    if expected is not None:
                        for actual,ref in zip(linear,expected[i]):
                            if not 0<=ref<=10000:counts['reference_clipped_channels']+=1
                            err=abs(actual-max(0,min(10000,ref)));errors.append(err)
                            if y<32:neutral_errors.append(err)
                            if 8<=y<24 and 4<=x%16<12:interior_neutral_errors.append(err)
            maxfall=max(maxfall,total/(cw*ch))
    if frames==0:raise ValueError('empty decoded clip')
    side=info.get('frames',[{}])[0].get('side_data_list',[])
    metadata=next((v for v in side if v.get('side_data_type')=='Content light level metadata'),None)
    errors.sort()
    result={'frames':frames,'whole_clip':True,'stream_tags':{k:stream.get(k) for k in ('pix_fmt','color_range','color_space','color_transfer','color_primaries')},
        'active_area':[x0,y0,cw,ch],'max_cll_nits':maxcll,'max_fall_nits':maxfall,'declared_content_light':metadata,
        'declared_exceeded': None if not metadata else {k:(None if metadata.get(field,0)==0 else value>metadata[field]+.5) for k,field,value in [('max_cll','max_content',maxcll),('max_fall','max_average',maxfall)]},
        'first_frame_static_side_data':side,'counts':counts,'boundary_hits_channels':boundary_hits,
        'rules':'All decoded active pixels included, including black. No alpha in these formats. Negative/above1 RGB counted, clamped only for nominal PQ EOTF evaluation. MaxFALL=mean(maxRGB), not CIE Y. Whole-clip maxima; no metadata rewrite.',
        'interior_neutral_max_error_nits':max(interior_neutral_errors) if interior_neutral_errors else None,
        'neutral_ramp_max_error_nits':max(neutral_errors) if neutral_errors else None,
        'error_nits': None if not errors else {'max':errors[-1],'p50':errors[len(errors)//2],'p95':errors[int((len(errors)-1)*.95)],'p99':errors[int((len(errors)-1)*.99)]}}
    return result

def main():
    p=argparse.ArgumentParser();sub=p.add_subparsers(dest='mode',required=True)
    f=sub.add_parser('fixture');f.add_argument('directory',type=pathlib.Path)
    a=sub.add_parser('audit');a.add_argument('path',type=pathlib.Path);a.add_argument('--crop',nargs=4,type=int);a.add_argument('--max-frames',type=int,default=500)
    args=p.parse_args()
    if args.mode=='audit':
        if not 1<=args.max_frames<=10000:raise ValueError('frame budget1..10000')
        result=audit(args.path,args.crop,max_frames=args.max_frames)
    else:
        d=args.directory;d.mkdir(parents=True,exist_ok=True);pixels=list(reference_pixels())
        with (d/'oracle.rgba32f').open('wb') as out:
            for rgb in pixels:out.write(struct.pack('<4f',*(v/100 for v in rgb),1))
        run(['cargo','run','-q','-p','kimo-core','--example','hdr_fixture','--',str(d/'oracle.rgba32f'),str(d)],stderr=subprocess.PIPE)
        result={name:audit(d/name,expected=pixels) for name in ['pq-lossless.mkv','pq-hevc.mp4']}
        # Independent ideal code quantization evidence, distinct from codec loss.
        result['ideal_quantization']={str(levels):max(abs(pq_decode(round(pq_encode(v)*levels)/levels)-v) for rgb in pixels for v in rgb if 0<=v<=10000) for levels in [65535,876]}
    if args.mode=="fixture":
        lossless=result["pq-lossless.mkv"]
        result["lossless_fixture_passed"]=(lossless["frames"]==2 and lossless["error_nits"]["max"]<0.05)
        if not result["lossless_fixture_passed"]:raise AssertionError("lossless HDR fixture mismatch")
    print(json.dumps(result,indent=2))
if __name__=='__main__':main()
