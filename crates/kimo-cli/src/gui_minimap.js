// Pure timeline minimap/overview helper for kimo gui (v7.6): the main timeline only ever shows whatever
// window the current zoom and scroll position happen to fit; there is no at-a-glance view of the WHOLE
// project, and no fast way to jump far away without a lot of scrolling. This is the view-model for a
// fixed-width strip showing every clip across the whole duration, with a rectangle over the part the main
// timeline currently shows, click-to-jump. No DOM; served at GET /gui_minimap.js so the browser and
// tests/js/minimap_test.js (Node) run the exact same bytes.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoMinimap = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  const MIN_DURATION = 1; // a project with (near) zero duration still gets a sane, non-infinite scale

  // px/sec that fits `duration` seconds into exactly `widthPx` pixels -- independent of the main
  // timeline's own zoom (kimoZoom), which is what makes this an OVERVIEW rather than just another zoom
  // level.
  function scaleForWidth(duration, widthPx) {
    const d = Math.max(MIN_DURATION, duration || 0);
    return widthPx / d;
  }

  // timeline_view_model's own tracks/clips -> one block per clip in minimap pixel space, bottom-to-top
  // track order preserved (same order the view-model already lists tracks in) so the strip reads as a
  // squashed version of the real timeline, not a shuffled one.
  function buildBlocks(tl, widthPx) {
    const scale = scaleForWidth(tl && tl.duration, widthPx);
    const blocks = [];
    const tracks = (tl && tl.tracks) || [];
    for (let i = 0; i < tracks.length; i++) {
      for (const c of (tracks[i].clips || [])) {
        blocks.push({ trackIndex: i, clipId: c.id, left: c.start * scale, width: Math.max(1, c.duration * scale) });
      }
    }
    return blocks;
  }

  // The highlighted rectangle over the part of the whole duration the main (scrolled, zoomed) timeline is
  // currently showing: scrollLeftSec is where that view starts, visibleWidthSec how much of it is shown at
  // once (both already in seconds, so the minimap's own scale is the only pixel math needed here).
  // Clamped to the strip's own bounds so a view that has scrolled past the end never draws off the strip.
  function viewportRect(duration, widthPx, scrollLeftSec, visibleWidthSec) {
    const scale = scaleForWidth(duration, widthPx);
    const left = Math.max(0, Math.min(widthPx, scrollLeftSec * scale));
    const rawRight = Math.max(0, Math.min(widthPx, (scrollLeftSec + Math.max(0, visibleWidthSec)) * scale));
    return { left: left, width: Math.max(1, rawRight - left) };
  }

  // A click at clickX (minimap-local pixels, 0 at the strip's own left edge) -> the timeline second to
  // jump to, clamped to [0, duration] so a click past either edge still lands somewhere real.
  function timeFromClick(clickX, widthPx, duration) {
    const scale = scaleForWidth(duration, widthPx);
    const d = Math.max(0, duration || 0);
    return Math.max(0, Math.min(d, clickX / scale));
  }

  return { scaleForWidth: scaleForWidth, buildBlocks: buildBlocks, viewportRect: viewportRect, timeFromClick: timeFromClick };
});
