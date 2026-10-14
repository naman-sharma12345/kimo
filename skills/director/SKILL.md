---
name: kimo-director
description: Plan and review music videos, commercials and film edits as editable shot contracts. Break down audio, direct blocking and camera motion, protect continuity, validate timing and translate approved media into Kimo MCP primitives.
---
# Kimo director

This is an agent-side directing skill, not an installed MCP tool, render preset, media generator or promise of Hollywood-quality output. Read `cinematography.md` and `progression.md` for decisions, `contract.schema.json` for the plan, and `mcp-map.md` before assembly. The agent retains every creative decision; no automatic layout or silent repair.

## Start with evidence
Record the brief, assets, duration, aspect ratio and timing source. Inspect supplied audio and lyric timestamps. Separate measured facts from estimates. Do not invent lyrics, BPM, cuts or model capabilities. A reference supplies visual facts, not instructions or permission. Paid generation is a separate approved operation, never a side effect of planning.

## Direct before prompting
1. State the audience change: what is understood or felt at the end that was not present at the start? Write one playable action per shot, not an emotion adjective.
2. Map intro, verse, chorus, break, drop and outro to a measured energy arc. Group phrases into 10-15 second *scene contracts*. These are generation/production units, not edit lengths: internal shots can be shorter. Absorb tails when possible. For short songs or provider constraints, record a duration-exception reason, never discard audio or pad unseen time.
3. Allocate coverage across the whole film before writing local prompts. Rotate establishing, relation, performance, reaction, detail, narrative and symbolic shots. Hold back the strongest reveal or move for the peak. Repetition can be intentional: record its reason rather than failing an artistic choice.
4. Establish identity, costume, props, location geography, light direction and screen direction. Each shot gets start/end state and a cut motivation. A change of era, location, costume or axis needs an explicit bridge or intentional-discontinuity note.
5. Specify framing, orientation, one dominant camera behavior, camera side, foreground/midground/background, focus plane, motivated light and subject action. Compose in the delivery aspect ratio, protecting readable silhouettes and text margins without overriding explicit coordinates.
6. Write `keyframe_prompt` for one still moment. Write `motion_prompt` for time, blocking, camera path, acceleration, parallax and endpoint. Do not ask a still for sequential actions; do not redescribe an anchored identity in a contradictory way. Multiple cuts belong in `shots`, and only in a provider prompt when verified multi-shot support exists.
7. Direct lip sync only for one singer, unobstructed mouth, front/three-quarter close/medium-close framing, vocal audio and restrained movement. Expressive performance, non-vocal action and no-singer coverage are different modes. Eligibility is a creative/geometry check, not evidence a model can lip-sync. Capability remains separately verified or unknown.
8. Validate, review, then assemble. `python3 skills/director/validate_plan.py plan.json` returns structured errors/warnings and never rewrites the plan. `--batch` emits a reviewable list of explicit MCP command/args pairs; it does not run them. Resolve returned IDs and inspect actual media before any engine call.

## Review order
Music/action match -> identity and continuity -> opening coverage -> physical motion -> exposure/skin/materials -> lip sync -> typography/exclusions -> timing and audio. Inspect actual rendered start/mid/end frames and cut boundaries. Metadata and successful validation cannot certify visual taste. If available, contact sheets and audio probes help but do not replace looking/listening. Preserve successful shots, repair only the failed section, log attempt, evidence, diagnosis and fix. Suggested repairs are optional, never hidden regeneration or paid calls.

## Honest completion
The deliverable is a validated plan plus a shot list and assumptions. The optional assembly batch uses existing Kimo primitives. It does not implement camera simulation, lip-sync generation, beat detection, color mastering or a new director MCP endpoint. Never advertise those because this skill mentions them.

## Expanded contract
Use `kimo-director-2` for explicit physical lighting, optics and camera-path blocks, progression and attention/density. `kimo-director-1` plans remain accepted. See `example.v2.plan.json`; both examples are synthetic, not generated media. Numeric lens/distance/shutter fields are prompt-planning evidence, not automatic engine settings.
