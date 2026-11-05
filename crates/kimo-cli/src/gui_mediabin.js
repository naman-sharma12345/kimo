// Pure media bin helper for kimo gui (v7.6): a searchable, thumbnailed list of every asset in the media
// pool (find_assets' own result, reshaped for display) -- the GUI has no view of imported-but-not-yet-
// placed media at all today; only clips already on the timeline show up anywhere in the UI. No DOM;
// served at GET /gui_mediabin.js so the browser and tests/js/mediabin_test.js (Node) run the exact same
// bytes.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoMediaBin = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  function basename(path) {
    if (!path) return '';
    const parts = String(path).split(/[\\/]/);
    return parts[parts.length - 1] || path;
  }

  // "1:03.4" for anything under an hour -- a short duration label for a small list row, not a clock
  // (gui_markerpanel.js's formatTime is a separate module and this stays dependency-free).
  function formatDuration(seconds) {
    const s = Math.max(0, seconds || 0);
    const m = Math.floor(s / 60);
    const rem = s - m * 60;
    return `${m}:${rem.toFixed(1).padStart(4, '0')}`;
  }

  // Case-insensitive substring match on the file name (not the whole path, so searching "intro" finds
  // /media/clips/intro.mp4 but a folder literally named "intro" containing unrelated files does not
  // flood the result). An empty/whitespace-only query matches everything.
  function matchesQuery(asset, query) {
    const q = (query || '').trim().toLowerCase();
    if (!q) return true;
    return basename(asset.path).toLowerCase().includes(q);
  }

  // find_assets' own matches array -> display rows: a short name, the full path (for a tooltip), bin
  // (or "(unfiled)"), a formatted duration, whether anything on the timeline currently uses it
  // (find_assets' own used_by_clips, already the agent-facing reverse lookup), and the thumbnail URL this
  // asset's own id resolves to (served by gui.rs's /asset_thumb.png, a fresh ephemeral one-clip render of
  // just this asset -- never touching the live project). Filtered by matchesQuery, then sorted by name so
  // the list does not reshuffle as the user types.
  function buildAssetRows(assets, query) {
    return (assets || [])
      .filter((a) => matchesQuery(a, query))
      .map((a) => ({
        asset_id: a.asset_id,
        name: basename(a.path),
        path: a.path,
        bin: a.bin || '(unfiled)',
        durationLabel: formatDuration(a.duration),
        used: (a.used_by_clips || 0) > 0,
        thumbUrl: '/asset_thumb.png?asset_id=' + encodeURIComponent(a.asset_id),
      }))
      .sort((x, y) => x.name.localeCompare(y.name));
  }

  // A manual media-bin drop onto an unlocked timeline track uses the imported
  // asset's full probed duration and never changes fit/edit mode implicitly.
  // Invalid or stale drag payloads are rejected before reaching the tool API.
  function buildDropArgs(asset, track, start) {
    if (!asset || typeof asset.asset_id !== 'string' || !asset.asset_id.trim()) return { ok: false, error: 'That media item is no longer available.' };
    if (!track || typeof track.id !== 'string' || !track.id.trim()) return { ok: false, error: 'Drop onto a timeline track.' };
    if (track.locked) return { ok: false, error: 'Unlock this track before adding media.' };
    if (typeof start !== 'number' || !Number.isFinite(start)) return { ok: false, error: 'Choose a valid timeline position.' };
    if (typeof asset.duration !== 'number' || !Number.isFinite(asset.duration) || asset.duration <= 0) return { ok: false, error: 'This media item has no usable duration.' };
    return { ok: true, args: { track_id: track.id, source_type: 'media', asset_id: asset.asset_id, start: Math.max(0, start), duration: asset.duration } };
  }

  return { basename: basename, formatDuration: formatDuration, matchesQuery: matchesQuery, buildAssetRows: buildAssetRows, buildDropArgs: buildDropArgs };
});
