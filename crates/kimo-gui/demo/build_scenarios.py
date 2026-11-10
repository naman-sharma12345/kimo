#!/usr/bin/env python3
"""Author the 16 demo scenarios (one per mockup) as engine events.

Writes demo/scenarios.json. Each scenario is the event stream an engine would
send for that screen: a session reset, the mode, the panel state (view
patches), stats, and the tool calls with their real durations. The viewer
frames are crops of the mockups under demo/frames/ (demo only; never part of
the shipped ui/ bundle).
"""
import json, os
HERE = os.path.dirname(os.path.abspath(__file__))
F = lambda n: f"../demo/frames/{n}.jpg"

def K(k, v, **kw):
    d = {"k": k, "v": v}; d.update(kw); return d
def S(t, **kw):
    d = {"sec": t}; d.update(kw); return d

SC = []
def scen(id, mode, project, agent, elapsed, calls, views, stats=None, activity=None, final=None):
    SC.append(dict(id=id, mode=mode, project=project, agent=agent, elapsed=elapsed, calls=calls,
                   views=views, stats=stats or {}, activity=activity, final=final or {}))

def hms(s):
    h, m, x = s.split(":"); return int(h) * 3600 + int(m) * 60 + int(x)

# ---------------------------------------------------------------- m16 edit
scen("m16", "edit", "dune_film.kimo", "codex", hms("00:12:48"),
  [["open_project", ["dune_film"], 200], ["import_media", ["dune_aerial.mov"], 400], ["import_media", ["walk_ridge.mov"], 400],
   ["cut", ["clip_02", {"at": "00:06:12"}], 100], ["trim", ["clip_03", {"in": "00:02.10"}], 100], ["add_transition", ["cross", "12f"], 200],
   ["mix_audio", ["music", "-14 LUFS"], 600], ["caption", [{"track": "en"}], 1800], ["grade", ["clip_04", {"warmth": "+18"}], 800]],
  {"viewer": {"src": F("m16"), "tag": "grade - clip_04", "tc": "01:00:14:07", "res": "1920 x 1080 24p"},
   "insp": {"sub": "clip_04 · dune_ridge", "blocks": [K("Position", "0, 0"), K("Scale", "104%"), K("Rotation", "0.0"), K("Opacity", "100%"),
            S("Grade"), K("Temperature", "6050K", **{"from": "5600K"}), K("Tint", "+2"), K("Saturation", "1.08"), K("Contrast", "1.12")]},
   "tl": {"ruler": {"t0": 0, "t1": 21.9, "step": 5, "last": 20, "minor": 1, "align": "left"}, "playhead": 9.5, "tracks": [
      {"label": "V2", "clips": [{"s": 0, "e": 9.95, "label": "dune_aerial", "thumb": F("m16_t1")}, {"s": 10.1, "e": 21.9, "label": "sky_wide", "thumb": F("m16_t2")}]},
      {"label": "V1", "clips": [{"s": 0, "e": 7.6, "label": "walk_ridge", "thumb": F("m16_t3")}, {"s": 7.72, "e": 14.37, "label": "dune_wide", "thumb": F("m16_t4")}, {"s": 14.54, "e": 21.9, "label": "dune_ridge", "thumb": F("m16_t5"), "cls": "sel"}]},
      {"label": "A1", "area": {"seed": 3, "amp": .45}},
      {"label": "A2", "area": {"seed": 8, "amp": .35, "base": .6}},
      {"label": "CAP", "clips": [{"s": 0, "e": 5.0, "label": "The desert keeps time.", "cls": "cap"}, {"s": 7.95, "e": 11.3, "label": "Every step erased.", "cls": "cap"}, {"s": 14.9, "e": 18.8, "label": "Walk anyway.", "cls": "cap"}]}]}},
  stats={"cpu": 61, "ffmpeg": "7.1", "saved_s": 2}, final={"count": 41})

# ---------------------------------------------------------------- m01 captions
scen("m01", "captions", "dune_film.kimo", "codex", hms("00:18:36"),
  [["transcribe", ["clip_03", {"lang": "en"}], 2400], ["transcript_to_captions", [{"track": "CAP"}], 300], ["set_caption_style", ["kinetic_pop"], 100],
   ["add_title", ['"DUNE"', {"t": "00:00"}], 100], ["add_text_animator", ["title_01", "fade_up"], 200], ["set_text_path", ["title_02", "arc_01"], 100],
   ["validate_captions", ["CAP"], 400], ["transcript_edit_word", ["w_142", '"erased"'], 600]],
  {"viewer": {"src": F("m01"), "tc": "01:00:18:02"},
   "insp": {"sub": "title_01 · DUNE", "blocks": [K("Font", "Geist 500"), K("Size", "96"), K("Tracking", "+40"), K("Animator", "fade_up"), K("Range", "0 – 100%"),
            K("Offset Y", "24px"), K("Opacity", "0 – 100"), K("Stagger", "2f"), S("Captions"), K("Style", "kinetic_pop"), K("Max chars", "32"), K("Check", "0 issues")]},
   "trans": {"lines": [{"spk": "S1", "tc": "00:09", "text": "The desert keeps time."},
                       {"spk": "S1", "tc": "00:14", "html": "Every step <s>erase</s> <span class=\"fix\">erased</span> ."},
                       {"spk": "S1", "tc": "00:19", "text": "Walk anyway."}]},
   "tl": {"ruler": {"t0": 0, "t1": 20, "step": 5, "ticks": False, "edge": True}, "playhead": 9.67, "tracks": [
      {"label": "V2", "clips": [{"s": 0, "e": 10.17, "label": "walk_ridge", "cls": "mauve", "thumb": F("m01_t1")}, {"s": 10.36, "e": 20, "label": "dune_ridge", "cls": "mauve", "thumb": F("m01_t2")}]},
      {"label": "V1", "lane": "#252422", "clips": []},
      {"label": "A1", "area": {"pts": [1.0, 0.83, 0.49, 0.34, 0.57, 0.63, 0.43, 0.23, 0.4, 0.66, 0.86, 0.86, 0.91, 1.0], "bg": "#252422", "fill": "#5f5f59"}},
      {"label": "CAP", "clips": [{"s": 0, "e": 1.64, "label": "The", "cls": "cap"}, {"s": 1.78, "e": 4.03, "label": "desert", "cls": "cap"}, {"s": 4.19, "e": 6.33, "label": "keeps", "cls": "cap"}, {"s": 6.47, "e": 8.38, "label": "time.", "cls": "cap"},
                                 {"s": 8.52, "e": 10.58, "label": "Every", "cls": "cap"}, {"s": 10.74, "e": 12.52, "label": "step", "cls": "cap"}, {"s": 12.68, "e": 15.12, "label": "erased.", "cls": "cap emb"},
                                 {"s": 15.29, "e": 17.21, "label": "Walk", "cls": "cap"}, {"s": 17.34, "e": 20.0, "label": "anyway.", "cls": "cap"}]}]}},
  stats={"cpu": 58, "ffmpeg": "7.1", "saved_s": 1})

