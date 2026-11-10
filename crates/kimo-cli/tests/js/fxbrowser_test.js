// v7.4: pure-logic test for kimo gui's effects/transitions browser (gui_fxbrowser.js), headless (no
// DOM/browser). Exactly the module gui_index.html's fx-browser panel calls (served at GET
// /gui_fxbrowser.js, same bytes).
//
//     node crates/kimo-cli/tests/js/fxbrowser_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { buildEntries, filterEntries, addArgsFor } = require(path.join(__dirname, '..', '..', 'src', 'gui_fxbrowser.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}

// buildEntries: list_effects' object shape + list_transitions' bare-string shape -> one flat list
const EFFECTS = [{ kind: 'blur', description: 'Gaussian blur', params: [{ name: 'radius', default: 2 }] }, { kind: 'chroma_key', description: 'Key out a screen colour', params: [] }];
const TRANSITIONS = ['crossfade', 'wipe_left'];
check('combines both catalogs into one flat list, effects first then transitions, in their own order',
  buildEntries(EFFECTS, TRANSITIONS),
  [{ type: 'effect', kind: 'blur', description: 'Gaussian blur' },
   { type: 'effect', kind: 'chroma_key', description: 'Key out a screen colour' },
   { type: 'transition', kind: 'crossfade', description: 'transition' },
   { type: 'transition', kind: 'wipe_left', description: 'transition' }]);
check('an effect with no description still gets an entry (empty string, not undefined)', buildEntries([{ kind: 'x' }], [])[0].description, '');
check('missing catalogs build an empty list rather than throwing', buildEntries(undefined, undefined), []);

const ENTRIES = buildEntries(EFFECTS, TRANSITIONS);

// filterEntries
check('an empty query returns everything, unfiltered', filterEntries(ENTRIES, ''), ENTRIES);
check('a whitespace-only query also returns everything', filterEntries(ENTRIES, '   '), ENTRIES);
check('filters by kind substring, case-insensitive', filterEntries(ENTRIES, 'BLUR').map((e) => e.kind), ['blur']);
check('filters by description substring too (finds chroma_key via "screen")', filterEntries(ENTRIES, 'screen').map((e) => e.kind), ['chroma_key']);
check('matches a transition kind', filterEntries(ENTRIES, 'cross').map((e) => e.kind), ['crossfade']);
check('no match is an empty list, not an error', filterEntries(ENTRIES, 'zzzznothing'), []);

// addArgsFor
check('an effect entry builds an add_effect call with empty params', addArgsFor({ type: 'effect', kind: 'blur' }, 'c1'), { tool: 'add_effect', args: { clip_id: 'c1', kind: 'blur', params: {} } });
check('a transition entry builds an add_transition call with a 1s default duration', addArgsFor({ type: 'transition', kind: 'crossfade' }, 'c1'), { tool: 'add_transition', args: { clip_id: 'c1', kind: 'crossfade', duration: 1.0 } });
check('a missing entry throws rather than silently building a bad call', (() => { try { addArgsFor(null, 'c1'); return 'no throw'; } catch (e) { return e.message; } })(), 'addArgsFor: entry is required');

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall fx-browser tests passed');
