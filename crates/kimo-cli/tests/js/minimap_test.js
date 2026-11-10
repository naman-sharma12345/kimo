// v7.6: pure-logic test for kimo gui's timeline minimap (gui_minimap.js), headless.
//     node crates/kimo-cli/tests/js/minimap_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { scaleForWidth, buildBlocks, viewportRect, timeFromClick } = require(path.join(__dirname, '..', '..', 'src', 'gui_minimap.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}

check('scaleForWidth fits the duration exactly into the width', scaleForWidth(10, 300), 30);
check('scaleForWidth of 0 duration uses the 1s floor, not infinity', scaleForWidth(0, 300), 300);
check('scaleForWidth of undefined duration uses the floor', scaleForWidth(undefined, 300), 300);
check('scaleForWidth of a sub-1s duration still uses the floor', scaleForWidth(0.2, 300), 300);

const tl = { duration: 10, tracks: [
  { clips: [{ id: 'a', start: 0, duration: 2 }, { id: 'b', start: 5, duration: 3 }] },
  { clips: [{ id: 'c', start: 1, duration: 1 }] },
] };
check('buildBlocks returns one block per clip across all tracks', buildBlocks(tl, 300).length, 3);
check('buildBlocks scales start/duration into minimap pixels', buildBlocks(tl, 300).find((b) => b.clipId === 'b'), { trackIndex: 0, clipId: 'b', left: 150, width: 90 });
check('buildBlocks keeps track order (trackIndex 0 then 1)', buildBlocks(tl, 300).map((b) => b.trackIndex), [0, 0, 1]);
check('buildBlocks of an empty timeline is empty', buildBlocks({ duration: 0, tracks: [] }, 300), []);
check('buildBlocks of undefined timeline is empty', buildBlocks(undefined, 300), []);
check('buildBlocks never gives a zero-width block for a very short clip', buildBlocks({ duration: 1000, tracks: [{ clips: [{ id: 'z', start: 0, duration: 0.001 }] }] }, 300)[0].width, 1);

check('viewportRect of the whole duration fills the whole strip', viewportRect(10, 300, 0, 10), { left: 0, width: 300 });
check('viewportRect of the first half', viewportRect(10, 300, 0, 5), { left: 0, width: 150 });
check('viewportRect scrolled to the middle', viewportRect(10, 300, 3, 4), { left: 90, width: 120 });
check('viewportRect clamps when scrolled past the end', viewportRect(10, 300, 9, 5), { left: 270, width: 30 });
check('viewportRect clamps a scroll position past the whole duration to the strip edge', viewportRect(10, 300, 20, 5), { left: 300, width: 1 });
check('viewportRect never goes negative for a negative scroll', viewportRect(10, 300, -5, 5), { left: 0, width: 1 });

check('timeFromClick at the strip start is time 0', timeFromClick(0, 300, 10), 0);
check('timeFromClick at the strip end is the full duration', timeFromClick(300, 300, 10), 10);
check('timeFromClick at the midpoint is half the duration', timeFromClick(150, 300, 10), 5);
check('timeFromClick past the right edge clamps to the duration', timeFromClick(1000, 300, 10), 10);
check('timeFromClick before the left edge clamps to 0', timeFromClick(-50, 300, 10), 0);
check('timeFromClick on a zero-duration project is 0', timeFromClick(150, 300, 0), 0);

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall minimap tests passed');
