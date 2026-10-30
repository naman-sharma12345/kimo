// Pure multi-select helper for kimo gui (v7.4): selection-set toggling, group-move delta clamping (so a
// dragged group can never push any of its members to a negative start), and the batch-op lists for a group
// move/delete -- both built as ONE atomic `batch` call (one undo step for the whole group action, not one
// per clip) rather than a loop of individual move_clip/delete_clip calls. No DOM; served at
// GET /gui_multiselect.js so the browser and tests/js/multiselect_test.js (Node, headless) run the exact
// same bytes.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoMultiselect = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  // A plain click (additive=false) replaces the whole selection with just this clip (or clears it, if this
  // clip was already the sole selection -- a second plain click on an already-selected single clip
  // deselects it, matching "click empty space to deselect" without needing empty space). Shift/Ctrl/Cmd
  // (additive=true) toggles this clip's membership, leaving everything else selected as it was.
  function toggleSelect(selected, clipId, additive) {
    selected = selected || [];
    if (!additive) { return (selected.length === 1 && selected[0] === clipId) ? [] : [clipId]; }
    return selected.includes(clipId) ? selected.filter((id) => id !== clipId) : selected.concat([clipId]);
  }
  function isSelected(selected, clipId) { return !!(selected && selected.includes(clipId)); }

  // The largest negative shift allowed is bounded by the earliest-starting clip in the group; a positive
  // or zero delta never needs clamping (nothing is trying to move left of 0).
  function clampGroupDelta(starts, deltaSec) {
    if (!starts || starts.length === 0 || deltaSec >= 0) return deltaSec;
    const earliest = Math.min.apply(null, starts);
    return Math.max(deltaSec, -earliest);
  }

  // clips: [{id, start}], already filtered to the selected ones. Rounds to 1e-4s (matches the precision a
  // drag's own pixel math already lives at) so a batch call never carries binary-float noise.
  function round4(x) { return Math.round(x * 10000) / 10000; }
  function buildGroupMoveOps(clips, deltaSec) {
    return clips.map((c) => ({ tool: 'move_clip', args: { clip_id: c.id, start: round4(Math.max(0, c.start + deltaSec)) } }));
  }
  function buildGroupDeleteOps(clipIds) { return clipIds.map((id) => ({ tool: 'delete_clip', args: { clip_id: id } })); }

  return { toggleSelect: toggleSelect, isSelected: isSelected, clampGroupDelta: clampGroupDelta, buildGroupMoveOps: buildGroupMoveOps, buildGroupDeleteOps: buildGroupDeleteOps };
});
