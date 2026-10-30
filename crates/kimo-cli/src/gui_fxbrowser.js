// Pure effects/transitions browser helper for kimo gui (v7.4): combines list_effects' and
// list_transitions' two different catalog shapes into one searchable list, filters it, and builds the exact
// {tool, args} call to add a chosen entry to a clip. No DOM; served at GET /gui_fxbrowser.js so the browser
// and tests/js/fxbrowser_test.js (Node, headless) run the exact same bytes.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoFxBrowser = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  // list_effects returns [{kind, description, params}]; list_transitions returns a bare [string] of kinds
  // (no description, no params) -- genuinely two different shapes, not an oversight to paper over. Both
  // become one flat {type, kind, description} entry so one list, one filter and one click-to-add path
  // serves both catalogs.
  function buildEntries(effectsCatalog, transitionKinds) {
    const effects = (effectsCatalog || []).map((e) => ({ type: 'effect', kind: e.kind, description: e.description || '' }));
    const transitions = (transitionKinds || []).map((k) => ({ type: 'transition', kind: k, description: 'transition' }));
    return effects.concat(transitions);
  }
  // Case-insensitive substring match against the kind and (for effects) its description, so "chroma" finds
  // chroma_key even if the user doesn't know its exact kind name. An empty/whitespace query matches
  // everything (the unfiltered list), same as the existing tool-search box's own behaviour.
  function filterEntries(entries, query) {
    const q = (query || '').trim().toLowerCase();
    if (!q) return entries.slice();
    return entries.filter((e) => e.kind.toLowerCase().includes(q) || (e.description || '').toLowerCase().includes(q));
  }
  // The exact tool call to add one catalog entry to a clip: add_effect for an effect (empty params -- the
  // existing per-param inspector controls set real values afterward), add_transition for a transition
  // (duration defaults to 1 second, the same default the engine's own add_transition schema example uses;
  // editable afterward the same way an effect's params are, through set_effect_param's transition cousin --
  // there is none yet, so for now duration is fixed at add time, a stated limitation rather than a hidden one).
  function addArgsFor(entry, clipId) {
    if (!entry) throw new Error('addArgsFor: entry is required');
    if (entry.type === 'transition') return { tool: 'add_transition', args: { clip_id: clipId, kind: entry.kind, duration: 1.0 } };
    return { tool: 'add_effect', args: { clip_id: clipId, kind: entry.kind, params: {} } };
  }
  return { buildEntries: buildEntries, filterEntries: filterEntries, addArgsFor: addArgsFor };
});
