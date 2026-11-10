"""Explicit real-media still parity proof. Requires a built kimo-mcp, ffmpeg and ffprobe.

Example: python3 examples/color_still_contract_proof.py source.mp4 --output-dir /tmp/proof
Optional --ocio needs installed pinned OpenColorIO2.5.1; never installs dependencies.
--compare checks encoded files and decoded samples against a previous manifest.
This script is not part of the fuzz suite. PQ samples are not an SDR preview.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import select
import subprocess
import sys
import time


def finite_nonnegative(value):
    number = float(value)
    if not math.isfinite(number) or number < 0:
        raise argparse.ArgumentTypeError('must be finite and nonnegative')
    return number


def options():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('--output-dir', required=True, type=Path)
    parser.add_argument('--in-point', type=finite_nonnegative, default=3.)
    parser.add_argument('--time', type=finite_nonnegative, default=.2)
    parser.add_argument('--ocio', action='store_true')
    parser.add_argument('--compare', type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    args.source = args.source.resolve()
    args.output_dir = args.output_dir.resolve()
    if not args.source.is_file():
        parser.error('source must be an existing file')
    if args.output_dir == repo or repo in args.output_dir.parents:
        parser.error('output-dir must be outside the repository')
    if args.output_dir.exists() and not args.output_dir.is_dir():
        parser.error('output-dir must be a directory')
    names = ('engine-stderr.log', 'manifest.json', 'waveform.png', 'parade.png',
             'vectorscope.png', 'histogram.png', 'highbit.png',
             'render_aces2_reference.png', 'render_gamut_excursions.png',
             'render_exposure_false_color.png')
    for name in names:
        target = args.output_dir / name
        if target.is_symlink() or (target.exists() and not target.is_file()):
            parser.error('output file must not be a symlink or directory: ' + name)
    if args.compare is not None and not args.compare.is_file():
        parser.error('compare must be an existing manifest file')
    return args, repo


class Engine:
    def __init__(self, binary, log):
        self.process = subprocess.Popen([str(binary)], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=log, text=True)
        self.sequence = 0

    def call(self, tool, **arguments):
        self.sequence += 1
        request = dict(jsonrpc='2.0', id=self.sequence, method='tools/call',
                       params=dict(name=tool, arguments=arguments))
        self.process.stdin.write(json.dumps(request) + '\n')
        self.process.stdin.flush()
        deadline = time.monotonic() + 120
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([self.process.stdout], [], [], remaining)[0]:
                raise TimeoutError(tool)
            line = self.process.stdout.readline()
            if not line:
                raise RuntimeError('engine exited during ' + tool)
            response = json.loads(line)
            if response.get('id') != self.sequence:
                continue
            if 'error' in response:
                raise RuntimeError(response['error'])
            result = response['result']
            if result.get('isError'):
                raise RuntimeError(result)
            return json.loads(result['content'][0]['text'])

    def close(self):
        self.process.terminate()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()


def inspect(path, pixel_format):
    info = json.loads(subprocess.check_output([
        'ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_entries',
        'stream=width,height,pix_fmt', '-of', 'json', str(path)], timeout=30))['streams'][0]
    raw = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(path),
                                   '-f', 'rawvideo', '-pix_fmt', pixel_format, '-'], timeout=30)
    expected = info['width'] * info['height'] * (8 if pixel_format == 'rgba64le' else 4)
    if len(raw) != expected:
        raise RuntimeError('decoded sample byte count mismatch: ' + str(path))
    return dict(file_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                decoded_sha256=hashlib.sha256(raw).hexdigest(), decoded_bytes=len(raw),
                width=info['width'], height=info['height'], file_pixel_format=info['pix_fmt'],
                decoded_pixel_format=pixel_format)


def compare(current, previous):
    fields = ('file_sha256', 'decoded_sha256', 'decoded_bytes', 'width', 'height',
              'file_pixel_format', 'decoded_pixel_format')
    if set(current['outputs']) != set(previous['outputs']):
        raise RuntimeError('compare mismatch: output names differ')
    for name, output in current['outputs'].items():
        for field in fields:
            if output[field] != previous['outputs'][name][field]:
                raise RuntimeError('compare mismatch: ' + name + '.' + field)


def main():
    args, repo = options()
    previous = json.loads(args.compare.read_text()) if args.compare else None
    args.output_dir.mkdir(parents=True, exist_ok=True)
    manifest = dict(format_version=1, source=str(args.source), in_point=args.in_point,
                    time=args.time, width=320, height=180, fps=25, ocio=args.ocio, outputs={},
                    note='Same-engine byte parity, not HDR/ACES certification; PQ is not an SDR preview.')
    with (args.output_dir / 'engine-stderr.log').open('w') as log:
        engine = Engine(repo / 'target/release/kimo-mcp', log)
        try:
            engine.call('project_set', width=320, height=180, fps=25)
            asset = engine.call('import_media', path=str(args.source))['asset_id']
            track = engine.call('add_track')['track_id']
            engine.call('add_clip', track_id=track, source_type='media', asset_id=asset,
                        start=0, in_point=args.in_point, duration=max(1., args.time + .1))
            def render(name, tool, pixel_format='rgba', **extra):
                path = args.output_dir / (name + '.png')
                start = time.perf_counter()
                metadata = engine.call(tool, time=args.time, path=str(path), **extra)
                elapsed = time.perf_counter() - start
                output = inspect(path, pixel_format)
                output.update(path=str(path), seconds=elapsed, metadata=metadata)
                manifest['outputs'][name] = output
            for kind in ('waveform', 'parade', 'vectorscope', 'histogram'):
                render(kind, 'render_scope', scope=kind)
            engine.call('set_color_pipeline', settings=dict(enabled=True, working_gamut=3,
                        input_transfer=8, output_gamut=4, output_transfer=6))
            render('highbit', 'render_high_bit_still', 'rgba64le')
            if args.ocio:
                engine.call('set_color_pipeline', settings=dict(enabled=True, working_gamut=3,
                            input_transfer=8))
                for tool in ('render_aces2_reference', 'render_gamut_excursions',
                             'render_exposure_false_color'):
                    render(tool, tool)
        finally:
            engine.close()
    destination = args.output_dir / 'manifest.json'
    destination.write_text(json.dumps(manifest, indent=2) + '\n')
    if previous is not None:
        compare(manifest, previous)
    print(destination)


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
