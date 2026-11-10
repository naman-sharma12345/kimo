// v7.6: pure-logic test for kimo gui's media bin panel (gui_mediabin.js), headless.
//     node crates/kimo-cli/tests/js/mediabin_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { basename, formatDuration, matchesQuery, buildAssetRows } = require(path.join(__dirname, '..', '..', 'src', 'gui_mediabin.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}

check('basename of a unix path', basename('/media/clips/intro.mp4'), 'intro.mp4');
check('basename of a windows-ish path', basename('C:\\media\\clips\\intro.mp4'), 'intro.mp4');
check('basename of a bare name is itself', basename('intro.mp4'), 'intro.mp4');
check('basename of empty/undefined is empty', basename(''), '');
check('basename of undefined is empty', basename(undefined), '');

check('formatDuration under a minute', formatDuration(3.4), '0:03.4');
check('formatDuration over a minute', formatDuration(63.4), '1:03.4');
check('formatDuration of 0', formatDuration(0), '0:00.0');
check('formatDuration of a negative value clamps to 0', formatDuration(-5), '0:00.0');

const asset = { asset_id: 'a1', path: '/media/Intro Shot.mp4' };
check('matchesQuery empty query matches everything', matchesQuery(asset, ''), true);
check('matchesQuery whitespace-only query matches everything', matchesQuery(asset, '   '), true);
check('matchesQuery matches case-insensitively on the file name', matchesQuery(asset, 'intro'), true);
check('matchesQuery does not match an unrelated substring', matchesQuery(asset, 'outro'), false);
check('matchesQuery matches a path-free name fragment', matchesQuery(asset, 'shot'), true);

const assets = [
  { asset_id: 'a2', path: '/media/b_roll.mp4', bin: 'Video', duration: 12.25, used_by_clips: 2 },
  { asset_id: 'a1', path: '/media/intro.mp4', duration: 3.4, used_by_clips: 0 },
];
const rows = buildAssetRows(assets, '');
check('buildAssetRows returns one row per asset', rows.length, 2);
check('buildAssetRows sorts by display name, not input order', rows.map((r) => r.name), ['b_roll.mp4', 'intro.mp4']);
check('buildAssetRows marks a used asset used:true', rows.find((r) => r.asset_id === 'a2').used, true);
check('buildAssetRows marks an unused asset used:false', rows.find((r) => r.asset_id === 'a1').used, false);
check('buildAssetRows defaults a missing bin to (unfiled)', rows.find((r) => r.asset_id === 'a1').bin, '(unfiled)');
check('buildAssetRows keeps a real bin name', rows.find((r) => r.asset_id === 'a2').bin, 'Video');
check('buildAssetRows builds a thumbnail URL from the asset id', rows.find((r) => r.asset_id === 'a1').thumbUrl, '/asset_thumb.png?asset_id=a1');
check('buildAssetRows formats duration', rows.find((r) => r.asset_id === 'a1').durationLabel, '0:03.4');

check('buildAssetRows filters by the search query', buildAssetRows(assets, 'b_roll').map((r) => r.asset_id), ['a2']);
check('buildAssetRows of an empty/undefined list is empty', buildAssetRows([], 'x'), []);
check('buildAssetRows of undefined list is empty', buildAssetRows(undefined, ''), []);

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall media bin tests passed');
