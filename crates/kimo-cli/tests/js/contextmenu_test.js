// v7.6: pure-logic test for kimo gui's clip context menu (gui_contextmenu.js), headless.
//     node crates/kimo-cli/tests/js/contextmenu_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { buildMenuItems, actionFor, menuPosition } = require(path.join(__dirname, '..', '..', 'src', 'gui_contextmenu.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}

check('buildMenuItems of null clip is empty', buildMenuItems(null, 1), []);

const clip = { id: 'c1', start: 2, duration: 3, end: 5 };
check('split enabled when playhead strictly inside the clip', buildMenuItems(clip, 3).find(i => i.id === 'split'),
  { id: 'split', label: 'Split at Playhead', enabled: true, reason: null });
check('split disabled at the clip start edge', buildMenuItems(clip, 2).find(i => i.id === 'split').enabled, false);
check('split disabled at the clip end edge', buildMenuItems(clip, 5).find(i => i.id === 'split').enabled, false);
check('split disabled outside the clip, with a reason', buildMenuItems(clip, 10).find(i => i.id === 'split'),
  { id: 'split', label: 'Split at Playhead', enabled: false, reason: 'move the playhead inside this clip first' });
check('duplicate/delete/properties are always enabled', buildMenuItems(clip, 10).filter(i => i.id !== 'split').every(i => i.enabled === true), true);
check('exactly 4 items in a fixed order', buildMenuItems(clip, 3).map(i => i.id), ['split', 'duplicate', 'delete', 'properties']);

check('actionFor split builds split_clip at the playhead', actionFor('split', clip, 3), { tool: 'split_clip', args: { clip_id: 'c1', at: 3 } });
check('actionFor duplicate lands right after the clip ends, same track implied', actionFor('duplicate', clip, 3), { tool: 'duplicate_clip', args: { clip_id: 'c1', start: 5 } });
check('actionFor delete', actionFor('delete', clip, 3), { tool: 'delete_clip', args: { clip_id: 'c1' } });
check('actionFor properties opens the inspector instead of calling a tool', actionFor('properties', clip, 3), { open: 'properties', clip_id: 'c1' });
check('actionFor an unknown id returns null', actionFor('nonsense', clip, 3), null);
check('actionFor with no clip returns null', actionFor('split', null, 3), null);
check('actionFor computes end from start+duration when end is absent', actionFor('duplicate', { id: 'c2', start: 1, duration: 2 }, 0), { tool: 'duplicate_clip', args: { clip_id: 'c2', start: 3 } });

check('menuPosition keeps the menu fully inside the viewport, bottom-right corner', menuPosition(190, 190, 200, 200, 50, 30), { left: 150, top: 170 });
check('menuPosition leaves an in-bounds click alone', menuPosition(10, 10, 200, 200, 50, 30), { left: 10, top: 10 });
check('menuPosition clamps to 0 when the menu is bigger than the viewport', menuPosition(5, 5, 40, 20, 50, 30), { left: 0, top: 0 });

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall context menu tests passed');
