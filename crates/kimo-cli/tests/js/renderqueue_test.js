// v7.5: pure-logic test for kimo gui's render queue panel (gui_renderqueue.js), headless (no DOM, no
// browser -- run directly under Node). This is the exact module gui_index.html's queue panel calls (served
// at GET /gui_renderqueue.js, same bytes), so a pass here is a pass for the browser's own panel too.
//
//     node crates/kimo-cli/tests/js/renderqueue_test.js      # exit 1 on failure
const assert = require('assert');
const path = require('path');
const { addJob, updateJob, removeJob, activeJobIds, isFinished } = require(path.join(__dirname, '..', '..', 'src', 'gui_renderqueue.js'));

let failures = 0;
function check(name, actual, expected) {
  try { assert.deepStrictEqual(actual, expected); console.log(`ok - ${name}`); }
  catch (e) { failures++; console.log(`NOT OK - ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); }
}

check('addJob appends a queued entry with 0%', addJob([], 'j1', 'out.mp4'), [{ id: 'j1', path: 'out.mp4', state: 'queued', pct: 0, note: '' }]);
check('addJob onto an existing list keeps the earlier jobs', addJob([{ id: 'j1', path: 'a.mp4', state: 'done', pct: 100, note: '' }], 'j2', 'b.mp4'),
  [{ id: 'j1', path: 'a.mp4', state: 'done', pct: 100, note: '' }, { id: 'j2', path: 'b.mp4', state: 'queued', pct: 0, note: '' }]);
check('addJob onto undefined starts a fresh list', addJob(undefined, 'j1', 'x.mp4'), [{ id: 'j1', path: 'x.mp4', state: 'queued', pct: 0, note: '' }]);

const jobs = addJob([], 'j1', 'out.mp4');
check('updateJob reshapes a chunked job_status into state/pct/note', updateJob(jobs, 'j1', { state: 'running', chunks: { done: 3, total: 10 } }),
  [{ id: 'j1', path: 'out.mp4', state: 'running', pct: 30, note: '' }]);
check('updateJob falls back to frames when there are no chunks', updateJob(jobs, 'j1', { state: 'running', frames_done: 25, frames_total: 100 }),
  [{ id: 'j1', path: 'out.mp4', state: 'running', pct: 25, note: '' }]);
check('updateJob carries an eta note', updateJob(jobs, 'j1', { state: 'running', chunks: { done: 1, total: 4 }, eta: { seconds: 12.4 } }),
  [{ id: 'j1', path: 'out.mp4', state: 'running', pct: 25, note: '~13s left' }]);
check('updateJob carries an error note on a failed job', updateJob(jobs, 'j1', { state: 'failed', error: 'disk full' }),
  [{ id: 'j1', path: 'out.mp4', state: 'failed', pct: 0, note: 'error: disk full' }]);
check('updateJob only touches the named job id, others pass through unchanged', updateJob(addJob(jobs, 'j2', 'two.mp4'), 'j2', { state: 'done', chunks: { done: 5, total: 5 } }),
  [{ id: 'j1', path: 'out.mp4', state: 'queued', pct: 0, note: '' }, { id: 'j2', path: 'two.mp4', state: 'done', pct: 100, note: '' }]);

check('removeJob drops exactly the named job', removeJob(addJob(jobs, 'j2', 'two.mp4'), 'j1'), [{ id: 'j2', path: 'two.mp4', state: 'queued', pct: 0, note: '' }]);
check('removeJob of an unknown id changes nothing', removeJob(jobs, 'nope'), jobs);

check('isFinished is true for done/failed/cancelled', [isFinished({ state: 'done' }), isFinished({ state: 'failed' }), isFinished({ state: 'cancelled' })], [true, true, true]);
check('isFinished is false for queued/running/cancelling', [isFinished({ state: 'queued' }), isFinished({ state: 'running' }), isFinished({ state: 'cancelling' })], [false, false, false]);
check('isFinished of null/undefined is false, not a crash', [isFinished(null), isFinished(undefined)], [false, false]);

const mixed = [{ id: 'a', state: 'done' }, { id: 'b', state: 'running' }, { id: 'c', state: 'queued' }, { id: 'd', state: 'failed' }];
check('activeJobIds lists only the still-unfinished jobs', activeJobIds(mixed), ['b', 'c']);
check('activeJobIds of an empty/undefined list is empty', [activeJobIds([]), activeJobIds(undefined)], [[], []]);

if (failures > 0) { console.log(`\n${failures} failure(s)`); process.exit(1); }
console.log('\nall render queue tests passed');