# ---------------------------------------------------------------- m02 batch
scen("m02", "batch", "dune_film.kimo", "codex", hms("00:31:20"),
  [["history_list", [], 100], ["checkpoint", ['"before_recipe"'], 200], ["relink_media", ["/footage/dune"], 600], ["recipe_apply", ["film_look", {"clips": 6}], 1400],
   ["template_apply", ["lower_third_01"], 300], ["project_diff", ["cp_11", "cp_12"], 200], ["undo", [], 100], ["batch", ["grade", {"clips": 6}], 2100]],
  {"viewer": {"src": F("m02"), "tc": "01:00:09:12", "thumbs": [{"src": F(f"m02_t{i}"), "check": i < 5, "sel": i == 5} for i in range(1, 7)]},
   "hist": {"items": [{"id": "cp_12", "name": "after_recipe", "ago": "now", "live": True, "src": F("m02_h1"), "sel": True},
                      {"id": "cp_11", "name": "before_recipe", "ago": "2m", "src": F("m02_h2")}, {"id": "cp_10", "name": "captions_done", "ago": "9m", "src": F("m02_h3")},
                      {"id": "cp_09", "name": "rough_cut", "ago": "18m", "src": F("m02_h4")}, {"id": "cp_08", "name": "import", "ago": "31m", "src": F("m02_h5")}]},
   "diff": {"title": "Diff", "titleSmall": "cp_11 to cp_12", "diff": [["+", "node look clip_02"], ["~", "  saturation 1.00 -> 1.10"], ["~", "  contrast   1.00 -> 1.14"], ["~", "  grain 0 -> 12"], ["-", " lut old_rec709.cube"], ["+", "title lower_third_01"]],
            "batch": {"title": "Batch grade", "meta": "4 / 6", "rows": [["clip_01", "film_look", "done"], ["clip_02", "film_look", "done"], ["clip_03", "film_look", "done"], ["clip_04", "film_look", "done"], ["clip_05", "film_look", "grading"], ["clip_06", "film_look", "queued"]]}},
   "media": {"items": [{"name": "dune_aerial.mov", "note": "relinked", "link": True, "src": F("m02_m1")}, {"name": "walk_ridge.mov", "note": "relinked", "link": True, "src": F("m02_m2")}, {"name": "sky_wide.mov", "note": "relinked", "link": True, "src": F("m02_m3")}, {"name": "wind_sfx.wav", "note": "relinked", "link": True, "src": F("m02_m4")}],
             "foot": "collect_project: 14 files, 6.2 GB"}},
  final={"progress": {"done": 4, "total": 6}, "running": True})

# ---------------------------------------------------------------- m03 graph editor
def curve(f, a, b, n=60):
    return [[a + (b - a) * i / n, f(a + (b - a) * i / n)] for i in range(n + 1)]
import math
scen("m03", "graph", "skate_edit.kimo", "codex", hms("00:27:40"),
  [["add_null", ["ctrl"], 100], ["set_parent", ["title", "ctrl"], 100], ["set_motion_path", ["ctrl"], 200], ["bind_beats", ["scale", 2], 300], ["set_expression", ["rot"], 100], ["set_keyframe_bezier", ["y"], 200]],
  {"viewer": {"src": F("m03"), "left": "00:00:03:12", "res": "1920 x 1080 24p"},
   "insp": {"sub": "ctrl", "blocks": [S("Transform"), K("Position", "960, 412"), K("Scale", "104%"), K("Rotation", "12.0"), S("Expression"), {"code": "wiggle(2, 8)\n+ beat * 6"}, S("Motion blur"), K("Shutter", "180")]},
   "graph": {"meta": "beats 120 bpm", "ruler": {"t0": 0, "t1": 5.79, "step": 1, "fmt": "s", "align": "left", "ticks": False}, "beats": {"t0": 0.01, "step": 0.1}, "playhead": 3.06,
      "lanes": ["position y", "scale", "rotation"],
      "curves": [{"smooth": True, "color": "#e9e4d8", "pts": [[0, .188], [1, .837], [2, .656], [3, .276], [4, .388], [5, .419], [5.79, .419]],
                  "keys": [[0, .188], [1, .837], [2, .656], [3, .276], [4, .388], [5, .419]]},
                 {"smooth": True, "color": "#8b877f", "pts": [[0, .188], [1, .613], [1.5, .70], [2, .66], [3, .419], [4, .47], [5, .625], [5.79, .681]],
                  "keys": [[1, .613], [3, .419], [5, .625]]},
                 {"smooth": True, "color": "#66625b", "pts": [[0, .188], [1, .55], [2, .188]], "keys": [[1, .55], [2, .188]]},
                 {"em": True, "smooth": True, "pts": [[2, .188], [2.3, .22], [2.6, .42], [2.85, .70], [3, .818], [3.2, .893], [3.5, .84], [3.8, .66], [4, .594], [4.5, .63], [5, .656], [5.79, .613]],
                  "handles": [[[2.716, .651], [3.254, 1.0]]], "keys": [[3, .818], [4, .594], [5, .656]]}]}},
  )

