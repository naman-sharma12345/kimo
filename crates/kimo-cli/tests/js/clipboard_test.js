// v7.5: pure-logic test for kimo gui's clipboard (gui_clipboard.js), headless (no DOM, no browser --
// run directly under Node). This is the exact module gui_index.html's copy/paste shortcuts call (served
// at GET /gui_clipboard.js, same bytes), so a pass here is a pass for the browser's own copy/paste too.
//
//     node crates/kimo-cli/tests/js/clipboard_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { copySelection, buildPasteOps, buildRippleDeleteOp } = require(path.join(__dirname, '..', '..', 'src', 'gui_clipboard.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}

check('copySelection snapshots id and start, nothing else', copySelection([{ id: 'c1', start: 1.5, duration: 2, name: 'ignored' }]), [{ id: 'c1', start: 1.5 }]);
check('copySelection of an empty list is an empty clipboard', copySelection([]), []);
check('copySelection of undefined is an empty clipboard', copySelection(undefined), []);

check('an empty clipboard pastes nothing', buildPasteOps([], 5), []);
check('a single-clip paste lands exactly at pasteAtTime', buildPasteOps([{ id: 'c1', start: 3 }], 10), [{ tool: 'duplicate_clip', args: { clip_id: 'c1', start: 10 } }]);
check('a two-clip paste keeps their original relative offset', buildPasteOps([{ id: 'c1', start: 2 }, { id: 'c2', start: 4 }], 10),
  [{ tool: 'duplicate_clip', args: { clip_id: 'c1', start: 10 } }, { tool: 'duplicate_clip', args: { clip_id: 'c2', start: 12 } }]);
check('clipboard order does not matter -- the earliest-starting clip always lands at pasteAtTime', buildPasteOps([{ id: 'c2', start: 4 }, { id: 'c1', start: 2 }], 10),
  [{ tool: 'duplicate_clip', args: { clip_id: 'c2', start: 12 } }, { tool: 'duplicate_clip', args: { clip_id: 'c1', start: 10 } }]);
check('pasting never goes negative even if pasteAtTime is 0 and offsets would otherwise go below it', buildPasteOps([{ id: 'c1', start: 0 }], 0), [{ tool: 'duplicate_clip', args: { clip_id: 'c1', start: 0 } }]);
check('paste rounds to 1e-4s precision', buildPasteOps([{ id: 'c1', start: 0 }, { id: 'c2', start: 0.1 + 0.2 }], 1), [{ tool: 'duplicate_clip', args: { clip_id: 'c1', start: 1 } }, { tool: 'duplicate_clip', args: { clip_id: 'c2', start: 1.3 } }]);

check('buildRippleDeleteOp is one ripple_delete call for the given clip', buildRippleDeleteOp('c7'), { tool: 'ripple_delete', args: { clip_id: 'c7' } });

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall clipboard tests passed');
