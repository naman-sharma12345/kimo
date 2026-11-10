// v7.4: pure-logic test for kimo gui's multi-select / group move / group delete (gui_multiselect.js),
// headless (no DOM/browser). Exactly the module gui_index.html's selection and drag handlers call (served
// at GET /gui_multiselect.js, same bytes).
//
//     node crates/kimo-cli/tests/js/multiselect_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { toggleSelect, isSelected, clampGroupDelta, buildGroupMoveOps, buildGroupDeleteOps } = require(path.join(__dirname, '..', '..', 'src', 'gui_multiselect.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}

// toggleSelect
check('a plain click on nothing selected selects just that clip', toggleSelect([], 'a', false), ['a']);
check('a plain click on a different clip replaces the whole selection', toggleSelect(['a', 'b'], 'c', false), ['c']);
check('a plain click on the only selected clip deselects it (toggle off)', toggleSelect(['a'], 'a', false), []);
check('a plain click on one of several selected clips still replaces with just that one', toggleSelect(['a', 'b'], 'a', false), ['a']);
check('a shift-click adds a new clip to the selection', toggleSelect(['a'], 'b', true), ['a', 'b']);
check('a shift-click on an already-selected clip removes just that one', toggleSelect(['a', 'b', 'c'], 'b', true), ['a', 'c']);
check('a shift-click with nothing selected yet starts a selection', toggleSelect([], 'a', true), ['a']);
check('a shift-click with no selected array given at all does not throw', toggleSelect(undefined, 'a', true), ['a']);

// isSelected
check('isSelected is true for a member', isSelected(['a', 'b'], 'b'), true);
check('isSelected is false for a non-member', isSelected(['a', 'b'], 'z'), false);
check('isSelected is false against an empty/undefined selection', isSelected(undefined, 'a'), false);

// clampGroupDelta
check('a rightward (positive) delta is never clamped', clampGroupDelta([2, 5, 1], 10), 10);
check('a leftward delta that keeps every start >= 0 passes through unclamped', clampGroupDelta([2, 5, 1], -1), -1);
check('a leftward delta that would push the earliest clip negative is clamped to exactly -earliest', clampGroupDelta([2, 5, 1], -3), -1);
check('an empty group clamps nothing', clampGroupDelta([], -5), -5);

// buildGroupMoveOps: one move_clip op per clip, each clamped to >= 0 and rounded
check('builds one move_clip op per clip with the delta applied', buildGroupMoveOps([{ id: 'a', start: 2 }, { id: 'b', start: 5 }], 1),
  [{ tool: 'move_clip', args: { clip_id: 'a', start: 3 } }, { tool: 'move_clip', args: { clip_id: 'b', start: 6 } }]);
check('never proposes a negative start even if the delta alone would', buildGroupMoveOps([{ id: 'a', start: 0.2 }], -5), [{ tool: 'move_clip', args: { clip_id: 'a', start: 0 } }]);
check('rounds to 1e-4s', buildGroupMoveOps([{ id: 'a', start: 1 }], 0.123456789), [{ tool: 'move_clip', args: { clip_id: 'a', start: 1.1235 } }]);

// buildGroupDeleteOps
check('builds one delete_clip op per clip id, in order', buildGroupDeleteOps(['x', 'y', 'z']),
  [{ tool: 'delete_clip', args: { clip_id: 'x' } }, { tool: 'delete_clip', args: { clip_id: 'y' } }, { tool: 'delete_clip', args: { clip_id: 'z' } }]);
check('an empty selection builds no ops', buildGroupDeleteOps([]), []);

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall multiselect tests passed');