# ---------------------------------------------------------------- m04 mixing
scen("m04", "mixing", "dune_film.kimo", "codex", hms("00:21:09"),
  [["measure_loudness", ["mix"], 900], ["detect_beats", ["music_01"], 700], ["audio_transients", ["sfx_wind"], 700], ["set_audio", ["dialogue_01", {"gain": "+2 dB"}], 100],
   ["set_audio_pan", ["sfx_wind", "-0.30"], 100], ["add_audio_effect", ["dialogue_01", "eq"], 200], ["normalize_loudness", ["mix", "-14 LUFS"], 1100], ["auto_duck", ["music_01", {"under": "dialogue"}], 800]],
  {"viewer": {"src": F("m04"), "tc": "01:00:21:14"},
   "audio": {"sub": "dialogue_01", "blocks": [{"widget": "eq", "h": 131, "labH": 24, "labY": 13, "style": "border:none;background:none;margin:7px 0 0", "bands": [{"f": 100, "type": "highpass"}, {"f": 210, "g": 2.6, "q": 1.4, "dim": True}, {"f": 495, "g": -0.6, "q": 1.6}, {"f": 2960, "g": 4.6, "q": 1.0, "sel": True}, {"f": 16000, "type": "lowpass"}]},
              {"widget": "comp", "h": 86, "threshold": -18, "ratio": 3, "style": "border:none;border-bottom:1px solid var(--line2);border-radius:0;background:none;margin:4px 0 0"}, K("Attack", "10 ms"), K("Release", "120 ms"), K("Pan", "C")]},
   "tracks": {"ruler": {"t0": -0.39, "t1": 17.65, "step": 5, "first": 0, "ticks": False}, "playhead": 8.93, "tracks": [
      {"label": "Dialogue", "lane": "#463e33", "wave": {"peaks": "001111111111111111111111122222332344322223336776643332229difeeijheopfchtpnhcdc963233eeeaii943336cc976322htkoeecfdaeonhdfeecedcc9ccacdjlfhhffca9cfhfe964226klca7a43tsjjijhdd967a866996643479876avlidc844337hhjltokitpc97eiheaad97447ef9774323stoptuutjeafjhd9732347uvtqoffec764337fe97dhc8adaiqvvfc63334iuvtqkhjlopljlkkpoidfec97lxvtsuuyxvsqqsqkheaa9766433333224eyysvpkosxuqkjfdca87444442222222iijkoklijeqzzystsekiedeeedffadc943237jkfhe9646vxuixkfkuuohlefadcloloqqkd7644qqlkojsvsjkljihha7669vxqiljifcccde9cec8644322222222224fklieipssk0000003322jonjhc9ddpxyvpljssldda988988dfjkifffjcihfied99dac986443643342ajottqkkktsnyytlfchceadeecahtqonkidhiheeedcddaa8667pvvqp9pynspjljiffhjikfefopkfdaa9ac9976334437einkjlqqljhhlpjdc9864433322222222239dfnqkhc9743322222322222234acilieinifhdd9ckkid743333aptlkdppeidakllkedeaa9874648aa8862224qpfkjdd9d74ppfiea98ehd4333fhcc9ciiefdceifalph8da76747ikdfhiehfhiccenleed87744jjjh966iif9ed648eecdcccdcehhkjfosiiifhhfsqjkljc433323jjonifikjdceda96633222223dhpphadkkec884432223222222111111111111111111111100", "color": "#d2c5ae"}},
      {"label": "Music", "lane": "#2a2620", "wave": {"peaks": "00111111111111111111112442245467657756567575aa77642222222557767799c774467652246654256765676652577444222457766676667ahfcba979eabb7cbcba996757a997ba7965574466544442665424256642276556754246a97997997a77755654445557999796664445244456654444555504422244222222221111111111111111111111111111111565422222579cba76799677797999722222767776556576a7997579775442244222447hhfeghkgiggfcaabbbba995442242469acbbeefmlknlnlbifbgf9ee9b977779feaebb9bb9ba9cfeackhlkgfacaabbb9a9babb9744299ba99bebee9abcfbbbaa6544fhfbfbffbecbbfigebaa75554544222000001111010111111696444247bcfca7abbb96554422444566562456099669669666664454444447bcgfccgbcbggbfe9976766977b97a77766a79967775554447bcbefgggghceeba776676569967b999665554422424479a9aaacfebc9acb754222222100000111111111111111111111111111111124764577667646675565221000775559766644b6a96424544222266555422469667644569egaa65657a974akhfc9799bc9749gmbkpibgg7744b7474444figkhe7a976446545ffacbafike77957fhca9667a9677aa7bbb7556b66f6569knigfaema755647a9be9aff9cc9496ab7gmmgha44442cfca6797ba7ahib6444442cgeabebfgf721100", "color": "#9d927f", "beats": [round(0.18 + i * 0.664, 3) for i in range(27)],
                "env": [[-0.39, 0.82], [3.58, 0.82], [4.34, 0.14], [5.14, 0.14], [5.94, 0.82], [8.74, 0.82], [9.24, 0.14], [9.54, 0.14], [10.14, 0.82], [12.44, 0.82], [12.94, 0.14], [13.54, 0.14], [14.04, 0.82], [17.65, 0.82]]}},
      {"label": "SFX", "lane": "#2a2620", "wave": {"peaks": "001111111111111111111111111111111111111111111111111111112222222223444644334334443333333346646646667644666747768864446688aec8778hiiffdc8aa8c9eecefnnidccaca9667667cheedzzxqlkkj99hhaa676clupvyvshiic968ca6432233iiiddaccaca9a79ikllsvqvzssqqqjotpkefhdeahidjljfjhe9ivvqsjljiiljinnjkkffjtusnjidideadeijciec6esslttjdkfcnjfkfifdhd9jiefca8876elhiiedeccqshekhd9986hjhc87433aksnh9ac88a7777474896cc633339ha73333366lfiic996633422334qpic844323333443337ppefecaa776344488646644jlhhd977644433huka632339zxpll8a68646964743334cc66434432334xztptjefa0zccaa9a897664433yvppkfahc7ceec88643oojhjjkj9ac8987666cf9fhcdadd99987766cda767767a986766786478644444467998766447886676677876886467647446468876466467666446444686774777667776687447746644449c9666667cf986686664446444444444688677466446444447acca9889987766444443444664333333334344333344466876677768744644644apzvqyzxuottplhhecaaa887649cdccccefecc9fic79d866lqoidfhh8644djjhceeihivytsjjfhda9hccc88669puxusnijjhafiea8643699867866774644663344334422344443322232222222211111111111111111111111111111111111100", "color": "#c2b6a0"}}]},
   "loud": {"lufs": "-14.2", "target": "-14", "rows": [["True peak", "-1.3 dBTP"], ["LRA", "6.1 LU"]], "meters": [{"l": "L", "v": .613, "peak": .645, "led": "#8d887e"}, {"l": "R", "v": .577, "led": "#ff5a23"}]}})

