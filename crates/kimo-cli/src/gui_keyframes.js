// Pure keyframe-lane helper for kimo gui (v7.5): a lane for ONE selected animated property on the selected
// clip, keyed to list_animated_properties/get_curve's own shapes. No DOM, no new engine math -- this only
// turns their JSON into pixel positions and back, and mirrors the server's own "nearest key within half a
// frame" rule (move_key/remove_key) so a click can tell "this is an existing key" from "this is empty lane"
// without guessing or a wasted round trip. Served at GET /gui_keyframes.js so the browser and
// tests/js/keyframes_test.js (Node) run the exact same bytes.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoKeyframes = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  // list_animated_properties' own entries: {scope: "clip"|"effect", effect_id, effect_kind, property, base, key_count, expr_count}.
  // A composite key disambiguates two different effects that both animate a param of the same name.
  function propertyKey(entry) { return (entry.effect_id || '') + '::' + entry.property; }
  function parsePropertyKey(key) {
    const i = key.indexOf('::');
    const effectId = key.slice(0, i);
    return { effectId: effectId || null, property: key.slice(i + 2) };
  }
  function buildPropertyOptions(animated) {
    return (animated || []).map((e) => ({
      key: propertyKey(e),
      label: (e.scope === 'effect' ? `${e.effect_kind || 'fx'}.` : '') + e.property + ` (${e.key_count} key${e.key_count === 1 ? '' : 's'})`,
    }));
  }

  function round4(x) { return Math.round(x * 10000) / 10000; }
  function clamp(x, lo, hi) { return Math.max(lo, Math.min(hi, x)); }

  // get_curve's own `keys` array ([{time, value, ease, bezier, in, out}]) -> lane marker positions, clip-local
  // time mapped linearly across [0, duration] onto [0, laneWidthPx]. duration <= 0 is a clip with no length
  // yet to show a lane against -- every marker collapses to x=0 rather than dividing by zero.
  function buildKeyMarkers(keys, duration, laneWidthPx) {
    return (keys || []).map((k) => ({
      time: k.time,
      value: k.value,
      ease: k.ease,
      x: round4(duration > 0 ? clamp((k.time / duration) * laneWidthPx, 0, laneWidthPx) : 0),
    }));
  }
  // Inverse of the x mapping above -- where a click on the lane lands in clip-local seconds.
  function timeForLaneX(x, duration, laneWidthPx) {
    if (!(duration > 0) || !(laneWidthPx > 0)) return 0;
    return round4(clamp((x / laneWidthPx) * duration, 0, duration));
  }

  // Same rule the server's own move_key/remove_key use: the nearest key within half a frame, or null. Lets
  // a click decide "edit/remove this key" vs "add a new one here" the same way the tools themselves would.
  function nearestKeyWithinHalfFrame(keys, time, fps) {
    if (!keys || !keys.length || !(fps > 0)) return null;
    let best = null; let bestDist = Infinity;
    for (const k of keys) { const d = Math.abs(k.time - time); if (d < bestDist) { bestDist = d; best = k; } }
    return (best && bestDist <= 0.5 / fps) ? best : null;
  }

  return {
    propertyKey: propertyKey,
    parsePropertyKey: parsePropertyKey,
    buildPropertyOptions: buildPropertyOptions,
    buildKeyMarkers: buildKeyMarkers,
    timeForLaneX: timeForLaneX,
    nearestKeyWithinHalfFrame: nearestKeyWithinHalfFrame,
  };
});
