# Track diagnostics (MCP tools)

Native Rust port of the showcase film agent's track-analysis scripts. These tools DIAGNOSE a track. None of them certify it.

Workflow: `track_features` (raw tracks stored) -> `fit_track_homography` (soft-L1 robust fit + held-out residuals, optional `native_quads` compare) -> `decompose_track_homography`, `track_jitter_metrics`, `corner_disagreement`, `track_feature_residuals` -> `ecc_refine_track` (new fit) -> `lk_residual_audit` -> `render_error_field`. `track_set_info` and `fit_info` read back raw data.

## Honest limits (also in every tool description and result)
- Held-out residuals are measured on features from the same sequential tracker. They are not ground truth. Small residual does not mean accurate; drift or identity swaps can agree with the fit.
- Forward/back consistency does not eliminate identity drift on repeated windows. Smaller coplanar patches also failed late in the film agent's take, which points at drift/misidentity inside the sequential measurement. Use `track_set_info` and `track_feature_residuals` to inspect individual feature identities.
- Corner columns are native-vs-fit disagreement, not annotated truth.
- LK audit residuals are biased low (LK starts at the fit's prediction).
- ECC correlation is a photometric score, not accuracy.
- Frame 0 of a fit is pinned to identity (features are anchored to the first frame). Analysis runs at project resolution; coordinates are project pixels.

## Differences from the original scripts
Hardcoded ROIs, corners and paths are parameters (`roi`, `corners`, `out_dir`, `render_dir`). Features are detected with Kimo's Shi-Tomasi + pyramidal LK, not OpenCV's, so numbers are not bit-identical to the originals. The soft-L1 fit uses Levenberg-Marquardt with the scipy `soft_l1` loss definition.

## Validation
`cargo test -p kimo-core --lib trackmath`; `python3 tests/track_diag_gen_plate.py` then `python3 tests/track_diag_proof.py <dir>` (synthetic plate with known corners); `python3 tests/tool_fuzz.py` (248 tools, 0 failures).