# ---------------------------------------------------------------- m05 color science
scen("m05", "color", "dune_film.kimo", "codex", hms("00:26:51"),
  [["analyze_exposure", ["clip_07"], 500], ["render_scope", ["clip_07", "waveform"], 200], ["render_scope", ["clip_07", "vectorscope"], 200], ["render_exposure_false_color", ["clip_07"], 300],
   ["render_ocio_display_view", ["ACES2", "PQ"], 600], ["render_scope", ["clip_07", "cie1931"], 300], ["hdr_validate", ["clip_07", "PQ 1000"], 400], ["render_gamut_excursions", ["clip_07"], 900]],
  {"viewer": {"src": F("m05"), "tags": [{"text": "source"}, {"text": "false color", "pos": "tr"}], "tc": "01:00:42:19", "res": "3840 x 2160 24p", "falsecolor": {"split": 0.5075}},
   "pipe": {"blocks": [K("input", "ARRI LogC4"), K("working", "ACEScg"), K("view", "ACES 2.0", emb=True, bar=True), K("output", "Rec.2100 PQ"), S("HDR"),
             K("MaxCLL", "812 nits", h=34), K("MaxFALL", "214 nits", h=34), K("Peak", "1000 nits", h=34), K("Out of gamut", "2.4%", h=34), K("PQ check", "pass", h=34)]},
   "scopes": json.load(open(os.path.join(HERE, "m05_scopes.json"))), "cie": {"gamuts": [{"pts": [[.68, .32], [.265, .69], [.15, .06]], "label": "P3", "color": "#efe9dd", "w": 2, "lpt": [.255, .715]}, {"pts": [[.64, .33], [.30, .60], [.15, .06]], "label": "Rec.709", "color": "#ffffff", "w": 1.6, "lpt": [.13, .2]}], "points": [[.3127, .329, "ring"]]}})

# ---------------------------------------------------------------- m06 effects
scen("m06", "effects", "neon_run.kimo", "claude", hms("00:18:22"),
  [["add_effect", ["clip_07", "glitch"], 300], ["set_effect_param", [{"amount": "0.42"}], 100], ["add_effect", ["clip_07", "chromatic"], 200], ["set_effect_mask", ["chromatic"], 200],
   ["add_adjustment_layer", ["V3"], 100], ["set_posterize_time", ["12fps"], 100], ["add_transition", ["whip", "10f"], 200], ["add_speed_lines", ["clip_07"], 700]],
  {"viewer": {"src": F("m06"), "tag": "fx - clip_07", "tc": "01:00:09:14", "res": "1920 x 1080 24p"},
   "insp": {"sub": "clip_07 · night_street", "blocks": [K("Position", "0, 0"), K("Scale", "104%"), K("Rotation", "0.0"), K("Opacity", "100%"), S("Effect stack"),
            {"list": ["glitch", "chromatic", "speed lines"], "sel": 2}, S("speed lines"), K("Density", "64"), K("Length", "0.35"), K("Center", "960, 540"), K("Amount", "0.60", **{"from": "0.20"}), K("Blend", "screen")]},
   "tl": {"ruler": {"t0": 4.21, "t1": 14.14, "step": 2, "first": 6, "minor": 2, "minorFrom": 5, "ticks": False}, "playhead": 9.77, "tracks": [
      {"label": "V2", "h": 36, "lane": "#39352c", "laneInset": 0, "clips": [{"s": 8.0, "e": 11.99, "label": "adjustment", "cls": "mauve"}]},
      {"label": "V1", "h": 80, "cls": "tall", "clips": [{"s": 4.21, "e": 8.68, "label": "alley_rain", "thumb": F("m06_t1")}, {"s": 8.76, "e": 9.71, "cls": "ghost haspill", "pill": "whip 10f"},
                                     {"s": 9.80, "e": 14.14, "label": "night_street", "thumb": F("m06_t2"), "cls": "sel", "fx": ["glitch", "chromatic", "speed lines"]}]},
      {"label": "A1", "h": 35, "area": {"pts": [0.7, 0.45, 0.62, 0.38, 0.55, 0.42, 0.6, 0.35, 0.5, 0.3, 0.48, 0.4, 0.58, 0.45, 0.62], "bg": "#252422", "fill": "#4f4e49"}}]}},
  stats={"cpu": 74, "ffmpeg": "7.1", "saved_s": 1}, final={"count": 57})

