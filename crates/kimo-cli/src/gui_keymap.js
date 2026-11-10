// Pure keyboard-shortcut resolver for kimo gui. No DOM, no globals: given a key-event-shaped object and
// the tag name focus is currently on, returns one action name or null. This is the ONE mapping kimo gui
// uses -- gui_index.html's keydown listener calls this exact function (served at GET /gui_keymap.js, same
// bytes a Node test requires directly), so there is nothing to keep in sync between "what the browser does"
// and "what is tested". Editing while focus is in a text field never fires a shortcut (space in a filter
// box must type a space, not toggle play).
//
// v7.5: Ctrl/Cmd+C copies the current selection, Ctrl/Cmd+V pastes it at the playhead (gui_clipboard.js
// builds the duplicate_clip ops); Shift+Delete/Backspace ripple-deletes the selected clip (closes the gap)
// instead of plain delete_clip (leaves one) -- a second, deliberately different key so an accidental plain
// Delete never ripples clips the user did not mean to shift.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoKeymap = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  var EDITABLE_TAGS = { INPUT: true, TEXTAREA: true, SELECT: true };

  // ev: {key, ctrlKey, metaKey, shiftKey, altKey, target: {tagName}} (a real KeyboardEvent duck-types this).
  function resolveKeyAction(ev) {
    var targetTag = (ev.target && ev.target.tagName) || '';
    if (EDITABLE_TAGS[targetTag]) return null;
    var key = ev.key;
    var ctrlOrMeta = !!(ev.ctrlKey || ev.metaKey);
    if (ev.altKey) return null;
    if (ctrlOrMeta) {
      if (key === 'z' || key === 'Z') return ev.shiftKey ? 'redo' : 'undo';
      if (key === 'c' || key === 'C') return 'copy_clip';
      if (key === 'v' || key === 'V') return 'paste_clip';
      return null; // any other modified combo is not one of ours (browser/OS shortcuts pass through)
    }
    switch (key) {
      case ' ': case 'Spacebar': return 'play_pause';
      case 'j': case 'J': return 'step_back';
      case 'k': case 'K': return 'pause';
      case 'l': case 'L': return 'step_forward';
      case 'Delete': case 'Backspace': return ev.shiftKey ? 'ripple_delete_clip' : 'delete_clip';
      case 's': case 'S': return 'split_at_playhead';
      default: return null;
    }
  }

  return { resolveKeyAction: resolveKeyAction };
});
