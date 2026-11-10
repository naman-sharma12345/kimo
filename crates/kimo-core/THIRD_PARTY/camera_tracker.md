# 3D camera tracker (lane K5): formula sources

No third-party code was copied for the camera tracker. The formulas below come from published documentation or papers ("formula only").

- **Brown-Conrady lens distortion, OpenCV order** (`sfm::distort_norm`, `sfm::distort_px`, the tangential terms in `sfm::bundle_adjust`,
  `lens_undistort` / `lens_redistort`, `export_st_map`): OpenCV calib3d documentation (Apache-2.0 project), "Camera Calibration and 3D
  Reconstruction", formula only. With normalised x, y and r2 = x^2 + y^2:
  x_d = x (1 + k1 r2 + k2 r2^2) + 2 p1 x y + p2 (r2 + 2 x^2),
  y_d = y (1 + k1 r2 + k2 r2^2) + p1 (r2 + 2 y^2) + 2 p2 x y.
  The inverse (`sfm::norm_obs` with p1/p2 set) is our own fixed-point iteration plus Newton steps on that formula; OpenCV's
  undistortPoints code was not used.
- **Least-squares plane / ground-plane similarity** (`sfm::fit_plane`, `sfm::ground_plane`, `sfm::apply_similarity`): textbook total least
  squares (smallest right singular vector of the centred points) and the Rodrigues rotation between two unit vectors. Our own derivation.
- **Umeyama similarity alignment** (tests/camera_tracker.rs only): S. Umeyama, "Least-squares estimation of transformation parameters between
  two point patterns", IEEE TPAMI 13(4), 1991, formula only (the same closed form tests/sfm_synthetic.rs already uses).
- **ST map convention** (`export_st_map`): the Foundry Nuke STMap node documentation, convention only: R = s = x / width, G = t = y / height
  measured from the bottom, pixel centres at +0.5.
