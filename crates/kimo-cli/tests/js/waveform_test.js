// v7.4: pure-logic test for kimo gui's waveform fetch/scale helper (gui_waveform.js), headless (no
// DOM/browser, no audio decode -- the real decode and SVG generation live in audio_transients, already
// shipped; this module just decides which clips to ask and rewrites the SVG it gets back to fill its box).
// Served at GET /gui_waveform.js, same bytes.
//
//     node crates/kimo-cli/tests/js/waveform_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { shouldFetchWaveform, waveformToolArgs, scaleSvgToFill } = require(path.join(__dirname, '..', '..', 'src', 'gui_waveform.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}

// shouldFetchWaveform
check('a media clip should get a waveform fetch', shouldFetchWaveform({ source_kind: 'media' }), true);
check('a solid clip should not', shouldFetchWaveform({ source_kind: 'solid' }), false);
check('a text clip should not', shouldFetchWaveform({ source_kind: 'text' }), false);
check('a shape clip should not', shouldFetchWaveform({ source_kind: 'shape' }), false);
check('an undefined clip should not throw and should not', shouldFetchWaveform(undefined), false);

// waveformToolArgs
check('builds the exact audio_transients args for a clip', waveformToolArgs({ id: 'c1' }), { target: { clip_id: 'c1' }, image: 'waveform' });

// scaleSvgToFill: the exact shape audio_transients' image:"waveform" actually returns
const REAL_SAMPLE = '<svg xmlns="http://www.w3.org/2000/svg" width="640" height="100"><path d="M0 48.000V52.000M1 10.000V90.000" stroke="#ff5c28"/></svg>';
const scaled = scaleSvgToFill(REAL_SAMPLE);
check('adds a viewBox matching the original pixel size', scaled.includes('viewBox="0 0 640 100"'), true);
check('adds preserveAspectRatio="none" so it stretches rather than letterboxes', scaled.includes('preserveAspectRatio="none"'), true);
check('rewrites width/height to 100% (exactly one of each, not left duplicated)', (scaled.match(/width="100%"/g) || []).length, 1);
check('keeps the xmlns attribute intact', scaled.includes('xmlns="http://www.w3.org/2000/svg"'), true);
check('keeps the path content byte-for-byte', scaled.includes('<path d="M0 48.000V52.000M1 10.000V90.000" stroke="#ff5c28"/>'), true);
check('the result is still one well-formed svg..svg pair (no stray tags)', (scaled.match(/<svg/g) || []).length === 1 && scaled.endsWith('</svg>'), true);

check('a non-string input passes through unchanged', scaleSvgToFill(null), null);
check('a string with no <svg tag passes through unchanged', scaleSvgToFill('not svg at all'), 'not svg at all');
check('an svg tag missing width/height passes through unchanged rather than guessing', scaleSvgToFill('<svg xmlns="x"><path/></svg>'), '<svg xmlns="x"><path/></svg>');

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall waveform tests passed');
