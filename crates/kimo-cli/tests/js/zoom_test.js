// v7.3: pure-logic test for kimo gui's timeline zoom (gui_zoom.js), headless (no DOM/browser). This is
// the exact module gui_index.html's zoomBy()/zoomReset() call (served at GET /gui_zoom.js, same bytes).
//
//     node crates/kimo-cli/tests/js/zoom_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { MIN_PX_PER_SEC, MAX_PX_PER_SEC, DEFAULT_PX_PER_SEC, clampZoom, zoomBy } = require(path.join(__dirname, '..', '..', 'src', 'gui_zoom.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}

check('default is 40px/s', DEFAULT_PX_PER_SEC, 40);
check('zooming in doubles', zoomBy(40, 2), 80);
check('zooming out halves', zoomBy(80, 0.5), 40);
check('zooming in never exceeds the max', zoomBy(300, 2), MAX_PX_PER_SEC);
check('zooming out never drops below the min', zoomBy(5, 0.5), MIN_PX_PER_SEC);
check('repeated zoom-in eventually clamps and stays clamped', zoomBy(zoomBy(zoomBy(MAX_PX_PER_SEC, 2), 2), 2), MAX_PX_PER_SEC);
check('clampZoom leaves an in-range value untouched', clampZoom(100), 100);
check('clampZoom pulls a too-small value up to the min', clampZoom(0), MIN_PX_PER_SEC);
check('clampZoom pulls a too-large value down to the max', clampZoom(99999), MAX_PX_PER_SEC);
check('zoomBy rounds to a whole pixel', zoomBy(41, 1.5), 62); // 61.5 -> 62

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall zoom tests passed');
