#!/usr/bin/env python3
"""Original dependency-free contract validator and review-only primitive compiler."""
import argparse
import json
import math
import os
from pathlib import Path

HERE = Path(__file__).resolve().parent


def check_schema(value, spec, path, errors):
    kinds = {'object': dict, 'array': list, 'string': str, 'boolean': bool,
             'number': (int, float), 'integer': int}
    kind = spec.get('type')
    if kind and (not isinstance(value, kinds[kind]) or
                 (kind in ('number', 'integer') and isinstance(value, bool))):
        errors.append(f'{path}: expected {kind}')
        return
    if kind in ('number', 'integer'):
        if isinstance(value, float) and not math.isfinite(value):
            errors.append(f'{path}: number must be finite')
            return
        for key, bad in [('minimum', lambda a,b:a<b), ('maximum', lambda a,b:a>b),
                         ('exclusiveMinimum', lambda a,b:a<=b)]:
            if key in spec and bad(value, spec[key]):
                errors.append(f'{path}: violates {key} {spec[key]}')
    if 'enum' in spec and value not in spec['enum']:
        errors.append(f'{path}: value outside enum')
    if kind == 'string' and len(value.strip()) < spec.get('minLength', 0):
        errors.append(f'{path}: non-empty text required')
    if kind == 'object':
        for key in spec.get('required', []):
            if key not in value: errors.append(f'{path}.{key}: required')
        for key, item in value.items():
            if key in spec['properties']:
                check_schema(item, spec['properties'][key], f'{path}.{key}', errors)
            elif spec.get('additionalProperties') is False:
                errors.append(f'{path}.{key}: unknown field')
    if kind == 'array':
        if len(value) < spec.get('minItems', 0): errors.append(f'{path}: empty array')
        for i, item in enumerate(value):
            check_schema(item, spec['items'], f'{path}[{i}]', errors)


def validate(data):
    errors, warnings = [], []
    check_schema(data, json.loads((HERE/'contract.schema.json').read_text()), '$', errors)
    if errors: return {'valid': False, 'errors': errors, 'warnings': warnings}
    version2 = data['schema_version'] in ('kimo-director-2','kimo-director-3')
    if version2:
        for i, scene in enumerate(data['scenes']):
            if 'progression' not in scene: errors.append(f'scene {i}: v2 progression required')
            for j, shot in enumerate(scene['shots']):
                for key in ('lighting_contract','optics_contract','movement_contract','density','attention_target'):
                    if key not in shot: errors.append(f'scene {i} shot {j}: v2 {key} required')
    if errors: return {'valid': False, 'errors': errors, 'warnings': warnings}
    cue_map={}
    for cue in data.get('music_cues', []):
        if cue['id'] in cue_map: errors.append('music_cues: duplicate id')
        cue_map[cue['id']]=cue
        if cue['time'] > data['project']['duration']: errors.append('music_cues: time beyond project')
        if cue['basis'] == 'estimated': warnings.append(f"cue {cue['id']}: estimated audio event; inspect master")
    for record in data.get('review_records', []):
        if record['scene_id'] >= len(data['scenes']): errors.append('review_records: unknown scene_id')
        if record['status'] == 'selected' and (not record['evidence'] or record['blockers']):
            errors.append('review_records: selected requires evidence pointers and no blockers')
    if data.get('review_records'):
        warnings.append('Review evidence pointers are unverified by this validator; inspect actual media')
    fps = data['project']['fps']
    # Less than a frame: floating serialization noise is accepted, real gaps are not.
    tolerance = min(1e-6, 0.01/fps)
    cursor, previous, outputs = 0., None, set()
    distinct_openings, opening_run, longest_run = set(), 0, 0
    audio = data['project']['audio_master']
    def same(a,b): return abs(a-b) <= tolerance
    for i, scene in enumerate(data['scenes']):
        label = f'scene {i}'
        if scene['id'] != i: errors.append(f'{label}: ids must be sequential from zero')
        if not same(scene['start'], cursor): errors.append(f'{label}: timeline gap/overlap')
        duration = scene['end']-scene['start']
        if duration <= 0: errors.append(f'{label}: end must follow start')
        if not 10 <= duration <= 15 and not scene.get('duration_exception'):
            errors.append(f'{label}: outside 10-15s; duration_exception required')
        cursor = scene['end']
        out = os.path.normpath(scene['output_path'])
        if out in outputs or out == os.path.normpath(audio) or out == os.path.normpath(data['assembly']['output_path']):
            errors.append(f'{label}: output path collision')
        outputs.add(out)
        cap = scene['generation_capability']
        if cap != 'unknown' and not scene.get('capability_source'):
            errors.append(f'{label}: verified capability requires source')
        if len(scene['shots']) > 1 and cap != 'multi_shot_verified':
            errors.append(f'{label}: multiple generation shots need verified multi-shot support')
        if cap == 'unknown': warnings.append(f'{label}: generation capability unknown; no provider execution authorized')
        reference_ids = [ref['id'] for ref in scene.get('references', [])]
        if len(reference_ids) != len(set(reference_ids)):
            errors.append(f'{label}: duplicate reference id')
        if version2:
            progression = scene['progression']
            if progression['mode'] == 'narrative' and progression['cause'] == progression['consequence']:
                warnings.append(f'{label}: narrative returns unchanged; check deliberate circular beat')
            if progression['emotion_in'] == progression['emotion_out']:
                warnings.append(f'{label}: emotion held constant; check action or perceptual change')
            if all(sh['density'] == 'high' for sh in scene['shots']) and not scene.get('pacing_reason'):
                warnings.append(f'{label}: constant high density; inspect rhythm or record pacing_reason')
        first = scene['shots'][0]
        signature = (first['framing'], first['movement'])
        distinct_openings.add(signature)
        opening_run = opening_run + 1 if previous and previous[0] == signature else 1
        longest_run = max(longest_run, opening_run)
        if previous and signature == previous[0] and not scene.get('repetition_reason'):
            warnings.append(f'{label}: repeated opening; revise or record repetition_reason')
        if previous and first['start_state'] != previous[1] and not scene.get('continuity_break_reason'):
            warnings.append(f'{label}: boundary states differ; review or explain discontinuity')
        previous = (signature, scene['shots'][-1]['end_state'])
        local = 0.
        for j, shot in enumerate(scene['shots']):
            if not same(shot['start'], local) or shot['end'] <= shot['start']:
                errors.append(f'{label} shot {j}: gap, overlap or nonpositive duration')
            local = shot['end']
            if shot.get('minimum_readable_seconds', 0) > shot['end']-shot['start']:
                warnings.append(f'{label} shot {j}: interval shorter than declared readability budget')
            binding=shot.get('cue_binding')
            if binding:
                cue=cue_map.get(binding['cue_id'])
                if not cue: errors.append(f'{label} shot {j}: unknown cue id')
                elif abs(scene['start']+shot['start']-cue['time']-binding['offset_seconds']) > 0.5/fps:
                    errors.append(f'{label} shot {j}: cue binding differs by more than half a frame')
            if scene['performance_mode'] == 'direct_lipsync':
                if (not shot['has_vocals'] or shot['singer_count'] != 1 or
                    not shot['mouth_visible'] or shot['orientation'] not in ('front','three_quarter') or
                    shot['framing'] not in ('close','medium_close') or
                    shot['movement'] not in ('locked','push_in')):
                    errors.append(f'{label} shot {j}: ineligible direct lip-sync geometry/audio')
        if not same(local, duration): errors.append(f'{label}: shots must cover scene-local duration')
    policy = data.get('coverage_policy')
    if policy:
        if len(distinct_openings) < policy['minimum_distinct_openings']:
            warnings.append('Coverage policy: fewer distinct openings than requested; review deliberate repetition')
        if longest_run > policy['maximum_identical_opening_run']:
            warnings.append('Coverage policy: identical opening run exceeds requested limit; review pacing contrast')
    if not same(cursor, data['project']['duration']): errors.append('timeline does not cover project duration')
    if os.path.normpath(audio) == os.path.normpath(data['assembly']['output_path']): errors.append('final output would overwrite audio master')
    if data['project']['timing_basis'] == 'estimated': warnings.append('Timing is estimated; verify against audio before assembly')
    return {'valid': not errors, 'errors': errors, 'warnings': warnings}


