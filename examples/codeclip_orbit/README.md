# codeclip_orbit: example kimo code clip

A web project Kimo renders frame by frame (`add_code_clip`). `index.html` loads `scene.js`, which defines
`window.renderFrame(t, params)`: a lit torus knot inside a ring of cubes with an orbiting camera, on a transparent
background. Params (declared in `kimo.json`, keyframeable with `set_code_clip_param`): `hue` (0..1), `speed` (0..4),
`orbit` (radians).

Rules for any code clip: draw only from `t` and `params` (no clock, no unseeded randomness, no state carried between
frames), keep every file inside the folder (the page cannot reach the network), and set `window.__ready = true` once
loaded. `check_code_clip_determinism` verifies the first rule.

Licence: the example code is MIT (kimo). `vendor/three/` is three.js r186, MIT, Copyright 2010-2026 three.js authors
(see `vendor/three/LICENSE`).
