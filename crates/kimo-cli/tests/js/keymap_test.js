// v7.3: pure-logic test for kimo gui's keyboard shortcuts (gui_keymap.js), headless (no DOM, no browser --
// run directly under Node). This is the exact module gui_index.html's keydown listener calls (served at
// GET /gui_keymap.js, same bytes), so a pass here is a pass for the browser's own mapping too.
//
//     node crates/kimo-cli/tests/js/keymap_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { resolveKeyAction } = require(path.join(__dirname, '..', '..', 'src', 'gui_keymap.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}
const body = { tagName: 'BODY' };
const input = { tagName: 'INPUT' };
const textarea = { tagName: 'TEXTAREA' };

check('space plays/pauses', resolveKeyAction({ key: ' ', target: body }), 'play_pause');
check('j steps back', resolveKeyAction({ key: 'j', target: body }), 'step_back');
check('J (shifted) also steps back', resolveKeyAction({ key: 'J', target: body }), 'step_back');
check('k pauses', resolveKeyAction({ key: 'k', target: body }), 'pause');
check('l steps forward', resolveKeyAction({ key: 'l', target: body }), 'step_forward');
check('Delete deletes the selected clip', resolveKeyAction({ key: 'Delete', target: body }), 'delete_clip');
check('Backspace also deletes the selected clip', resolveKeyAction({ key: 'Backspace', target: body }), 'delete_clip');
check('s splits at the playhead', resolveKeyAction({ key: 's', target: body }), 'split_at_playhead');
check('ctrl+z undoes', resolveKeyAction({ key: 'z', ctrlKey: true, target: body }), 'undo');
check('cmd+z (meta) undoes too', resolveKeyAction({ key: 'z', metaKey: true, target: body }), 'undo');
check('ctrl+shift+z redoes', resolveKeyAction({ key: 'z', ctrlKey: true, shiftKey: true, target: body }), 'redo');
check('ctrl+Z (shifted Z alone, no shiftKey flag) still undoes on the key value', resolveKeyAction({ key: 'Z', ctrlKey: true, target: body }), 'undo');
check('an unmapped key is null', resolveKeyAction({ key: 'q', target: body }), null);
check('alt+anything is left alone (not ours)', resolveKeyAction({ key: ' ', altKey: true, target: body }), null);
check('ctrl+other-letter is left alone (browser/OS shortcuts pass through)', resolveKeyAction({ key: 'a', ctrlKey: true, target: body }), null);

// The whole point: typing in a field must never fire a shortcut -- space in the tool-filter box types a
// space, Delete in the args textarea deletes a character, not the selected clip.
check('space while focused in an input does nothing (typing a literal space)', resolveKeyAction({ key: ' ', target: input }), null);
check('Delete while focused in a textarea does nothing (editing text, not the timeline)', resolveKeyAction({ key: 'Delete', target: textarea }), null);
check('ctrl+z while focused in an input still does nothing (field owns its own undo)', resolveKeyAction({ key: 'z', ctrlKey: true, target: input }), null);
check('j while focused in a select does nothing', resolveKeyAction({ key: 'j', target: { tagName: 'SELECT' } }), null);

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall keymap tests passed');