# ---------------------------------------------------------------- m07 tracking
res = [0.73, 0.73, 0.73, 0.73, 0.8, 0.73, 0.77, 0.91, 0.73, 0.91, 1.1, 1.28, 1.1, 1.12, 0.77, 0.73, 0.8, 0.73, 0.64, 0.55, 0.55, 0.6, 0.91, 0.91, 1.14, 1.1, 1.08, 0.55, 0.72, 0.89, 0.59, 0.55, 0.73, 0.83, 0.91, 1.02, 1.1, 1.1, 1.03, 0.68, 0.59, 0.94, 1.1, 0.92, 0.57, 0.55, 1.01, 0.94, 0.91, 0.91, 1.03, 1.1, 0.83, 1.39, 2.67, 2.31, 1.03, 0.75, 0.91, 0.91, 0.9, 0.91, 1.26, 1.1, 0.91, 1.1, 1.1, 1.1, 1.01, 0.91, 0.91, 0.91, 0.91]  # traced from the m07 mockup residual line
scen("m07", "tracking", "city_billboard.kimo", "claude", hms("00:44:51"),
  [["undistort_clip", ["clip_05", {"k1": "-0.08"}], 3200], ["stabilize", ["clip_05", {"smooth": "0.5"}], 4100], ["camera_solve", ["clip_05", {"pan": "false"}], 9800],
   ["track_planar", ["clip_05", "screen_quad"], 6300], ["track_mesh", ["clip_05", "flag", "8x6"], 5000], ["validate_planar_track", ["screen_quad"], 2100]],
  {"viewer": {"src": F("m07"), "tag": "track - screen_quad", "left": "solve 0.70 px RMS", "tc": "01:00:02:21", "res": "1920 x 1080 24p"},
   "insp": {"sub": "screen_quad · planar", "blocks": [K("Confidence", "0.85"), K("Lost frames", "0"), K("Corner p95", "1.8 px"), K("Corner max", "2.6 px", **{"from": "3.4"}), K("Anchors", "4"),
            S("Camera solve"), {"widget": "solve", "h": 133, "src": F("m07_solve")}, {"html": "<div class=\"cap\"><span>focal <b>28.4 mm</b></span><b>96/96 frames</b></div>"}]},
   "res": {"ruler": {"t0": 0, "t1": 72, "step": 12, "fmt": "n"}, "playhead": 41, "graph": {"pts": res, "max": 11.8, "thr": 2, "thrLabel": "2 px"},
           "lanes": [{"label": "screen_quad", "bar": [[0, 72, "#64625c"]]}, {"label": "flag mesh", "bar": [[0, 72, "#5e5c57"]]}]}},
  stats={"cpu": 88, "ffmpeg": "7.1", "saved_s": 2}, final={"count": 72})

# ---------------------------------------------------------------- m08 modelling
scen("m08", "modelling", "amphora_study.kimo", "claude", hms("00:34:12"),
  [["generate_mesh_primitive", [], 100], ["mesh_lathe", ["profile", 96], 400], ["mesh_subdivide", ["vase", 2], 900], ["inspect_sdf_graph", ["vase"], 100], ["contour_sdf_graph", ["vase"], 100], ["mesh_boolean", ["vase", "slot"], 600]],
  {"viewer": {"src": F("m08"), "left": "wireframe on shaded", "res": "persp"},
   "stack": {"nodes": [{"name": "lathe"}, {"name": "subdivide"}, {"name": "boolean diff"}, {"name": "contour_sdf"}]},
   "insp": {"sub": "vase", "blocks": [S("Mesh"), K("Vertices", "24,576"), K("Triangles", "49,152", ch=True), K("Normals", "smooth"), K("Manifold", "yes"), S("Bounds"), K("Height", "1.42 m"), K("Radius", "0.38 m"), S("Export"), K("Format", "gltf")]},
   "prof": {"pts": [[0, 0], [.2417, .089], [.4914, .357], [.6936, .634], [.7812, .853], [1, 1]], "cursor": .4914,
            "thumbs": [{"label": "cylinder", "src": F("m08_t1")}, {"label": "lathe", "src": F("m08_t2")}, {"label": "subdiv", "src": F("m08_t3")}, {"label": "boolean", "src": F("m08_t4")}]}})

