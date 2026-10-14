"""Fast contract gates; no cargo, engine execution, network or generation."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest

HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('director_validator',HERE/'validate_plan.py')
v=importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)
BASE=json.loads((HERE/'example.plan.json').read_text())

class ContractTests(unittest.TestCase):
    def setUp(self): self.p=copy.deepcopy(BASE)
    def valid(self): self.assertTrue(v.validate(self.p)['valid'],v.validate(self.p))
    def invalid(self): self.assertFalse(v.validate(self.p)['valid'])
    def test_example(self): self.valid()
    def test_root_list(self): self.p=[]; self.invalid()
    def test_empty_scenes(self): self.p['scenes']=[]; self.invalid()
    def test_null_scene(self): self.p['scenes'][0]=None; self.invalid()
    def test_missing_project(self): del self.p['project']; self.invalid()
    def test_unknown_field(self): self.p['preset']='magic'; self.invalid()
    def test_nonfinite(self): self.p['project']['duration']=float('nan'); self.invalid()
    def test_infinity(self): self.p['project']['fps']=float('inf'); self.invalid()
    def test_bool_number(self): self.p['project']['duration']=True; self.invalid()
    def test_numeric_string(self): self.p['scenes'][0]['start']='0'; self.invalid()
    def test_duplicate_id(self): self.p['scenes'][1]['id']=0; self.invalid()
    def test_gap(self): self.p['scenes'][1]['start']+=.01; self.invalid()
    def test_overlap(self): self.p['scenes'][1]['start']-=.01; self.invalid()
    def test_short_tail(self): self.p['scenes'][1]['end']=14; self.invalid()
    def test_duration_exception(self):
        s=self.p['scenes'][1]; s['end']=14; s['duration_exception']='2s outro, measured phrase boundary'
        s['shots'][0]['end']=2; self.p['project']['duration']=14; self.valid()
    def test_final_duration(self): self.p['project']['duration']=25; self.invalid()
    def test_reverse_shot(self): self.p['scenes'][0]['shots'][0]['end']=0; self.invalid()
    def test_shot_gap(self): self.p['scenes'][0]['shots'][0]['start']=1; self.invalid()
    def test_shot_overflow(self): self.p['scenes'][0]['shots'][0]['end']=13; self.invalid()
    def test_whitespace_prompt(self): self.p['scenes'][0]['motion_prompt']='  '; self.invalid()
    def test_collision(self): self.p['scenes'][1]['output_path']=self.p['scenes'][0]['output_path']; self.invalid()
    def test_final_collision(self): self.p['assembly']['output_path']=self.p['scenes'][0]['output_path']; self.invalid()
    def test_audio_collision(self): self.p['assembly']['output_path']=self.p['project']['audio_master']; self.invalid()
    def test_unknown_capability_multi(self):
        s=self.p['scenes'][0]; second=copy.deepcopy(s['shots'][0]); s['shots'][0]['end']=6; second['start']=6
        s['shots'].append(second); self.invalid()
    def test_source_required(self): self.p['scenes'][0]['generation_capability']='single_shot_verified'; self.invalid()
    def test_repeat_warning(self):
        self.p['scenes'][1]['shots'][0]['framing']='wide'; self.p['scenes'][1]['shots'][0]['movement']='locked'
        self.valid(); self.assertTrue(any('repeated' in x for x in v.validate(self.p)['warnings']))
    def test_invalid_lipsync(self): self.p['scenes'][0]['performance_mode']='direct_lipsync'; self.invalid()
    def test_valid_lipsync_geometry(self):
        s=self.p['scenes'][0]; s['performance_mode']='direct_lipsync'
        s['shots'][0].update(has_vocals=True,singer_count=1,mouth_visible=True,orientation='front',framing='close')
        self.valid()
    def test_unsafe_lipsync_move(self):
        s=self.p['scenes'][0]; s['performance_mode']='direct_lipsync'
        s['shots'][0].update(has_vocals=True,singer_count=1,mouth_visible=True,orientation='front',framing='close',movement='orbit')
        self.invalid()
    def test_batch_transparent(self):
        b=v.compile_batch(self.p); self.assertTrue(b['review_only']); self.assertTrue(b['id_binding_required'])
        self.assertEqual(len(b['commands']),8)
        self.assertEqual(b['commands'][-1]['args']['start'],12)
        self.assertFalse(any(x['command']=='export' for x in b['commands']))
    def test_huge_integer(self): self.p['project']['fps']=10**1000; self.invalid()
    def test_negative_time(self): self.p['scenes'][0]['start']=-1; self.invalid()
    def test_enum_error(self): self.p['scenes'][0]['shots'][0]['movement']='epic'; self.invalid()
    def test_empty_shots(self): self.p['scenes'][0]['shots']=[]; self.invalid()
    def test_path_normalization(self):
        self.p['scenes'][1]['output_path']='outputs/../outputs/scene-00.mp4'; self.invalid()
    def test_absolute_imports(self):
        b=v.compile_batch(self.p,'/tmp/director-media')
        self.assertEqual(b['commands'][2]['args']['path'],'/tmp/director-media/input/song.wav')
    def test_exact_shot_coverage(self):
        s=self.p['scenes'][0]; other=copy.deepcopy(s['shots'][0]); s['shots'][0]['end']=6; other['start']=6
        s['shots'].append(other); s['generation_capability']='multi_shot_verified'; s['capability_source']='fixture only, not a real capability claim'; self.valid()
    def test_batch_commands_exist(self):
        source='\n'.join(f.read_text() for f in (HERE.parent.parent/'crates/kimo-core/src').rglob('*.rs'))
        for row in v.compile_batch(self.p)['commands']:
            self.assertIn('name: "'+row['command']+'"',source)

V2=json.loads((HERE/'example.v2.plan.json').read_text())
class ExtendedContracts(unittest.TestCase):
    def setUp(self): self.p=copy.deepcopy(V2)
    def result(self): return v.validate(self.p)
    def invalid(self): self.assertFalse(self.result()['valid'])
    def test_v2_fixture(self): self.assertTrue(self.result()['valid'])
    def test_required_progression(self): del self.p['scenes'][0]['progression']; self.invalid()
    def test_required_lighting(self): del self.p['scenes'][0]['shots'][0]['lighting_contract']; self.invalid()
    def test_missing_light_direction(self): del self.p['scenes'][0]['shots'][0]['lighting_contract']['direction']; self.invalid()
    def test_empty_movement_rationale(self): self.p['scenes'][0]['shots'][0]['movement_contract']['rationale']=' '; self.invalid()
    def test_zero_focal(self): self.p['scenes'][0]['shots'][0]['optics_contract']['focal_length_mm']=0; self.invalid()
    def test_negative_distance(self): self.p['scenes'][0]['shots'][0]['optics_contract']['subject_distance_m']=-1; self.invalid()
    def test_shutter_excess(self): self.p['scenes'][0]['shots'][0]['optics_contract']['shutter_degrees']=361; self.invalid()
    def test_optical_nonfinite(self): self.p['scenes'][0]['shots'][0]['optics_contract']['focal_length_mm']=float('nan'); self.invalid()
    def test_boolean_focal(self): self.p['scenes'][0]['shots'][0]['optics_contract']['focal_length_mm']=True; self.invalid()
    def test_invalid_dof(self): self.p['scenes'][0]['shots'][0]['optics_contract']['depth_of_field']='magic'; self.invalid()
    def test_unknown_light_field(self): self.p['scenes'][0]['shots'][0]['lighting_contract']['preset']='movie'; self.invalid()
    def test_unknown_progression_mode(self): self.p['scenes'][0]['progression']['mode']='auto'; self.invalid()
    def test_abstract_progression(self): self.p['scenes'][0]['progression']['mode']='abstract'; self.assertTrue(self.result()['valid'])
    def test_new_camera_vocabulary(self):
        for movement in ('pedestal','leading_track','whip_pan','body_mount'):
            self.p['scenes'][0]['shots'][0]['movement']=movement; self.assertTrue(self.result()['valid'])
    def test_new_camera_not_lipsync_safe(self): self.p['scenes'][0]['performance_mode']='direct_lipsync'; self.p['scenes'][0]['shots'][0]['movement']='whip_pan'; self.invalid()
    def test_reference_role(self): self.p['scenes'][0]['references']=[{'id':'A','path':'ref.png','role':'unknown','transfer':'composition','exclude':'identity'}]; self.invalid()
    def test_duplicate_reference(self):
        r={'id':'A','path':'ref.png','role':'camera_motion','transfer':'path','exclude':'performer identity'}
        self.p['scenes'][0]['references']=[r,copy.deepcopy(r)]; self.invalid()
    def test_reference_exclusion_required(self): self.p['scenes'][0]['references']=[{'id':'A','path':'ref.png','role':'camera_motion','transfer':'path'}]; self.invalid()
    def test_density_warning(self):
        s=self.p['scenes'][0]; del s['pacing_reason']; s['shots'][0]['density']='high'
        self.assertTrue(self.result()['valid']); self.assertTrue(any('density' in x for x in self.result()['warnings']))
    def test_emotion_warning(self):
        s=self.p['scenes'][0]['progression']; s['emotion_out']=s['emotion_in']
        self.assertTrue(any('emotion' in x for x in self.result()['warnings']))
    def test_circular_beat_warning(self):
        s=self.p['scenes'][0]['progression']; s['consequence']=s['cause']
        self.assertTrue(any('unchanged' in x for x in self.result()['warnings']))
    def test_v1_compatibility(self): self.assertTrue(v.validate(BASE)['valid'])
    def test_invalid_coverage_policy(self): self.p['coverage_policy']['minimum_distinct_openings']=0; self.invalid()
    def test_boolean_coverage_count(self): self.p['coverage_policy']['maximum_identical_opening_run']=True; self.invalid()
    def test_coverage_count_warning(self):
        self.p['coverage_policy']['minimum_distinct_openings']=3
        self.assertTrue(self.result()['valid']); self.assertTrue(any('fewer distinct' in x for x in self.result()['warnings']))
    def test_run_warning(self):
        self.p['scenes'][1]['shots'][0]['framing']='wide'; self.p['scenes'][1]['shots'][0]['movement']='locked'
        self.assertTrue(self.result()['valid']); self.assertTrue(any('opening run' in x for x in self.result()['warnings']))
    def test_optional_coverage_policy(self): del self.p['coverage_policy']; self.assertTrue(self.result()['valid'])
    def test_empty_lens_rationale(self): self.p['scenes'][0]['shots'][0]['lens_rationale']=' '; self.invalid()
    def test_empty_light_change_reason(self): self.p['scenes'][0]['shots'][0]['light_change_reason']=' '; self.invalid()
    def test_v2_batch_unchanged_primitives(self): self.assertEqual(len(v.compile_batch(self.p)['commands']),8)

V3=json.loads((HERE/'example.v3.plan.json').read_text())
class MusicHandoffContracts(unittest.TestCase):
    def setUp(self): self.p=copy.deepcopy(V3)
    def result(self): return v.validate(self.p)
    def invalid(self): self.assertFalse(self.result()['valid'])
    def test_fixture_v3(self): self.assertTrue(self.result()['valid'])
    def test_missing_v3_physical_block(self): del self.p['scenes'][0]['shots'][0]['optics_contract']; self.invalid()
    def test_duplicate_cue(self): self.p['music_cues'].append(copy.deepcopy(self.p['music_cues'][0])); self.invalid()
    def test_negative_cue_time(self): self.p['music_cues'][0]['time']=-1; self.invalid()
    def test_cue_beyond_song(self): self.p['music_cues'][0]['time']=25; self.invalid()
    def test_unknown_cue_kind(self): self.p['music_cues'][0]['kind']='imagined_drop'; self.invalid()
    def test_cue_evidence_required(self): del self.p['music_cues'][0]['evidence']; self.invalid()
    def test_cue_nan(self): self.p['music_cues'][0]['time']=float('nan'); self.invalid()
    def test_unknown_binding(self): self.p['scenes'][1]['shots'][0]['cue_binding']['cue_id']='missing'; self.invalid()
    def test_binding_timing(self): self.p['scenes'][1]['shots'][0]['cue_binding']['offset_seconds']=1; self.invalid()
    def test_intentional_offset(self):
        self.p['music_cues'][0]['time']=13; self.p['scenes'][1]['shots'][0]['cue_binding']['offset_seconds']=-1
        self.assertTrue(self.result()['valid'])
    def test_readability_warning(self):
        self.p['scenes'][0]['shots'][0]['minimum_readable_seconds']=20
        self.assertTrue(self.result()['valid']); self.assertTrue(any('readability' in x for x in self.result()['warnings']))
    def test_negative_readability(self): self.p['scenes'][0]['shots'][0]['minimum_readable_seconds']=-1; self.invalid()
    def test_unknown_review_scene(self):
        self.p['review_records']=[{'scene_id':7,'media_path':'take.mp4','status':'unreviewed','evidence':[],'blockers':[],'reason':'no review'}]; self.invalid()
    def test_selected_without_evidence(self):
        self.p['review_records']=[{'scene_id':0,'media_path':'take.mp4','status':'selected','evidence':[],'blockers':[],'reason':'chosen'}]; self.invalid()
    def test_selected_with_blocker(self):
        self.p['review_records']=[{'scene_id':0,'media_path':'take.mp4','status':'selected','evidence':['frame.png'],'blockers':['identity drift'],'reason':'chosen'}]; self.invalid()
    def test_review_pointer_not_certification(self):
        self.p['review_records']=[{'scene_id':0,'media_path':'take.mp4','status':'selected','evidence':['frame.png'],'blockers':[],'reason':'structural fixture only'}]
        self.assertTrue(self.result()['valid']); self.assertTrue(any('unverified' in x for x in self.result()['warnings']))
    def test_invalid_review_status(self):
        self.p['review_records']=[{'scene_id':0,'media_path':'take.mp4','status':'perfect','evidence':[],'blockers':[],'reason':'bad enum'}]; self.invalid()
    def test_optional_music_and_reviews(self):
        del self.p['music_cues']; del self.p['review_records']; del self.p['scenes'][1]['shots'][0]['cue_binding']; self.assertTrue(self.result()['valid'])
    def test_old_versions_compatible(self):
        for plan in (BASE,V2): self.assertTrue(v.validate(plan)['valid'])
    def test_v3_batch_review_only(self): self.assertTrue(v.compile_batch(self.p)['review_only'])

if __name__=='__main__': unittest.main()
