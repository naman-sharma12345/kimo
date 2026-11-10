// v7.5: pure-logic test for kimo gui's marker/chapter list panel (gui_markerpanel.js), headless (no DOM,
// no browser -- run directly under Node). This is the exact module gui_index.html's marker panel calls
// (served at GET /gui_markerpanel.js, same bytes), so a pass here is a pass for the browser's own panel too.
//
//     node crates/kimo-cli/tests/js/markerpanel_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { formatTime, buildMarkerRows } = require(path.join(__dirname, '..', '..', 'src', 'gui_markerpanel.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}

check('formatTime under a minute', formatTime(3.4), '0:03.40');
check('formatTime over a minute, under an hour', formatTime(63.4), '1:03.40');
check('formatTime past an hour includes the hour', formatTime(3663.4), '1:01:03.40');
check('formatTime of 0', formatTime(0), '0:00.00');
check('formatTime of a negative/garbage value clamps to 0', formatTime(-5), '0:00.00');

check('buildMarkerRows of an empty list is empty', buildMarkerRows([]), []);
check('buildMarkerRows of undefined is empty', buildMarkerRows(undefined), []);
check('buildMarkerRows carries id, label, time, timeLabel, color', buildMarkerRows([{ id: 'm1', time: 1.5, name: 'Intro', color: 'red' }]),
  [{ id: 'm1', label: 'Intro', time: 1.5, timeLabel: '0:01.50', color: 'red' }]);
check('an unnamed marker gets a generic placeholder label, never a blank row', buildMarkerRows([{ id: 'm2', time: 2, name: '' }]),
  [{ id: 'm2', label: '(marker)', time: 2, timeLabel: '0:02.00', color: 'gold' }]);
check('a marker with no name field at all also gets the placeholder', buildMarkerRows([{ id: 'm3', time: 4 }]),
  [{ id: 'm3', label: '(marker)', time: 4, timeLabel: '0:04.00', color: 'gold' }]);
check('a whitespace-only name also gets the placeholder, not a blank label', buildMarkerRows([{ id: 'm4', time: 1, name: '   ' }]),
  [{ id: 'm4', label: '(marker)', time: 1, timeLabel: '0:01.00', color: 'gold' }]);
check('preserves input order (caller is expected to pass already-sorted markers)', buildMarkerRows([{ id: 'b', time: 5 }, { id: 'a', time: 1 }]).map((r) => r.id), ['b', 'a']);
check('no color on the marker defaults to gold, matching the timeline ruler tick default', buildMarkerRows([{ id: 'm5', time: 1 }])[0].color, 'gold');

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall marker panel tests passed');
