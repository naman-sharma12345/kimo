#!/usr/bin/env python3
"""Docs/provenance integrity only. Does not test Kimo or third-party algorithms."""
from pathlib import Path
import json, hashlib,re
root=Path(__file__).parent
rows=json.loads((root/'survey.json').read_text())
ins=json.loads((root/'inspected-sources.json').read_text())
assert len(rows)==74
assert len({x['repo'].lower() for x in rows})==74
pinned=[x for x in rows if x.get('pin')]
assert len(pinned)==68
assert len(ins)==21
assert len({x['repo'].lower() for x in ins})==16
for r in pinned:
 assert re.fullmatch('[0-9a-f]{40}',r['pin'])
 if r.get('license_sha256'):
  p=root/'license-evidence'/(r['repo'].replace('/','__')+'.txt')
  assert hashlib.sha256(p.read_bytes()).hexdigest()==r['license_evidence_sha256'],r['repo']
for x in ins:
 assert x['url_verified'] and x['source_url']
 assert x['pin'] in x['source_url']
 assert re.fullmatch('[0-9a-f]{64}',x['sha256'])
 assert x['inspected_spans']
for f in ['README.md','GAP_AUDIT.md','PORT_PLAN.md','REPOSITORY_SURVEY.md','SOURCE_EVIDENCE.md']:
 assert (root/f).stat().st_size>200
for f in root.glob('*.md'):
 for target in re.findall(r'\]\(([^)]+)\)',f.read_text()):
  if not target.startswith(('http:','https:','#')):assert (f.parent/target.split('#')[0]).exists(),(f,target)
assert sum(1 for _ in (root/'license-evidence').glob('*.txt'))==62
print('PASS docs/provenance integrity: 74 paths, 68 pins, 62 license snapshots, 21 inspected sources / 16 repos, local links')
