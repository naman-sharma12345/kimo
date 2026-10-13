"""RV-24 proof: the `kimo` CLI runs the same core dispatch as kimo-mcp.
1 exit-code table, 2 CLI vs MCP byte-identical (project file + rendered frame + export sample), 3 atomic save (no .tmp), 4 no stdin needed,
5 progress only on stderr / --json machine output, 6 kimo info identity, 7 tools search, 8 pass-through stubs. /tmp only."""
import sys, os, json, glob, shutil, subprocess, hashlib
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); from mcp_client import Kimo
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
CLI = os.environ.get('KIMO_CLI') or (os.path.join(os.path.dirname(os.path.abspath(os.environ['KIMO_BIN'])), 'kimo') if os.environ.get('KIMO_BIN') else os.path.join(ROOT, 'target/release/kimo'))   # the CLI sits next to KIMO_BIN
D = '/tmp/cli_proof'; shutil.rmtree(D, ignore_errors=True); os.makedirs(D)
def sh(*a, stdin=subprocess.DEVNULL, env=None):
    e = dict(os.environ); e.update(env or {}); r = subprocess.run([CLI, *a], stdin=stdin, capture_output=True, text=True, env=e, timeout=300); return r
def H(p): return hashlib.sha256(open(p, 'rb').read()).hexdigest()
def ok(r, what):
    assert r.returncode == 0, f'{what}: rc={r.returncode} stderr={r.stderr}'; return json.loads(r.stdout)

# 1 exit-code table
table = []
def expect(code, what, *a, **kw):
    r = sh(*a, **kw); table.append((what, code, r.returncode)); assert r.returncode == code, f'{what}: want {code} got {r.returncode}: {r.stderr}'; return r
expect(0, 'info', 'info'); expect(0, 'tools list', 'tools'); expect(0, 'tools search', 'tools', 'search', 'blur')
expect(0, 'call ok', 'call', 'kimo_info', '--json', '{}')
r = expect(1, 'call unknown tool', '--json', 'call', 'no_such_tool'); e = json.loads(r.stderr.strip().splitlines()[-1])['error']; assert e['code'] == 'unknown_tool' and e['message'], e
r = expect(1, 'call bad arg', '--json', 'call', 'add_shape', '--args', '{"shape":"rect"}'); assert json.loads(r.stderr.strip().splitlines()[-1])['error']['code'] == 'missing_arg'
expect(2, 'no command'); expect(2, 'unknown command', 'frobnicate'); expect(2, 'call without tool', 'call'); expect(2, 'bad json', 'call', 'kimo_info', '--args', '{nope'); expect(0, 'json alias for args', 'call', 'kimo_info', '--json', '{}')
expect(2, 'export missing args', 'export'); expect(2, 'bad range', 'export', 'a.kimo', 'b.mp4', '--range', 'x')
expect(1, 'export missing project', 'export', D + '/none.kimo', D + '/o.mp4')
expect(0, 'version', '--version'); expect(0, 'version -V', '-V'); expect(2, 'tools search without query', 'tools', 'search')
# 8 pass-through: preflight / validate / render-job (exit 3 when preflight reports failure; unavailable tool = structured exit 1)
names = set(ok(sh('tools'), 'tools')['tools'])
for cmd, tool in (('preflight', 'preflight'), ('validate', 'validate_export'), ('render-job', 'render_job_status')):
    if tool in names: print('tool present, covered by its own proof:', tool); continue
    a = [cmd] + (['status'] if cmd == 'render-job' else []); r = expect(1, f'{cmd} unavailable', '--json', *a); assert json.loads(r.stderr.strip().splitlines()[-1])['error']['code'] == 'tool_unavailable'
print('exit-code table:'); [print(f'  {w:28s} want {c} got {g}') for w, c, g in table]

