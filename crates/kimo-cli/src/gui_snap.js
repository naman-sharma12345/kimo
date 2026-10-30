// Pure timeline-snapping helper for kimo gui. No DOM: given a proposed time and a flat list of candidate
// edit-point times (clip starts/ends, marker times, 0 -- gathered once per drag from the already-loaded
// timeline, so a drag never needs a mid-drag API round trip), returns the nearest candidate when it is
// within toleranceSec, else the proposed time unchanged. Served at GET /gui_snap.js (same bytes a browser
// <script src> and a Node test both run) -- one function, not two to keep in sync.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoSnap = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  // Nearest candidate to `proposed` within `toleranceSec`; ties go to the smaller time (stable, matches
  // sorting candidates ascending and taking the first within tolerance at equal distance).
  function snapTime(proposed, candidates, toleranceSec) {
    if (!candidates || candidates.length === 0) return proposed;
    var best = null; var bestDist = Infinity;
    for (var i = 0; i < candidates.length; i++) {
      var c = candidates[i];
      var d = Math.abs(c - proposed);
      if (d < bestDist || (d === bestDist && best !== null && c < best)) { bestDist = d; best = c; }
    }
    if (best !== null && bestDist <= toleranceSec) return best;
    return proposed;
  }

  // Every edit-point time worth snapping to from a loaded /api/timeline view-model: every clip's start
  // and end on every track (optionally excluding one clip id, the one being dragged, so a clip never
  // snaps to its own edge), every marker's time, and 0 (the same candidate set snap_time's own
  // description names: "clip edges, markers, playhead 0").
  function candidatesFromTimeline(tl, excludeClipId) {
    var out = [0];
    var tracks = (tl && tl.tracks) || [];
    for (var i = 0; i < tracks.length; i++) {
      var clips = tracks[i].clips || [];
      for (var j = 0; j < clips.length; j++) {
        var c = clips[j];
        if (excludeClipId && c.id === excludeClipId) continue;
        out.push(c.start, c.end);
      }
    }
    var markers = (tl && tl.markers) || [];
    for (var k = 0; k < markers.length; k++) out.push(markers[k].time);
    return out;
  }

  return { snapTime: snapTime, candidatesFromTimeline: candidatesFromTimeline };
});
