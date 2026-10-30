// Pure timeline-zoom helper for kimo gui. No DOM: bounds-clamping and the x2 / x0.5 step is pure math,
// so it's tested headless the same way gui_keymap.js and gui_snap.js are. Served at GET /gui_zoom.js (same
// bytes a browser <script src> and a Node test both run).
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoZoom = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  var MIN_PX_PER_SEC = 4;
  var MAX_PX_PER_SEC = 400;
  var DEFAULT_PX_PER_SEC = 40;

  function clampZoom(v) {
    return Math.max(MIN_PX_PER_SEC, Math.min(MAX_PX_PER_SEC, v));
  }

  // The next zoom level after multiplying the current one by `factor` (2 for "+", 0.5 for the "-" button),
  // rounded to a whole pixel-per-second and clamped to [MIN_PX_PER_SEC, MAX_PX_PER_SEC].
  function zoomBy(current, factor) {
    return clampZoom(Math.round(current * factor));
  }

  return { MIN_PX_PER_SEC: MIN_PX_PER_SEC, MAX_PX_PER_SEC: MAX_PX_PER_SEC, DEFAULT_PX_PER_SEC: DEFAULT_PX_PER_SEC, clampZoom: clampZoom, zoomBy: zoomBy };
});