# 2 CLI vs MCP byte-identical
P_CLI, P_MCP = D + '/cli.kimo', D + '/mcp.kimo'
seq = [('project_set', dict(width=320, height=180, fps=25)), ('add_track', {}),
       ('add_shape', dict(track_id='TRACK', shape='rect', width=120, height=80, fill=[1, 0.2, 0.1, 1], start=0, duration=2)),
       ('add_shape', dict(track_id='TRACK', shape='ellipse', width=90, height=90, fill=[0.1, 0.4, 1, 1], stroke_width=4, stroke_color=[1, 1, 1, 1], start=0.5, duration=1.5))]
# same call sequence as the CLI performs: load the project file (when it exists), call, save. (load_project moves the id counter past loaded ids, so
# a fresh-process CLI call and an MCP call that loads first agree; that is core behaviour, not CLI behaviour.)
k = Kimo(); track = None
for name, a in seq:
    a = {x: (track if y == 'TRACK' else y) for x, y in a.items()}
    if os.path.exists(P_MCP): k.call('load_project', path=P_MCP)
    r = k.call(name, **a); track = r.get('track_id', track) if isinstance(r, dict) else track; k.call('save_project', path=P_MCP)
k.call('load_project', path=P_MCP); k.call('render_frame', path=D + '/mcp.png', time=1.0); k.call('export', path=D + '/mcp.mp4', start=0, end=1)
track = None
for name, a in seq:
    a = {x: (track if y == 'TRACK' else y) for x, y in a.items()}
    r = ok(sh('--json', '--quiet', 'call', name, '--args', json.dumps(a), '--project', P_CLI), name); track = r.get('track_id', track)
r = ok(sh('--json', 'call', 'render_frame', '--args', json.dumps({'path': D + '/cli.png', 'time': 1.0}), '--project', P_CLI), 'render_frame')
r = ok(sh('--json', 'export', P_CLI, D + '/cli.mp4', '--range', '0:1'), 'export'); print('cli export:', r)
assert H(P_CLI) == H(P_MCP), 'project bytes differ CLI vs MCP'; print('project sha256 identical', H(P_CLI)[:16])
# 2b in-memory parity (F1): a long-running MCP session that NEVER reloads between calls gives the same ids and project bytes as the CLI (which loads each time)
P_MEM = D + '/mem.kimo'; km = Kimo(); track = None
for name, a in seq:
    a = {x: (track if y == 'TRACK' else y) for x, y in a.items()}
    r = km.call(name, **a); track = r.get('track_id', track) if isinstance(r, dict) else track
