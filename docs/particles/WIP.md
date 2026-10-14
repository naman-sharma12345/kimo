# Raw fixed-step particle unit (WIP)

Base 2977b73, instinct/wip-particles. Original Rust/test draft, no cargo, no tests executed, no renders/warning check. No main merge/PR.

particle_simulate_raw exposes single 3D particle/sphere state, exact fixed dt, raw acceleration, linear drag, softened inverse-square attractor and explicit force vector; swept sphere vs unit-normal planes prevents discrete endpoint tunneling under held step velocity. Bounded collision events error rather than silently fall through. Numeric test sources cover semi-implicit ballistic oracle/convergence, fast swept collision/restitution, deterministic repeat, mass/force contract, drag and invalid input.

Scope limits: NOT an emitter array or renderer adapter, no birth/lifetime/trails/cache integration, no particle-particle sweep, arbitrary sampled spatial force field or curl-field binding yet. VectorField here is a constant supplied force, not a procedural field sampler. Do not claim the requested full field-driven emitter engine is done. Semi-implicit integration is first order; high drag or attractor stiffness requires smaller fixed dt, and there is no adaptive stability guarantee. Simultaneous multi-plane corners and incompatible penetrating planes need deeper solver work. Raw force vector is force/mass; acceleration and attractor are acceleration conventions. All units must be agreed before production binding.

Next adapter: multiple initial states and birth/lifetime schedule; sampled vector/curl potential at particle positions; deterministic trajectory/checkpoint cache keyed by forces/seed/colliders; sprite/trail renderer consuming states; sphere/sphere sweep; actual 1080p temporal proof. No claim of fluid/cloth/general rigid bodies.

Serial test commands after parent provides window:

```sh
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test particle_fields --test registry_lint --test registry_unique --test ext_registry -- --test-threads=1
```

Compilation, independent numeric review, zero new warnings and actual 1080p collision/force-field renders are pending. This unit provides no renderer proof. Golden baselines unchanged and never auto-blessed.