# ---------------------------------------------------------------- m09 graphics
scen("m09", "graphics", "drone_hud.kimo", "cursor", hms("00:27:40"),
  [["add_hud_frame", ["V4", "corners"], 200], ["add_detection_box", ["car", "0.94"], 100], ["add_detection_box", ["person"], 100], ["add_counter", ["speed", "0-72"], 200],
   ["add_prompt_box", ["V2"], 100], ["add_path_shape", ["route", "6 pts"], 100], ["add_shape_modifier", ["trim"], 100], ["set_layer_style", ["hud", "stroke"], 400]],
  {"viewer": {"src": F("m09"), "tags": [{"text": "graphics - V4", "x": 25, "y": 21}], "tc": "01:00:06:03", "res": "1920 x 1080 30p"},
   "insp": {"sub": "hud - shape layer", "blocks": [S("Layer style"), K("Stroke", "2 px", **{"from": "1"}), K("Stroke color", "cream"), K("Drop shadow", "off"), K("Inner glow", "off"), K("Opacity", "90%"),
            S("Shape layers"), {"list": ["hud", "det_car", "det_person", "counter_speed", "prompt_1", "route"], "sel": 0}]},
   "tl": {"ruler": {"t0": 0.885, "t1": 11.087, "step": 2, "first": 2, "last": 10, "minor": 0.5, "minorFrom": 1.0}, "playhead": 6.0, "tracks": [
      {"label": "V4", "clips": [{"s": 0.885, "e": 5.553, "label": "hud", "cls": "mauve sel", "sw": True}, {"s": 5.609, "e": 11.087, "label": "counter_speed", "cls": "mauve", "sw": True}]},
      {"label": "V3", "clips": [{"s": 0.885, "e": 4.447, "label": "det_car", "thumb": F("m09_t1")}, {"s": 4.502, "e": 7.609, "label": "det_person", "thumb": F("m09_t2")}, {"s": 7.672, "e": 11.087, "cls": "ghost"}]},
      {"label": "V2", "clips": [{"s": 0.885, "e": 5.553, "label": "prompt_1", "sw": True}, {"s": 5.617, "e": 11.087, "label": "- - - -  route", "sw": True}]},
      {"label": "V1", "clips": [{"s": 0.885, "e": 11.087, "label": "coast_drone", "thumb": F("m09_t3"), "bg": "url(" + F("m09_v1") + ") center/100% 100%"}]}]}},
  stats={"cpu": 54, "ffmpeg": "7.1", "saved_s": 1}, final={"count": 48})

# ---------------------------------------------------------------- m10 particles
scen("m10", "particles", "glass_break.kimo", "claude", hms("00:42:51"),
  [["add_emitter", ["dust", 4000], 100], ["sample_advection_field", [], 300], ["add_shard_burst", ["pane", 220], 500], ["shards_project", ["pane"], 200], ["simulate_physics", ["1-96"], 3800]],
  {"viewer": {"src": F("m10"), "tc": "00:00:01:14", "res": "frame 38 / 96"},
   "emit": {"rows": [{"k": "dust", "v": "4,000/s"}, {"k": "sparks", "v": "600/s"}, {"k": "shards", "v": "220"}]},
   "insp": {"sub": "shard_burst", "blocks": [S("Fracture"), K("Shards", "220", chip=True), K("Seed", "7", chip=True), S("Physics"), K("Gravity", "-9.81", chip=True), K("Restitution", "0.30", chip=True), K("Drag", "0.04", chip=True),
            S("Particles"), K("Alive", "18,402", chip=True, emb=True), K("Lifetime", "2.0 s", chip=True)]},
   "sim": {"metaChip": "cache 38 / 96", "ruler": {"t0": 0, "t1": 96, "step": 24, "fmt": "n"}, "playhead": 38,
           "lanes": [{"label": "cache", "cache": [38, 96]}, {"label": "shards", "dots": [16.9, 19.8, 20.6, 22.6, 25.7, 28.8, 33.8], "r": 3.4, "dotColor": "#8a8780"}, {"label": "dust", "dots": [3.2, 7.5, 11.8, 18.0, 21.8, 23.2, 24.0, 28.8, 33.8, 35.6, 37.4], "r": 3.4, "dotColor": "#8a8780"}]}})

# ---------------------------------------------------------------- m11 lighting
scen("m11", "lighting", "perfume_launch.kimo", "codex", hms("00:18:05"),
  [["light_rig", ["rembrandt", 4], 200], ["set_material", ["bottle", "glass"], 100], ["set_material", ["cap", "metal"], 100], ["preview_material", ["cap"], 300], ["set_relight", ["product"], 400], ["render_wgpu_pbr", [48], 1200]],
  {"viewer": {"src": F("m11"), "tag": "relight rig_key", "tc": "00:00:02:00", "res": "pbr 1920 x 1080"},
   "lights": {"rows": [{"icon": "bulb", "k": "rig_key", "v": "5600K"}, {"icon": "bulb", "k": "rig_fill", "v": "6200K"}, {"icon": "bulb", "k": "rig_rim", "v": "7500K", "dim": True}]},
   "insp": {"sub": "cap", "blocks": [{"widget": "swatches", "h": 81, "d": 60, "gap2": 8, "style": "border:none;border-bottom:1px solid var(--line2);border-radius:0;background:none;margin:12px 0 0 -2px", "items": [
              {"hi": "#ffffff", "mid": "#8d8d8d", "lo": "#1a1a1a", "src": F("m11_sw1")}, {"hi": "#f4efe6", "mid": "#b9b2a6", "lo": "#3a3631", "src": F("m11_sw2")}, {"hi": "#bdb8ae", "mid": "#6f6b64", "lo": "#2a2825", "src": F("m11_sw3")}, {"hi": "#5a5a5a", "mid": "#202020", "lo": "#050505", "src": F("m11_sw4")}]},
            S("Material"), K("Albedo", "0.62"), K("Roughness", "0.34", ch=True), K("Metallic", "1.00"), K("IOR", "1.45"), K("Clearcoat", "0.00")]},
   "rig": {"lights": [{"name": "key", "a": 90, "em": True}, {"name": "fill", "a": 215}, {"name": "rim", "a": 325}],
           "rows": [["rig_key", "5600K", "2.4", .8, True, "1.0"], ["rig_fill", "6200K", "0.6", .25, False, "1.0"], ["rig_rim", "7500K", "1.8", .6, False, "0.8"]],
           "thumbs": [{"label": "ratio 2", "src": F("m11_t1")}, {"label": "ratio 4", "src": F("m11_t2"), "sel": True}, {"label": "ratio 8", "src": F("m11_t3")}, {"label": "ratio 16", "src": F("m11_t4")}]}})

