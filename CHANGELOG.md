# Changelog

## Unreleased (v6 "eyes")

- New tool `flash_series` (group `scopes`, read-only): per-frame linear relative luminance (sRGB decode, Rec.709 Y), red saturation R/(R+G+B) and red quantity, luminance and red transitions, changed-area fractions, and per-window transition counts with window start times. Window length and thresholds are arguments; raw numbers only, no pass/fail. Harding / ITU-R BT.1702 / WCAG reference values are in the tool description only. Tools: 376 → 377 registered, flat `tools/list` 378 → 379.
  Next (documented, not implemented): the WCAG 2.2 red-flash rule (a 0.2 change in CIE 1976 u′v′); only the WCAG 2.0 (R−G−B)×320 red measure is computed today.
  Per-window flash counts come two ways: `lum_flashes` / `red_flashes` count a flash in every window it touches, `lum_flashes_fully_inside` / `red_flashes_fully_inside` only where both of its transitions fall inside the window. `params_used.max_resolvable_hz` reports fps / step_frames / 2. Next (not in this release): an optional `min_area_fraction` argument so a flash over a small part of the frame can be left out of the counts; today `changed_area_fraction` is reported but does not feed the counts.
