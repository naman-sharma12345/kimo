# Director layer quick start

Load `SKILL.md` in the AI client that drives Kimo. Supply a brief, song or footage,
then request a plan conforming to `contract.schema.json`. The engine itself does
not load this directory and its MCP tool list is unchanged.

1. Build the global music/visual contract before local shot prompts.
2. Keep scene timeline times absolute, shot times scene-local, and engine
   animation keyframe times clip-local. These are three different clocks.
3. Validate a real plan with `python3 skills/director/validate_plan.py plan.json`.
4. Review warnings, compare against actual audio, and inspect actual frames.
5. With completed media present, optionally emit the explicit assembly commands:
   `python3 skills/director/validate_plan.py plan.json --batch --media-root /absolute/project`.
6. Bind IDs from each actual Kimo result; probe media and only then execute approved
   primitives through the connected server's advertised command wrapper.

Tests: `python3 -m unittest discover -s skills/director -p 'test_*.py'`.
Python 3.9+; standard library only, no cargo, GPU or generation provider required.
`example.plan.json` is a synthetic 24-second planning fixture, with no audio or media.

## Contract conventions

- `schema_version`: `kimo-director-1` or `kimo-director-2`. Unknown fields are rejected to catch typos.
- Project: duration/fps/canvas/audio master and measured/supplied/estimated timing.
- Visual contract: concrete palette, lighting, optics, materials, physics, continuity.
- Scenes: zero-based IDs, gapless absolute start/end, music role/energy/lyrics,
  purpose, coverage, performance mode, continuity anchors and unique output path.
- Still and motion prompts are separate nonempty strings. A validator cannot prove
  natural-language coherence; the directing/review pass owns that judgment.
- Shots: gapless local start/end, framing/orientation/movement enums, one action,
  cut motivation, boundary states, camera side, focus/light and vocal geometry.
- Duration exceptions, repetition and discontinuity can be explained, not hidden.
- Capability source is required for a verified generation claim. For an unknown
  generator, keep a single shot and verify official capability before execution.
- Generation-scene duration and editing shot duration are different concepts.
  A provider's single-shot constraint must not be mistaken for the editor's
  ability to cut completed media. Multiple generation shots require verified
  support here; ordinary Kimo timeline edits are not constrained to 10-15 seconds.
- Validation is structural/timeline hygiene, not a guarantee of realism, identity,
  lip-sync accuracy, source permissions, media existence, or cinematic taste.

See `mcp-map.md` for verified engine names and limits and `CREDITS.md` for sources.

## Second-pass contracts

`kimo-director-2` requires progression per scene and lighting/optics/movement,
density and attention per shot. Use `example.v2.plan.json` as the synthetic fixture.
Progression distinguishes narrative, performance and abstract plans. Causal or
emotional repetition and constant high density raise review warnings, not forced
creative changes. References declare transfer and exclusions; duplicate IDs fail.

`progression.md` covers entering/leaving emotion, cause/action/consequence, motif
payoff, action settling time, contrast and source conflicts. Numeric optics are
planning values, not automatically executable camera commands. Source capabilities
are not promoted into facts, and references are never uploaded by the validator.

An optional plan-wide `coverage_policy` declares the desired number of distinct
opening compositions and maximum identical-opening run, with a rationale.
Violations produce review warnings, never forced changes. Optional shot-level
`lens_rationale` and `light_change_reason` connect physical choices to story and
continuity rather than treating parameters as decoration.