# ---------------------------------------------------------------- m12 roto
def dense(a, b, n): return [a + (b - a) * i / (n - 1) for i in range(n)]
scen("m12", "roto", "studio_portrait.kimo", "codex", hms("00:31:05"),
  [["chroma_key", ["clip_02", "green"], 200], ["spill_suppress", ["clip_02", "0.6"], 600], ["add_mask", ["clip_02", "bezier", "14 pts"], 400], ["set_mask_path_key", ["mask_1", {"f": 112}], 100],
   ["roto_propagate", ["mask_1", "112-180"], 200], ["set_track_matte", ["V2", "luma"], 1800], ["refine_matte", ["mask_1", {"feather": 6}], 1100]],
  {"viewer": {"src": F("m12"), "tag": "mask_1 - clip_02", "tc": "01:00:04:16", "res": "1920 x 1080 25p"},
   "insp": {"sub": "mask_1 · bezier", "blocks": [K("Position", "0, 0, 0"), K("Scale", "104%"), K("Rotation", "0.0"), K("Opacity", "100%"), S("Refine matte", h=38), K("Feather", "6.0 px", tight=True, **{"from": "2.0"}), K("Choke", "-1.5", tight=True), K("Edge contrast", "0.40", tight=True), {"html": "<div style=\"position:relative;height:24px\"><i style=\"position:absolute;left:0;right:0;top:4px;height:1px;background:var(--line2)\"></i><i style=\"position:absolute;left:54.6%;top:0;width:9px;height:9px;margin-left:-4.5px;border-radius:50%;background:var(--cream)\"></i></div>"},
            K("Decontaminate", "on", tight=True), K("Bg model", "auto", tight=True), {"html": "<div style=\"height:1px;background:var(--line2);margin-top:4px\"></div>"}, S("Key", h=50), K("Screen", "green"), K("Spill", "0.60"), S("Quality", h=48), K("Matte MAE", "6.70", tight=True)]},
   "keys": {"ruler": {"t0": 97.965, "t1": 159.0, "step": 10, "first": 100, "last": 150, "fmt": "n", "minor": 10, "minorFrom": 115, "ticks": False}, "playhead": 118,
            "lanes": [{"label": "mask_1 path", "keys": dense(97.8, 157.9, 72), "size": 4.5, "keyColor": "#f1e2c2"},
                      {"label": "mask_1 feather", "keys": [97.8, 111.7, 125.8, 139.9, 153.5], "size": 4.5, "keyColor": "#f1e2c2"},
                      {"label": "mask_1 opacity", "area": [[97.6, .05], [113.2, .05], [114.2, .2], [115.2, .62], [116.4, .93], [117.6, 1], [159.2, 1]], "color": "#6f6c66"},
                      {"label": "track matte", "area": [[113.6, 1], [159.2, 1]], "color": "#6f6c66", "stroke": "#e8e0cc"}]}},
  stats={"cpu": 68, "ffmpeg": "7.1", "saved_s": 3}, final={"count": 63})

# ---------------------------------------------------------------- m13 scene3d
scen("m13", "scene3d", "helix_study.kimo", "cursor", hms("00:00:04"),
  [["scene_add", ["helix_plates"], 2100], ["light_add", ["key", "4200K"], 200], ["light_add", ["rim", "7500K"], 200], ["camera_set", [{"fov": 35}], 100], ["keyframe", ["camera", "pos", "4.0s"], 300]],
  {"viewer": {"src": F("m13"), "left": "00:00:04:00", "res": "1920 x 1080 24p"},
   "scene": {"items": [{"name": "camera_main", "icon": "cam", "sel": True}, {"name": "key_light", "icon": "light"}, {"name": "rim_light", "icon": "light"}, {"name": "fill_light", "icon": "fill"}, {"name": "helix_plates", "icon": "mesh"}, {"name": "floor", "icon": "plane"}]},
   "insp": {"sub": "camera_main", "blocks": [S("Transform"), {"html": "<div class=\"row\" style=\"gap:8px\"><span class=\"k\">Position</span><span class=\"v\" style=\"font-size:15px;letter-spacing:-0.02em;overflow:visible\"><span style=\"font-family:var(--sans);color:var(--dim);margin-right:3px\">X</span>1.20<span style=\"font-family:var(--sans);color:var(--dim);margin:0 3px 0 6px\">Y</span>0.85<span style=\"font-family:var(--sans);color:var(--dim);margin:0 3px 0 6px\">Z</span><span class=em>-3.40</span></span></div>"},
            K("Rotation", "0, 18, 0"), K("FOV", "35"), S("Lens", h=40), K("Focus", "3.6 m"), K("Aperture", "f/2.8"), K("Motion blur", "on")]},
   "keys": {"ruler": {"t0": 0, "t1": 6.37, "step": 1, "fmt": "s", "align": "left", "lx": -3, "ticks": False}, "playhead": 4.0,
            "strip": [{"em": True, "pts": [[0, 0], [0.14, .1], [0.28, .26], [0.4, .46], [0.53, .66], [0.66, .83], [0.79, .94], [0.9, .99], [1.0, 1], [6.37, 1]]},
                      {"pts": [[0, 0], [1.6, 0], [2.18, .02], [2.56, .06], [2.82, .10], [3.13, .18], [3.45, .30], [3.83, .50], [4.09, .64], [4.40, .78], [4.72, .86], [5.04, .94], [5.36, .98], [5.6, 1], [6.37, 1]]}],
            "lanes": [{"label": "<span class=cream>camera_main</span> <span class=dim>position</span>", "html": True, "sel": True, "line": True, "keys": [0, 1, 2, 3, {"t": 4, "em": True}]},
                      {"label": "<span class=cream>camera_main</span> <span class=dim>rotation</span>", "html": True, "line": True, "keys": [0, 1, 2, 3, 4, 6]},
                      {"label": "<span class=cream>key_light</span> <span class=dim>intensity</span>", "html": True, "line": True, "keys": [0, 1, 2, 3, 4, 6]},
                      {"label": "<span class=cream>helix_plates</span> <span class=dim>rotation y</span>", "html": True, "line": True, "keys": [0, 1, 2, 3, 4, 6]}]}},
  activity="animating")
