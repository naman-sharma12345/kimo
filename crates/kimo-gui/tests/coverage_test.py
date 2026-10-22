#!/usr/bin/env python3
"""Tool coverage: does every kimo-mcp tool have a dedicated visual?

Reads the engine's own tool list (tests/tools_flat.json: names from
`tools/list` with mode "flat", regenerate with --refresh /path/to/kimo-mcp),
runs each name through ui/router.js `family()` inside the real page, and
reports how many land on a dedicated family visual (timeline edit, fx
badge, transform/curve, grade wipe + scopes, mixer, captions, 3D, export)
versus the generic fallback (rail entry + inspector diff + HUD pulse:
visible, but not specific). Prints x/N and lists every fallback tool.

    python3 crates/kimo-gui/tests/coverage_test.py            # report
    python3 crates/kimo-gui/tests/coverage_test.py --strict   # exit 1 if any fallback
    python3 crates/kimo-gui/tests/coverage_test.py --refresh <kimo-mcp binary>
"""
import asyncio, functools, http.server, json, socketserver, subprocess, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
LIST = Path(__file__).with_name('tools_flat.json')

def refresh(binary):
    p = subprocess.Popen([binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    def rpc(i, method, params):
        p.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': i, 'method': method, 'params': params}) + '\n'); p.stdin.flush()
        return json.loads(p.stdout.readline())
    rpc(0, 'initialize', {})
    tools = rpc(1, 'tools/list', {'mode': 'flat'})['result']['tools']
    p.stdin.close(); p.wait(10)
    names = sorted({t['name'] for t in tools})
    LIST.write_text(json.dumps(names, indent=0) + '\n')
    print(f'wrote {len(names)} tool names to {LIST.name}')

class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass

async def main(strict):
    from playwright.async_api import async_playwright
    names = json.loads(LIST.read_text())
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Quiet, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    async with async_playwright() as p:
        b = await p.chromium.launch(); pg = await b.new_page()
        await pg.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off')
        res = await pg.evaluate("""async names => { const r = await import('./router.js'), m = await import('./modes.js');
            return names.map(n => [n, r.family(n), m.inferMode(n)]); }""", names)
        await b.close()
    srv.shutdown()
    # families whose router code changes a panel beyond the rail / inspector diff / HUD that every call gets
    PANEL = {'roto': 'mask path / box on the viewer, Mask keys lanes (path / property keys, propagation run, track matte), Refine matte / Key / Quality rows', 'ingest': 'timeline insert', 'edit': 'timeline split/trim/ripple; duplicate/swap/loop/distribute/group clips, speed/ramp/blend/echo/freeze badges, shake layer, track rename, project size', 'effects': 'fx badge + wipe', 'text': 'caption chip',
             'audio': 'EQ / comp / pan / meters / M-S', 'color': 'grade wipe + false colour', 'export': 'job card: progress bar, settings, renders row',
             'transform': 'viewer gizmo: box + 8 handles + anchor, previous pose, move arrow, readout; keyframe diamonds on the clip; motion path, bezier ease, null, parent link',
             '3d': 'camera orbit ring + frustum + pan keys + axis triad, scene objects in the tree, track quad / ROI / mesh grid / point ring / matte seed on the viewer, light glyph + Lights row, mesh op on the Stack, physics bodies, solve RMS',
             'graphics': 'viewer: detection box + label, HUD brackets, box expand, counter, prompt box, shape, path, confetti; Shape layers list + Layer style rows',
             'pipeline': 'Color pipeline rows (m05): input / working / view / output from -> to, clip input override row, HDR MaxCLL / MaxFALL / Peak',
             'project': 'm02 History rows (checkpoint, restore, undo / redo, save, load), Diff lines (project_diff, templates), Batch table (batch, recipe, template_apply), Media list + footer (load / save); set_meta renames the clip / track + Inspector Name / Role / Tags',
             'qa': 'HUD status toast + the call\'s own rail row carry the result: diff_frames A vs B changed % (+ quality PSNR / SSIM / MS-SSIM row), compare_ranges A/B PSNR / SSIM / MS-SSIM mean / min / p5, lint_frame findings, flash_series transition counts + first-crossing stamps, audio_spectrum peak / loudest band / total, frame cache budget / freed (no mode hop, canvas untouched)',
             'motion': 'm03 Graph: the key lists before/after (moved / removed key squares, smoothed / simplified / baked / preset / pasted curves, text-animator keys, motion-extract curves, motion-diff RMS lanes + worst frame) + Inspector rows; removed motion path dashed on the viewer gizmo',
             'measure': 'solved layout boxes / text line + ink boxes on the viewer, raycast hit on the m08 Stack, compiled draws in the m13 Scene tree, subject-match regions in m15; Inspector rows of the result',
             'inspect': 'read-only: quiet HUD read state + result pill + peek ring on the target (no mutation, no mode hop)'}
    by = {}
    for n, fam, mode in res: by.setdefault(fam, []).append((n, mode))
    dedicated = [n for f in PANEL for n, _ in by.get(f, [])]
    routed = [(n, f, m) for f, xs in by.items() if f not in PANEL and f != 'other' for n, m in xs]
    fallback = sorted(n for n, _ in by.get('other', []))
    N = len(names)
    print(f'tool coverage: {len(dedicated)}/{N} dedicated panel visual · {len(routed)}/{N} family-routed (own screen + rail/inspector/HUD, no panel effect yet) · {len(fallback)}/{N} generic fallback')
    for f in sorted(by, key=lambda f: -len(by[f])):
        if f != 'other': print(f"  {f:<10} {len(by[f]):>3}  {PANEL.get(f, 'routed only: rail + inspector diff + HUD (+ mode screen)')}")
    print('routed-only tools:', ', '.join(sorted(n for n, _, _ in routed)))
    print('fallback tools:', ', '.join(fallback) if fallback else 'none')
    sys.exit(1 if strict and (fallback or routed) else 0)

if __name__ == '__main__':
    if '--refresh' in sys.argv: refresh(sys.argv[sys.argv.index('--refresh') + 1])
    else: asyncio.run(main('--strict' in sys.argv))
