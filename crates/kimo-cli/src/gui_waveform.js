// Pure waveform helper for kimo gui (v7.4): which clips to fetch a waveform for, the exact audio_transients
// args to ask for it with, and rewriting the returned SVG so it stretches to fill whatever pixel width the
// clip bar is drawn at (the tool's own SVG is a fixed-pixel-width visualization, not a scalable one). No
// DOM, no audio decoding here -- the real decode and the waveform SVG itself are audio_transients'
// image:"waveform" option (already existed before this feature; this wires the GUI onto it rather than
// computing a second copy of the same peaks). Served at GET /gui_waveform.js so the browser and
// tests/js/waveform_test.js (Node, headless) run the exact same bytes.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoWaveform = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  // Only a real media clip can have audio; every other source kind (solid, text, shape, emitter, ...) is
  // silent by construction, so asking audio_transients for one would just cost a wasted decode attempt
  // that returns no_audio. (A media clip that turns out to carry no audio track still costs one real
  // decode attempt per clip -- that failure is handled by the caller treating any non-ok result as "no
  // waveform to draw", not by guessing the file's content ahead of time.)
  function shouldFetchWaveform(clip) { return !!(clip && clip.source_kind === 'media'); }
  function waveformToolArgs(clip) { return { target: { clip_id: clip.id }, image: 'waveform' }; }

  // Rewrites <svg ... width="W" height="H"> to also carry viewBox="0 0 W H" preserveAspectRatio="none" and
  // width/height of 100%, so the same markup that is W pixels wide at its native size instead stretches to
  // fill its CSS container (the clip bar, whatever pixels-per-second zoom drew it at). Passes anything that
  // doesn't look like the expected SVG shape straight through unchanged, rather than guessing.
  function scaleSvgToFill(svgMarkup) {
    if (typeof svgMarkup !== 'string') return svgMarkup;
    const tagMatch = svgMarkup.match(/<svg\b([^>]*)>/);
    if (!tagMatch) return svgMarkup;
    const attrs = tagMatch[1];
    const wm = attrs.match(/\bwidth="([\d.]+)"/);
    const hm = attrs.match(/\bheight="([\d.]+)"/);
    if (!wm || !hm) return svgMarkup;
    const strippedAttrs = attrs.replace(/\s*\bwidth="[^"]*"/, '').replace(/\s*\bheight="[^"]*"/, '');
    const newOpenTag = `<svg${strippedAttrs} viewBox="0 0 ${wm[1]} ${hm[1]}" preserveAspectRatio="none" width="100%" height="100%">`;
    return svgMarkup.slice(0, tagMatch.index) + newOpenTag + svgMarkup.slice(tagMatch.index + tagMatch[0].length);
  }

  return { shouldFetchWaveform: shouldFetchWaveform, waveformToolArgs: waveformToolArgs, scaleSvgToFill: scaleSvgToFill };
});