km.call('save_project', path=P_MEM); km.close()
assert H(P_CLI) == H(P_MEM), 'in-memory MCP session differs from CLI (id allocation after load?)'; print('in-memory MCP session == CLI project bytes', H(P_MEM)[:16])
assert H(D + '/cli.png') == H(D + '/mcp.png'), 'frame bytes differ'; print('frame sha256 identical', H(D + '/cli.png')[:16])
# export: compare decoded frames (container metadata may carry the file name), via ffmpeg framemd5
def md5(p): return subprocess.run(['ffmpeg', '-v', 'error', '-i', p, '-map', '0:v', '-f', 'framemd5', '-'], capture_output=True, text=True).stdout
a, b = md5(D + '/cli.mp4'), md5(D + '/mcp.mp4'); assert a and a == b, 'exported frames differ'; print('export frame md5s identical')
# 3 atomic save: no tmp left
assert not glob.glob(D + '/*.tmp'), 'tmp left behind'
# 5 progress on stderr only, stdout is one JSON document
r = sh('call', 'kimo_info', '--project', P_CLI, '--no-save'); assert r.stderr.startswith('kimo:'), r.stderr; json.loads(r.stdout)
r = sh('--quiet', '--json', 'call', 'kimo_info'); assert r.stderr == '' and json.loads(r.stdout), r.stderr
# determinism: same call twice -> same stdout
assert sh('--json', '--quiet', 'call', 'list_effects').stdout == sh('--json', '--quiet', 'call', 'list_effects').stdout
# 6 info shows RV-2 identity (equal to kimo_info of the server)
ci = ok(sh('--json', 'info'), 'info'); mi = k.call('kimo_info'); assert ci == mi, (ci, mi); assert ci.get('version') and ci.get('commit') is not None; print('info:', {x: ci[x] for x in ('version', 'commit') if x in ci})
# 4 works with stdin closed and with args on stdin
ok(subprocess.run([CLI, '--json', 'info'], stdin=subprocess.DEVNULL, capture_output=True, text=True), 'closed stdin')
r = sh('--json', 'call', 'kimo_info', '--json', '-', stdin=subprocess.PIPE) if False else subprocess.run([CLI, '--json', 'call', 'kimo_info', '--args', '-'], input='{}', capture_output=True, text=True); ok(r, 'json from stdin')
# env: KIMO_MEM_MB honored (a tiny budget must still run info)
ok(sh('--json', 'info', env={'KIMO_MEM_MB': '512'}), 'KIMO_MEM_MB')
# 7 tools search
s = ok(sh('--json', 'tools', 'search', 'blur'), 'search'); assert s['matches'], s
# --version = semver + commit of kimo_info
v = ok(sh('--json', '--version'), 'version'); assert v == {'version': ci['version'], 'commit': ci['commit']}, v
# stateless call without --project; created note + atomic save for a new project; no save for non-mutating call
r = sh('call', 'add_track'); assert r.returncode == 0 and 'created' not in r.stderr and not glob.glob('*.kimo')
NP = D + '/new.kimo'; r = sh('call', 'add_track', '--project', NP); assert r.returncode == 0 and f'kimo: created {NP}' in r.stderr and os.path.exists(NP), r.stderr
m = os.path.getmtime(NP); r = sh('call', 'kimo_info', '--project', NP); assert os.path.getmtime(NP) == m and 'created' not in r.stderr
# group call with action=add_shape must save (mutates resolved through the group)
g = ok(sh('--json', 'tools', 'describe', 'add_shape'), 'describe')['call']['tool']; GP = D + '/group.kimo'
tid = ok(sh('--json', '--quiet', 'call', 'add_track', '--project', GP), 'add_track')['track_id']
ok(sh('--json', '--quiet', 'call', g, '--args', json.dumps({'action': 'add_shape', 'args': {'track_id': tid, 'shape': 'rect', 'start': 0, 'duration': 1, 'width': 10, 'height': 10}}), '--project', GP), 'group call')
assert '"rect"' in open(GP).read(), 'group call with action=add_shape did not save'; print('group call saved, group =', g)
# CLI and MCP read the same env limits and give the same timeout message
env = {'KIMO_EXPORT_TIMEOUT_S': '1', 'KIMO_TIMEOUT_S': '1'}; os.environ.update(env); T = D + '/slow.kimo'
def build(c):
    c('project_set', width=1280, height=720, fps=30); t = c('add_track')['track_id']
    for i in range(6): c('add_shape', track_id=t, shape='ellipse', width=600, height=600, fill=[1, 0, 0, 1], start=0, duration=3000)
kt = Kimo(); build(kt.call); kt.call('save_project', path=T)
r = sh('--json', '--quiet', 'export', T, D + '/slow.mp4', env=env); assert r.returncode == 1, (r.returncode, r.stderr); ce = json.loads(r.stderr.strip().splitlines()[-1])['error']
try: kt.call('export', path=D + '/slow2.mp4'); raise AssertionError('MCP export did not time out')
except RuntimeError as e: me = e.text
assert ce['message'] == me and ce['code'] == 'timeout' and 'exceeded the 1s time limit' in me, (ce, me); print('timeout message identical:', me[:70])
# exit 124 (uncancellable hang) is documented; no test-only sleeping tool exists, so it is document-only
u = sh('help'); assert '124' in u.stderr and u.returncode == 2
print('CLI PROOF OK')
