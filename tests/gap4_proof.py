"""Synthetic end-to-end MCP proof. Requires FFmpeg and Pillow."""
import argparse
import json
import subprocess
from pathlib import Path
from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument('--binary', default='target/debug/kimo-mcp')
parser.add_argument('--output', default='/tmp/gap4-proof')
args = parser.parse_args()
root = Path(args.output).resolve()
root.mkdir(parents=True, exist_ok=True)
log = []
p = subprocess.Popen([str(Path(args.binary).resolve())], stdin=subprocess.PIPE,
                     stdout=subprocess.PIPE, stderr=open(root / 'server.stderr', 'w'), text=True)


def call(name, **arguments):
    request = dict(jsonrpc='2.0', id=len(log) + 1, method='tools/call',
                   params=dict(name=name, arguments=arguments))
    p.stdin.write(json.dumps(request) + '\n')
    p.stdin.flush()
    result = json.loads(p.stdout.readline())['result']
    log.append(dict(name=name, arguments=arguments, result=result))
    assert not result.get('isError'), result
    return json.loads(result['content'][0]['text'])


try:
    call('project_set', width=640, height=360, fps=25)
    angles = []
    for color in ['red', 'blue']:
        path = str(root / f'{color}.mp4')
        subprocess.run(['ffmpeg', '-v', 'error', '-y', '-f', 'lavfi', '-i',
                        f'color={color}:s=640x360:r=25:d=2', '-f', 'lavfi', '-i',
                        'sine=frequency=440:duration=2', '-c:v', 'libx264', '-c:a',
                        'aac', '-shortest', path], check=True)
        track = call('add_track')['track_id']
        asset = call('import_media', path=path)['asset_id']
        angles.append(call('add_clip', track_id=track, source_type='media',
                           asset_id=asset, start=0, duration=2)['clip_id'])
    mc = call('add_multicam', angles=angles)['multicam_id']
    call('switch_angle', multicam_id=mc, t=1, angle=1)
    for label, time in [('angle0', .5), ('angle1', 1.5)]:
        call('render_frame', time=time, path=str(root / f'{label}.png'))
    track = call('add_track')['track_id']
    srt = root / 'captions.srt'
    srt.write_text('1\n00:00:00,000 --> 00:00:02,000\nEditable Kimo captions\n')
    cg = call('add_captions', track_id=track, source='srt', path=str(srt),
              style='boxed')['caption_id']
    call('render_frame', time=.5, path=str(root / 'caption-on.png'))
    call('burn_in', caption_id=cg, enabled=False)
    call('render_frame', time=.5, path=str(root / 'caption-off.png'))
    call('burn_in', caption_id=cg, enabled=True)
    call('export_audio', path=str(root / 'multicam.wav'))
    call('save_project', path=str(root / 'project.json'))
finally:
    p.stdin.close()
    p.wait(timeout=30)
    (root / 'mcp-proof.json').write_text(json.dumps(log, indent=2))

on, off = [Image.open(root / f'caption-{state}.png') for state in ['on', 'off']]
assert on.tobytes() != off.tobytes()
assert Image.open(root / 'angle0.png').getpixel((50, 50))[0] > 200
assert Image.open(root / 'angle1.png').getpixel((50, 50))[2] > 200
sheet = Image.new('RGB', (1280, 720))
for label, pos in [('angle0', (0, 0)), ('angle1', (640, 0)),
                   ('caption-on', (0, 360)), ('caption-off', (640, 360))]:
    sheet.paste(Image.open(root / f'{label}.png'), pos)
sheet.save(root / 'contact-sheet.png')
print(f'MCP proof passed: {len(log)} calls, red/blue switch, caption on/off, audio export')
