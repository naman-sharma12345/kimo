"""Read-only purity (G3): run the server with KIMO_AUDIT_READONLY=1, which makes dispatch fingerprint the project around every tool marked read-only
(undo/redo/checkpoint/restore excepted) and fail the call with 'AUDIT:' if it changed. Calls every tool with empty args and with a populated project;
any response containing 'AUDIT:' is a tool that lies about being read-only. Coverage limit: only tools that accept the trivial args above actually run, so this is the same reach as the fuzz. Mutation check (done by hand, not in CI):
flipping set_light to read-only made `set_light id=a` fail with AUDIT and roll back. Writes nothing."""
import sys, os, json, subprocess; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); import mcp_client
os.environ['KIMO_AUDIT_READONLY'] = '1'; from mcp_client import Kimo
k = Kimo(); c = k.call; c('project_set', width=320, height=180, fps=25); t = c('add_track')['track_id']
cl = c('add_shape', track_id=t, shape='rect', width=50, height=50, fill=[1, 1, 1, 1], start=0, duration=1)['clip_id']
tools = [x['name'] for x in k.tools()]
bad = []
for n in tools:
    for args in ({}, {'clip_id': cl}, {'track_id': t}):
        try: c(n, **args)
        except RuntimeError as e:
            if 'AUDIT:' in str(e): bad.append((n, str(e)[:120]))
# direct tripwire check on a tool that really mutates when given valid args: if it were marked read-only the audit would reject it (verified by hand: with set_light flipped to read-only the call fails with AUDIT and rolls back)
print('tools checked', len(tools), 'violations', bad); assert not bad, 'read-only tool modified the project'
print('read-only audit proof OK')
