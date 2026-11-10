// Pure clipboard helper for kimo gui (v7.5): copy/paste of one or more clips, and the op-list for a
// ripple-delete. No DOM; served at GET /gui_clipboard.js so the browser and tests/js/clipboard_test.js
// (Node, headless) run the exact same bytes.
//
// copySelection snapshots {id, start} for the clips selected AT COPY TIME, not a live reference -- a
// paste later still works even if the original clip moved, was trimmed, or the selection changed, because
// paste only ever reads duplicate_clip (server-side: "copy this clip id, including every effect, keyframe
// and mask, to a new start") -- nothing about a clip's effects needs to travel through the clipboard
// itself. buildPasteOps keeps the RELATIVE arrangement of a multi-clip copy: the earliest-starting copied
// clip lands exactly at pasteAtTime, and every other copied clip keeps its original offset from that one,
// so copying two clips 2s apart and pasting pastes them 2s apart again, not stacked on the playhead.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoClipboard = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  function copySelection(clips) {
    return (clips || []).map((c) => ({ id: c.id, start: c.start }));
  }
  function round4(x) { return Math.round(x * 10000) / 10000; }

  // clipboard: [{id, start}] from copySelection. pasteAtTime: timeline seconds the earliest copied clip
  // should land at. Returns the exact `batch` op list (one atomic undo step for the whole paste, not one
  // per clip) -- duplicate_clip already carries every effect/keyframe/mask, so each op only needs the
  // source clip id and its new start; track_id is left unset so each copy lands on its own clip's own
  // track, same as a plain duplicate_clip call would.
  function buildPasteOps(clipboard, pasteAtTime) {
    if (!clipboard || clipboard.length === 0) return [];
    const earliest = Math.min.apply(null, clipboard.map((c) => c.start));
    return clipboard.map((c) => ({ tool: 'duplicate_clip', args: { clip_id: c.id, start: round4(Math.max(0, pasteAtTime + (c.start - earliest))) } }));
  }

  // Single-clip only (see gui_multiselect.js's own group-move/delete for the multi-clip case; ripple
  // across several arbitrarily-selected clips at once has no settled meaning yet, same honest limit this
  // codebase already states for group trim).
  function buildRippleDeleteOp(clipId) { return { tool: 'ripple_delete', args: { clip_id: clipId } }; }

  return { copySelection: copySelection, buildPasteOps: buildPasteOps, buildRippleDeleteOp: buildRippleDeleteOp };
});
