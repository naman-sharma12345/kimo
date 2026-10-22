# Director eval

A fixed, repeatable test of how well a model edits a short through kimo-mcp. Three footage sets, one brief each, scored by
measurement (no model in the loop) plus one vision judge on a contact sheet of the result.

    python3 gen_footage.py OUT            # deterministic footage: OUT/talk, OUT/cafe, OUT/vlog, each with truth.json
    FOOTAGE=OUT SCORE_KIMO=/path/kimo-mcp python3 run_eval.py plan.json results/

Model credentials come from the environment: the Anthropic base URL and API key for the judge and Anthropic models,
the OpenAI-compatible base URL and API key for other hosts (see the variable names at the top of agent.py).

A plan is a JSON list of runs: `{"name", "model", "adapter": "anthropic"|"openai", "set": "talk"|"cafe"|"vlog", "kimo": "/path/kimo-mcp",
"tools": "grouped"|"director"|..., "docs": ["docs/DIRECTOR_QUICKSTART.md"]}`. `agent.py` is the director loop (one model, kimo over stdio,
tool results capped at 12,000 chars, the two newest images kept); `score.py DIR SET KIMO` scores one finished run.

The sets (`truth.json` holds what the scorer checks against):

- talk: a talking head in two takes (the first flubbed), a shaky street shot, a desk shot, a dark blue street pan. 15-20 s short with b-roll over speech, captions, title.
- cafe: no speech. Six product shots (one shaky, one dark, one over-exposed) and a 120 BPM track. 15 s music reel cut on the beat with text.
- vlog: two speakers (one with a false start, one quieter), a shaky walk, a food shot, a dark outro. 20-30 s.

Measured checks (each 0..1; composite = mean x 100; an unusable export scores 0): finished (right size, has sound), duration,
exposure (no black or blown frames), loudness (-14 +-1 LUFS), peak (<= -1 dBFS), b-roll only during speech, best take used, dead air,
caption sync and words, cuts on pauses or beats, speaker balance, shots used, text present, stabilized (jitter vs the raw shot),
grade (luma and cast of the defect shots), text inside the platform safe area (kimo's own compute_text_bounds vs TikTok's UI zones).
The judge (an Anthropic vision model) scores a 12-frame contact sheet 1-10 and lists issues.

## Director mode and scoring (added for small models)

- `director_mode.md`: the step guide given to the model with `--tools director --allow ...`. `{footage_list}`, `{footage}` and `{out}` are filled in by agent.py.
- `score_v2.py`: anchored judge (16-frame sheets, a raw-concat anchor and a reference edit, two samples).
- `check_export.py`: duration, codec, black share and frozen share of an export.
- `make_pair.py`: labelled side-by-side of two exports.
- agent.py autosaves the project after each change and reloads it if the Kimo server restarts.
- `--fonts DIR` sets KIMO_FONTS.