SC[-1]["elapsed"] = hms("00:34:12")

# ---------------------------------------------------------------- m14 export
scen("m14", "export", "dune_film.kimo", "codex", hms("00:16:55"),
  [["trim", ["clip_09", {"out": "01:30:00"}], 100], ["grade", ["clip_04", {"warmth": "+18"}], 800], ["caption", [{"track": "en"}], 1800], ["mix_audio", ["music", "-14 LUFS"], 900],
   ["normalize", ["dialogue"], 400], ["validate_project", [], 300], ["set_export", [{"preset": "web_1080"}], 100], ["export", ["1920x1080", "h264", "crf:18"], 38400]],
  {"agent": {"footer": "project saved 2s ago"},
   "viewer": {"src": F("m14"), "progress": {"pct": 64, "left": "frame 1382 / 2160", "right": "42 fps   eta 00:18"}, "thumbs": [{"src": F(f"m14_t{i}"), "sel": i in (1, 5)} for i in range(1, 9)]},
   "exp": {"blocks": [K("Format", "MP4"), K("Video", "H.264 high"), K("Resolution", "1920 x 1080"), K("Frame rate", "24"), K("Quality", "CRF 18"), K("Audio", "AAC 320 kbps"), K("Loudness", "-14 LUFS"),
            K("Captions", "en.srt"), K("Output", "~/films/dune_film.mp4"), K("Size so far", "41.2 MB")]},
   "renders": {"head": ["name", "preset", "length", "status", "started"], "foot": "cpu 92%   ffmpeg 7.1   disk 212 GB free", "footEm": "92%",
               "rows": [["dune_film.mp4", "1080p h264", "01:30", "● rendering 64%", "now"], ["dune_film_square.mp4", "1080x1080", "01:30", "queued", "-"],
                        ["dune_teaser.mp4", "1080p h264", "00:15", "✓ done", "4m ago"], ["dune_film_v1.mp4", "1080p h264", "01:28", "✓ done", "32m ago"]]}})

# ---------------------------------------------------------------- m15 grading
scen("m15", "grading", "dune_film.kimo", "codex", hms("00:14:02"),
  [["import_media", ["neon_walk.mov"], 400], ["trim", ["clip_07", {"out": "00:04.20"}], 100], ["analyze_frame", ["clip_07"], 600], ["match_shot", ["clip_07", {"ref": "clip_06"}], 900],
   ["set_white_balance", ["clip_07", "5600K"], 100], ["grade", ["clip_07", {"lift": "-0.02"}], 100], ["grade", ["clip_07", {"gamma": "+0.01"}], 100], ["grade", ["clip_07", {"sat": "1.10"}], 100], ["grade", ["clip_07", {"gain": "+0.06"}], 1200]],
  {"agent": {"footer": "following grade"},
   "viewer": {"src": F("m15"), "tags": [{"text": "source", "x": 8, "y": 8}, {"text": "kimo grade", "pos": "tr", "x": 8, "y": 8}], "tc": "01:00:42:19"},
   "grade": {"sub": "clip_07  neon_walk", "blocks": [K("Temperature", "5600K"), K("Tint", "-3", dim=True), K("Contrast", "1.14"), K("Pivot", "0.435"), K("Saturation", "1.10"), K("Gain", "+0.06", ch=True), K("Highlights", "-12"), K("Shadows", "+6"),
             S("Nodes", h=58), {"list": ["01  balance", "02  contrast", "03  look"], "sel": 2}, {"widget": "thumbs", "h": 104, "tgap": 9.5, "labH": 24, "style": "border:none;border-top:1px solid var(--line);border-radius:0;background:none;margin:12px -15px 0;padding:13px 0 0 10px", "items": [{"src": F("m15_t1"), "w": 69, "h": 56.4}, {"src": F("m15_t2"), "sel": True, "label": "clip_07", "w": 99.2, "h": 57.4}, {"src": F("m15_t3"), "w": 64, "h": 56.4}]}]},
   "bottom": {"parade": {"src": F("m15_parade")}, "wheels": [{"name": "Lift", "dot": [0, 0], "vals": ["-0.02", "0.00", "+0.03"]}, {"name": "Gamma", "dot": [0, 0], "vals": ["+0.01", "0.00", "-0.01"], "cols": [None, None, "#6f8fe0"]},
                         {"name": "Gain", "em": True, "dot": [0, 0], "ring": 70, "vals": ["+0.06", "+0.04", "+0.02"]}]}})

for s in SC:
    s["order"] = int(s["id"][1:])
SC.sort(key=lambda s: s["order"])
json.dump(SC, open(os.path.join(HERE, "scenarios.json"), "w"), indent=1)
print(f"wrote {len(SC)} scenarios")
