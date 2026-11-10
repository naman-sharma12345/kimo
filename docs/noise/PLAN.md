# Coherent noise unit 1 (WIP)

Base: current main `2977b73751f1c9124501503e3cba0b9c33cc9413`, checked 2026-10-14. Branch `instinct/wip-noise`. V4 only, no PR/merge into main without Naman's explicit word. Main is locked to v3 plus release fixes.

Implemented but NOT built/executed/visually signed off: raw 2D Perlin + 2D simplex, analytic derivatives, MCP `sample_noise`, animatable `coherent_noise` effect. No preset stacks. No framebuffer noise replaces existing hash-noise semantics. Worley, FBM, domain warp and curl are the next distinct units in that order, not silently bundled here. No camera/projection work before noise visual sign-off.

## Original derivation and provenance

No third-party source, constants tables, gradient tables or permutation tables copied. Lattice gradients are unit vectors from an original FNV-based coordinate hash and angle map. Perlin is the tensor interpolation of four gradient-dot-offset terms with the quintic smoothstep obtained from endpoint value/first/second derivative constraints. Differentiate interpolation weights and dot products analytically.

Simplex uses an equilateral triangular lattice: skew `(sqrt(3)-1)/2`, inverse skew `(1-1/sqrt(3))/2`; choose triangular middle corner by local x>y, sum compact radial kernels `4*(max(0, .5-r.r))^4 * dot(gradient,r)`. Derivative per corner is `4*(k^4*g - 8*k^3*r*dot(g,r))`. Scale 4 is this implementation's explicit low-amplitude convention, NOT empirical normalization to unit range. Output is raw signed; gain/bias are separate explicit parameters. No hidden normalization/clamping.

Conservative bounds: Perlin abs(value)<=sqrt(2), since convex weights combine unit-gradient dot offsets of length<=sqrt(2). Simplex each active corner has |r|<1/sqrt(2), k<=1/2; three corners give abs(value)<=3/(4sqrt(2)). Perlin component gradient can be bounded by 4*(1+1.875sqrt(2))<15; simplex by 3*4*((.5)^4+8*(.5)^3*.5)<7. A conservative public component bound of 32 covers both. Bounds are intentionally loose, not claims of attained extrema or normalized output. Same-build determinism; no claim of bit-for-bit sin/cos identity across platforms.

Prior exposure: the survey author inspected permissively licensed FastNoiseLite/Ashima/noise-rs source spans before this assignment. This is original handwritten code with that exposure disclosed, not certified clean-room. No restrictive implementation source was read. No source library dependency was added. Original math still requires independent tests/review; familiarity with an algorithm is not proof.

## Simplex patent/license history

Fetched sources:
- https://patents.google.com/patent/US6867776B2/en : displays US6867776B2 "Expired - Lifetime", anticipated expiration 2022-01-08. The site explicitly says legal status is an assumption, not a legal conclusion.
- https://en.wikipedia.org/wiki/Simplex_noise : describes coordinate skew, simplex subdivision, gradient selection and kernel summation; notes the historical US patent for certain 3D+ textured-image implementations and expiry 2022-01-08.

This unit is 2D only. No claim of universal patent clearance or legal opinion follows from those pages. Patent rights and copyright are distinct; expiry does not authorize copying source. Existing library licenses were not used as permission to copy their implementations. The project license governs newly authored source; this doc preserves provenance and known uncertainty.

## Rendering and resource contract

Pixel-center coordinates normalized by frame HEIGHT preserve aspect, then frequency/offset/rate transform. Time is clip-local seconds. Gradients are with respect to field coordinates, not pixels or time. Point sampled: no band-limited antialias guarantee. Signed raw output, alpha preserved, amount=0 is exact identity. Explicit decimal-u64 seed in effect string params avoids lossy f64 seeds; sample command takes JSON integer u64. Inputs finite with +/-1e9 coordinate computational limit and work/time budgets. No artistic frequency clamps.

Numeric tests are first. 1080p render/contact-sheet runner follows only after numeric PASS. Saved exact 1080p source frames and 1:1 crops must be inspected alongside the resized contact sheet. Human visual sign-off is separate from numeric/byte-repeat tests. No "100% working" or premium claim.
