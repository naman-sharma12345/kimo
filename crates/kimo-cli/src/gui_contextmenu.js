// Pure clip context-menu helper for kimo gui (v7.6): right-click a clip bar and get Split/Duplicate/
// Delete/Properties as a visible menu, instead of only the keyboard shortcuts (S/Delete) and the
// inspector-click gui_index.html already has. No DOM -- gui_index.html's oncontextmenu handler and
// tests/js/contextmenu_test.js (Node, headless) run the exact same bytes, served at GET /gui_contextmenu.js.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoContextMenu = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  function round4(x) { return Math.round(x * 10000) / 10000; }

  // clip: {id, start, duration, end, ...} from timeline_view_model. playheadTime: the GUI's current t.
  // Split mirrors the server's own rule (split_clip errors "split point outside clip" at or past either
  // edge) so the menu item is shown disabled -- with why -- rather than enabled and failing on click.
  function buildMenuItems(clip, playheadTime) {
    if (!clip) return [];
    const end = clip.end != null ? clip.end : clip.start + clip.duration;
    const splitOk = playheadTime > clip.start && playheadTime < end;
    return [
      { id: 'split', label: 'Split at Playhead', enabled: splitOk, reason: splitOk ? null : 'move the playhead inside this clip first' },
      { id: 'duplicate', label: 'Duplicate', enabled: true, reason: null },
      { id: 'delete', label: 'Delete', enabled: true, reason: null },
      { id: 'properties', label: 'Properties', enabled: true, reason: null },
    ];
  }

  // The exact dispatch for one menu item: {tool, args} for the three mutating actions (split_clip,
  // duplicate_clip, delete_clip -- the same tools the keyboard shortcuts and inspector already call), or
  // {open: 'properties', clip_id} for Properties, which opens the existing inspector panel rather than
  // calling a tool. Duplicate lands the copy immediately after the original on the same clip's own track
  // (track_id omitted, same convention gui_clipboard.js's paste already uses) -- a direct "duplicate this
  // one clip in place" path that, before this, only existed indirectly via copy-then-paste.
  function actionFor(itemId, clip, playheadTime) {
    if (!clip) return null;
    const end = clip.end != null ? clip.end : clip.start + clip.duration;
    if (itemId === 'split') return { tool: 'split_clip', args: { clip_id: clip.id, at: playheadTime } };
    if (itemId === 'duplicate') return { tool: 'duplicate_clip', args: { clip_id: clip.id, start: round4(end) } };
    if (itemId === 'delete') return { tool: 'delete_clip', args: { clip_id: clip.id } };
    if (itemId === 'properties') return { open: 'properties', clip_id: clip.id };
    return null;
  }

  // Clamps a menu of (w, h) opening at (x, y) so it never renders off the right/bottom edge of a
  // viewport (vw, vh) -- a plain right-click near the window's edge should not lose part of the menu.
  function menuPosition(x, y, vw, vh, w, h) {
    return { left: Math.max(0, Math.min(x, vw - w)), top: Math.max(0, Math.min(y, vh - h)) };
  }

  return { buildMenuItems: buildMenuItems, actionFor: actionFor, menuPosition: menuPosition };
});
