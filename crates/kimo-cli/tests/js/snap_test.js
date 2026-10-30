// v7.3: pure-logic test for kimo gui's timeline snapping (gui_snap.js), headless (no DOM/browser). This
// is the exact module gui_index.html's drag handlers call (served at GET /gui_snap.js, same bytes).
//
//     node crates/kimo-cli/tests/js/snap_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { snapTime, candidatesFromTimeline } = require(path.join(__dirname, '..', '..', 'src', 'gui_snap.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}

check('snaps to the nearest candidate within tolerance', snapTime(5.08, [0, 5.0, 10.0], 0.2), 5.0);
check('does not snap when nothing is within tolerance', snapTime(5.08, [0, 10.0], 0.05), 5.08);
check('an empty candidate list leaves the proposed time untouched', snapTime(3.2, [], 0.5), 3.2);
check('picks the closer of two in-range candidates', snapTime(4.9, [4.8, 5.1], 0.5), 4.8);
check('exact match snaps to itself', snapTime(2.0, [2.0], 0.1), 2.0);
check('snaps to 0 (the playhead-at-rest candidate)', snapTime(0.03, [0, 20.0], 0.05), 0);

// candidatesFromTimeline: the set a drag would actually snap against, built once from the loaded
// /api/timeline view-model -- every clip edge (except the one being dragged), every marker, and 0.
const tl = {
  tracks: [
    { clips: [{ id: 'a', start: 0, end: 2 }, { id: 'b', start: 2, end: 5 }] },
    { clips: [{ id: 'c', start: 1, end: 3 }] },
  ],
  markers: [{ time: 4.5 }],
};
check('gathers every clip edge, every marker, and 0', candidatesFromTimeline(tl, null).sort((x, y) => x - y), [0, 0, 1, 2, 2, 3, 4.5, 5]);
check('excludes the dragged clip\'s own edges (so it never snaps to itself)', candidatesFromTimeline(tl, 'b').sort((x, y) => x - y), [0, 0, 1, 2, 3, 4.5]);
check('an empty timeline still yields the 0 candidate', candidatesFromTimeline({ tracks: [], markers: [] }, null), [0]);
check('a timeline with no markers key at all does not throw', candidatesFromTimeline({ tracks: [] }, null), [0]);

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall snap tests passed');
