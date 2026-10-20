# particles3d.rs (Source::Particles3D, lane K4): provenance

No third-party code was copied into `src/particles3d.rs` or `src/ext/particles3d_tools.rs`. Everything is an original
implementation in kimo's own style. The formulas and where they come from:

| What | Source | How it was used |
| --- | --- | --- |
| Closed-form motion under constant gravity `g` and linear drag `k` (`dv/dt = g - k v`): `v(t) = g/k + (v0 - g/k) e^(-kt)`, `x(t) = x0 + (g/k) t + (v0 - g/k)(1 - e^(-kt))/k`; `k = 0`: `x0 + v0 t + g t^2/2` | Textbook first-order linear ODE solution (public-domain mathematics) | Idea only. The task spec states the formula. Evaluated with `expm1` plus a short series when `k t < 1e-4`, to avoid cancellation. |
| Emitter shapes, spread cone, per-particle hash seeding, colour/size/opacity over life, additive vs depth-sorted blending | General particle-system practice, as in bevy_hanabi (MIT/Apache-2.0) and Godot `GPUParticles3D` / `ParticleProcessMaterial` (MIT) | Idea only. No code, shader or data from either project. |
| Uniform direction in a spherical cap (`cos θ = 1 - u (1 - cos spread)`) and uniform point in a ball (`r = R u^(1/3)`) | Standard sampling identities (public-domain mathematics) | Idea only. |
| Radial gaussian splat kernel `(e^(-h d²) - e^(-h)) / (1 - e^(-h))` | kimo's own `particle_flow.rs` (same crate, MIT) | Same formula, re-implemented. |
| Light key interpolation (sorted keys, smoothstep, missing pos = previous) | kimo's own `lighting::eval_light` (same crate, MIT) | Re-implemented without allocation, so the per-particle loop allocates nothing. |
| Splitmix64-style finaliser for `(seed, i, k)` hashing | kimo's own `particles.rs` hash (same crate, MIT). The constants are the public-domain SplitMix64 ones. | Same mixing steps. |

Licence of this code: MIT, like the rest of kimo-core.
