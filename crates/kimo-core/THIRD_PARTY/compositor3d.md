# compositor3d (src/comp3d.rs) — formula sources

Formulas only. No code was copied from any of these sources.

- **Thin-lens circle of confusion** (`coc_px`): CoC = A f |1/z_f - 1/z| (A = lens diameter, f = focal length, z_f = focus
  distance), the textbook thin-lens result, e.g. Potmesil & Chakravarty, "A Lens and Aperture Camera Model for Synthetic Image
  Generation", SIGGRAPH 1981. Public scientific formula; no licence applies.
- **Percentage-closer filtering / area-light soft shadows** (soft shadows by averaging visibility over samples of a light disc):
  Reeves, Salesin & Cook, "Rendering Antialiased Shadows with Depth Maps", SIGGRAPH 1987 (idea only; kimo tests visibility with
  exact ray-plane intersections instead of a depth map). No licence applies.
- **Vogel (golden-angle) disc sampling**: H. Vogel, "A better way to construct the sunflower head", Mathematical Biosciences 1979.
  r_k = sqrt((k + 0.5) / n), theta_k = k * pi (3 - sqrt 5). No licence applies.
- **Ray-plane intersection**: u = ((p0 - o) . n) / (d . n), standard analytic geometry. No licence applies.
- After Effects behaviour (Classic 3D grouping rule "a 2D layer breaks the 3D group", Casts Shadows default Off, Accepts Shadows
  default On) is matched from public Adobe documentation, behaviour only.
