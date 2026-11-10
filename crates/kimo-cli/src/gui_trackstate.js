// Pure track-state-button helper for kimo gui (v7.4): given one track's {locked, muted, solo} from the
// /api/timeline view-model, what the Lock/Mute/Solo buttons should show and whether clip drag/trim should
// be allowed. No DOM -- gui_index.html's track row renderer and tests/js/trackstate_test.js (Node, headless)
// run the exact same bytes, served at GET /gui_trackstate.js.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoTrackState = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  // One button's rendering: a fixed label (so a toggle never relabels itself, only its active state) and
  // whether it is "on" right now, plus a title string naming the real effect for a screen reader / tooltip.
  function buttonState(track) {
    track = track || {};
    return {
      lock: { label: 'L', active: !!track.locked, title: track.locked ? 'locked: unlock to edit this track' : 'lock this track (blocks edits)' },
      mute: { label: 'M', active: !!track.muted, title: track.muted ? 'muted: click to unmute' : 'mute this track' },
      solo: { label: 'S', active: !!track.solo, title: track.solo ? 'soloed: click to un-solo' : 'solo this track (silences every other track)' },
    };
  }
  // A locked track blocks edits from the timeline tools (set_track's own documented behaviour); the GUI
  // mirrors that by refusing to start a drag/trim on any of its clips rather than letting the drag run and
  // then surfacing a tool error only on mouseup.
  function clipInteractionAllowed(track) { return !(track && track.locked); }
  // The next set_track call to make when a button is clicked: toggles exactly the one flag the button owns,
  // leaving the other two untouched (set_track only writes the keys actually present in args).
  function toggleArgs(track, which) {
    track = track || {};
    if (which === 'lock') return { locked: !track.locked };
    if (which === 'mute') return { muted: !track.muted };
    if (which === 'solo') return { solo: !track.solo };
    throw new Error('toggleArgs: which must be lock, mute or solo, got ' + which);
  }
  return { buttonState: buttonState, clipInteractionAllowed: clipInteractionAllowed, toggleArgs: toggleArgs };
});
