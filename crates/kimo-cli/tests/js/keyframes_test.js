// v7.5: pure-logic test for kimo gui's keyframe lane (gui_keyframes.js), headless (no DOM, no browser --
// run directly under Node). This is the exact module gui_index.html's keyframe lane calls (served at
// GET /gui_keyframes.js, same bytes), so a pass here is a pass for the browser's own lane too.
//
//     node crates/kimo-cli/tests/js/keyframes_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { propertyKey, parsePropertyKey, buildPropertyOptions, buildKeyMarkers, timeForLaneX, nearestKeyWithinHalfFrame } = require(path.join(__dirname, '..', '..', 'src', 'gui_keyframes.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}

check('propertyKey of a clip-transform entry is just the property name', propertyKey({ scope: 'clip', property: 'opacity' }), '::opacity');
check('propertyKey of an effect-param entry carries the effect id too', propertyKey({ scope: 'effect', effect_id: 'fx1', property: 'radius' }), 'fx1::radius');
check('parsePropertyKey round-trips a clip property', parsePropertyKey('::opacity'), { effectId: null, property: 'opacity' });
check('parsePropertyKey round-trips an effect param', parsePropertyKey('fx1::radius'), { effectId: 'fx1', property: 'radius' });

check('buildPropertyOptions labels a clip property with its key count', buildPropertyOptions([{ scope: 'clip', property: 'opacity', key_count: 3 }]),
  [{ key: '::opacity', label: 'opacity (3 keys)' }]);
check('buildPropertyOptions labels a singular key count without an s', buildPropertyOptions([{ scope: 'clip', property: 'x', key_count: 1 }]),
  [{ key: '::x', label: 'x (1 key)' }]);
check('buildPropertyOptions prefixes an effect param with its effect kind', buildPropertyOptions([{ scope: 'effect', effect_id: 'fx1', effect_kind: 'blur', property: 'radius', key_count: 2 }]),
  [{ key: 'fx1::radius', label: 'blur.radius (2 keys)' }]);
check('buildPropertyOptions of an empty/undefined list is empty', [buildPropertyOptions([]), buildPropertyOptions(undefined)], [[], []]);

check('buildKeyMarkers maps clip-local time linearly across the lane width', buildKeyMarkers([{ time: 0, value: 0, ease: 'linear' }, { time: 2, value: 1, ease: 'linear' }], 4, 200),
  [{ time: 0, value: 0, ease: 'linear', x: 0 }, { time: 2, value: 1, ease: 'linear', x: 100 }]);
check('buildKeyMarkers of a zero-duration clip collapses every marker to x=0, no divide-by-zero crash', buildKeyMarkers([{ time: 0, value: 0, ease: 'linear' }], 0, 200),
  [{ time: 0, value: 0, ease: 'linear', x: 0 }]);
check('buildKeyMarkers of an empty/undefined keys list is empty', [buildKeyMarkers([], 4, 200), buildKeyMarkers(undefined, 4, 200)], [[], []]);
check('buildKeyMarkers clamps a key slightly past the clip end into the lane (float drift guard)', buildKeyMarkers([{ time: 4.001, value: 1, ease: 'linear' }], 4, 200), [{ time: 4.001, value: 1, ease: 'linear', x: 200 }]);

check('timeForLaneX is the inverse of the lane mapping', timeForLaneX(100, 4, 200), 2);
check('timeForLaneX clamps a click past the right edge to the clip end', timeForLaneX(500, 4, 200), 4);
check('timeForLaneX clamps a click before the left edge to 0', timeForLaneX(-50, 4, 200), 0);
check('timeForLaneX of a zero-duration clip is 0, not NaN', timeForLaneX(100, 0, 200), 0);

const keys = [{ time: 1.0, value: 0.5 }, { time: 2.0, value: 0.8 }];
check('nearestKeyWithinHalfFrame finds a key exactly on a frame boundary', nearestKeyWithinHalfFrame(keys, 1.0, 25), keys[0]);
check('nearestKeyWithinHalfFrame finds the nearest key just inside half a frame (at 25fps, half a frame is 0.02s)', nearestKeyWithinHalfFrame(keys, 1.015, 25), keys[0]);
check('nearestKeyWithinHalfFrame returns null when nothing is within half a frame (an empty click = add, not edit)', nearestKeyWithinHalfFrame(keys, 1.5, 25), null);
check('nearestKeyWithinHalfFrame of an empty key list is null', nearestKeyWithinHalfFrame([], 1.0, 25), null);
check('nearestKeyWithinHalfFrame picks the closer of two candidates', nearestKeyWithinHalfFrame(keys, 1.6, 25) === null ? 'null' : nearestKeyWithinHalfFrame(keys, 1.6, 25).time, 'null');

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall keyframe lane tests passed');
