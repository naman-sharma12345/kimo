// v7.4: pure-logic test for kimo gui's track mute/solo/lock buttons (gui_trackstate.js), headless (no
// DOM/browser). Exactly the module gui_index.html's track-row renderer calls (served at GET
// /gui_trackstate.js, same bytes).
//
//     node crates/kimo-cli/tests/js/trackstate_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { buttonState, clipInteractionAllowed, toggleArgs } = require(path.join(__dirname, '..', '..', 'src', 'gui_trackstate.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}

// buttonState
check('a plain track (no flags) shows all three buttons inactive',
  buttonState({ locked: false, muted: false, solo: false }),
  { lock: { label: 'L', active: false, title: 'lock this track (blocks edits)' },
    mute: { label: 'M', active: false, title: 'mute this track' },
    solo: { label: 'S', active: false, title: 'solo this track (silences every other track)' } });
check('a locked+muted track shows those two active, solo inactive',
  buttonState({ locked: true, muted: true, solo: false }),
  { lock: { label: 'L', active: true, title: 'locked: unlock to edit this track' },
    mute: { label: 'M', active: true, title: 'muted: click to unmute' },
    solo: { label: 'S', active: false, title: 'solo this track (silences every other track)' } });
check('a soloed track shows solo active', buttonState({ solo: true }).solo.active, true);
check('missing track object defaults every button to inactive', buttonState(undefined).lock.active, false);

// clipInteractionAllowed
check('an unlocked track allows clip drag/trim', clipInteractionAllowed({ locked: false }), true);
check('a locked track blocks clip drag/trim', clipInteractionAllowed({ locked: true }), false);
check('a track with no locked key at all allows interaction (default unlocked)', clipInteractionAllowed({}), true);
check('a null track allows interaction', clipInteractionAllowed(null), true);

// toggleArgs: exactly one key, the inverse of the current flag
check('toggling lock on an unlocked track asks to lock it', toggleArgs({ locked: false }, 'lock'), { locked: true });
check('toggling lock on a locked track asks to unlock it', toggleArgs({ locked: true }, 'lock'), { locked: false });
check('toggling mute asks for the inverse, leaving lock/solo out of the call', toggleArgs({ muted: false, locked: true, solo: true }, 'mute'), { muted: true });
check('toggling solo asks for the inverse', toggleArgs({ solo: true }, 'solo'), { solo: false });
check('toggling an unknown flag throws', (() => { try { toggleArgs({}, 'bogus'); return 'no throw'; } catch (e) { return e.message; } })(), 'toggleArgs: which must be lock, mute or solo, got bogus');

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall track-state tests passed');
