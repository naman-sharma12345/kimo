# Deep glow / lens flare v2 / linear light wrap: upstream notice

No upstream code was copied into `src/glow.rs`, `src/flare.rs` or `src/lightwrap.rs`. The formulas below were written from scratch; the projects
listed were used as references for the idea only.

- Godot Engine glow (MIT), https://github.com/godotengine/godot (servers/rendering, glow/bloom shaders): idea only. Summing several blur levels
  with per-level weights and a soft-knee HDR threshold.
- Filament bloom and lens flare (Apache-2.0), https://github.com/google/filament (filament/src/materials/bloom, flare): idea only. Soft-knee
  bright pass ahead of a multi-level blur; flare ghosts placed along the source -> screen-centre axis.
- three.js Lensflare (MIT), https://github.com/mrdoob/three.js/blob/dev/examples/jsm/objects/Lensflare.js: idea only. Flare elements positioned at
  a distance along the light -> centre line, and an occlusion test that scales the flare by how much of the source is visible.
- sRGB transfer function: IEC 61966-2-1 (public standard), piecewise 12.92 / 2.4 power curve.
- Exponential falloff as a Gaussian mixture: the subordination identity exp(-r) / (2 pi) = int sqrt(2/pi) s^3 exp(-s^2/2) N(r; s) d ln s, N the
  normalised 2D Gaussian (standard mathematics), sampled at octave steps. Not taken from any codebase.