def compile_batch(data, media_root=None):
    """Return transparent primitives. Symbolic IDs require actual return-value binding."""
    commands=[]
    def emit(command, args, bind=None):
        row={'command':command,'args':args}
        if bind: row['bind_return']=bind
        commands.append(row)
    pr=data['project']
    root=Path(media_root or Path.cwd()).resolve()
    def media_path(path): return str((root/path).resolve())
    emit('project_set', {k:pr[k] for k in ('width','height','fps')} | {'name':pr['title']})
    emit('add_track', {}, {'track_id':'video_track'})
    emit('import_media', {'path':media_path(pr['audio_master'])}, {'asset_id':'audio_master'})
    emit('add_audio_clip', {'asset_id':'$audio_master','start':0,'duration':pr['duration']})
    for scene in data['scenes']:
        name=f"asset_{scene['id']}"
        emit('import_media', {'path':media_path(scene['output_path'])}, {'asset_id':name})
        emit('add_clip', {'track_id':'$video_track','source_type':'media','asset_id':'$'+name,
                         'start':scene['start'],'duration':scene['end']-scene['start'],
                         'in_point':0,'fit':'contain'}, {'clip_id':f"scene_{scene['id']}"})
    return {'review_only':True,'id_binding_required':True,'media_probe_required':True,
            'commands':commands,'not_implemented':['generation','lip_sync','camera_simulation','automatic_grade']}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('plan',type=Path)
    parser.add_argument('--media-root',type=Path,help='base directory for absolute media paths; default current directory')
    parser.add_argument('--batch', action='store_true', help='emit review-only assembly primitives when valid')
    args=parser.parse_args()
    try:
        data=json.loads(args.plan.read_text(), parse_constant=lambda x: (_ for _ in ()).throw(ValueError(f'non-finite {x}')))
        result=validate(data)
    except (OSError,ValueError,TypeError,OverflowError) as exc:
        result={'valid':False,'errors':[f'cannot validate plan: {exc}'],'warnings':[]}
    if result['valid'] and args.batch: result['batch']=compile_batch(data,args.media_root)
    print(json.dumps(result,indent=2,allow_nan=False))
    return 0 if result['valid'] else 1

if __name__=='__main__': raise SystemExit(main())
