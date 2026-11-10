// Pure marker/chapter list helper for kimo gui (v7.5): markers already render as ticks on the timeline
// ruler (gui_index.html's own renderTimeline); this is the view-model for a scrollable LIST panel beside
// it -- the same markers list_markers already returns, sorted by time (same order timeline_view_model's
// own `markers` field already carries), formatted for display and click-to-seek. No DOM; served at
// GET /gui_markerpanel.js so the browser and tests/js/markerpanel_test.js (Node) run the exact same bytes.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoMarkerPanel = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  // "1:03.40" for anything under an hour, "1:01:03.40" past it -- a timeline-duration label, not a clock.
  function formatTime(seconds) {
    const s = Math.max(0, seconds || 0);
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const rem = s - h * 3600 - m * 60;
    const secStr = rem.toFixed(2).padStart(5, '0');
    if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${secStr}`;
    return `${m}:${secStr}`;
  }

  // markers: timeline_view_model's own `markers` array ({id, time, duration, name, color}), already sorted
  // by time. Returns one row per marker ready to render: a display label (name, or a generic placeholder so
  // an unnamed marker is never a blank row), the formatted time, and the raw time to seek to on click.
  function buildMarkerRows(markers) {
    return (markers || []).map((m) => ({
      id: m.id,
      label: (m.name && m.name.trim()) ? m.name : '(marker)',
      time: m.time,
      timeLabel: formatTime(m.time),
      color: m.color || 'gold',
    }));
  }

  return { formatTime: formatTime, buildMarkerRows: buildMarkerRows };
});
