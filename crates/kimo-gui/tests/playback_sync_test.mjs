// The viewer's A/V sync maths (ui/playback.js, window.KimoAV) against the same cases as the engine's contract
// (crates/kimo-core/src/preview_stream.rs tests): the audio clock drives the picture, audio chunks join back to back,
// runs at 2x / reverse land on the frames the engine renders, J/K/L and the loop region.
// Run: node crates/kimo-gui/tests/playback_sync_test.mjs
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import assert from 'node:assert/strict';

const src = readFileSync(fileURLToPath(new URL('../ui/playback.js', import.meta.url)), 'utf8');
const ctx = { window: {} }; vm.createContext(ctx); vm.runInContext(src, ctx);
const AV = ctx.window.KimoAV;
const near = (a, b, eps = 1e-9) => assert.ok(Math.abs(a - b) < eps, `${a} != ${b}`);
let n = 0; const test = (name, f) => { f(); n++; console.log('ok', name); };

test('the audio clock drives the picture', () => {
  const c = AV.clock(10, 2, 1, 0.04);
  assert.equal(AV.timelineAt(c, 2), 10);
  near(AV.timelineAt(c, 3.04), 11);
  near(AV.audioAt(c, 11), 3);
  assert.equal(AV.frameDue(c, 2, 25), 250);
  assert.equal(AV.frameDue(c, 2.04 + 0.039, 25), 250);
  assert.equal(AV.frameDue(c, 2.04 + 0.04, 25), 251);
  assert.equal(AV.frameDue(c, 3.04, 25), 275);
  const f = AV.clock(0, 0, 2, 0); near(AV.timelineAt(f, 1.5), 3); near(AV.audioAt(f, 3), 1.5);
  near(AV.driftFrames(c, 3.06, 274, 25), 1);
  near(AV.driftFrames(c, 3.06, 275, 25), 0);
  assert.equal(AV.clock(0, 0, NaN, -1).rate, 1); assert.equal(AV.clock(0, 0, 1, -1).latency, 0);
});

test('audio chunks are scheduled back to back and join mid-chunk', () => {
  const c = AV.clock(10.3, 5, 1, 0);
  const a = AV.chunkSchedule(c, 10, 1, 5);
  near(a.when, 5); near(a.offset, 0.3); near(a.left, 0.7);
  const b = AV.chunkSchedule(c, 11, 1, 5);
  near(b.when, a.when + a.left); assert.equal(b.offset, 0);
  const late = AV.chunkSchedule(c, 11, 1, 5.95);
  near(late.when, 5.95); near(late.offset, 0.25); near(late.left, 0.75);
  assert.equal(AV.chunkSchedule(c, 10, 1, 5.8), null);
  assert.equal(AV.chunkSchedule(AV.clock(0, 0, -1, 0), 0, 1, 0), null);
});

test('a frame on a 25 fps timeline stays within one frame of the sound over ten minutes', () => {
  const c = AV.clock(0, 100, 1, 0.02), fps = 25;
  for (let i = 0; i < 25 * 600; i += 7) {
    const a = 100 + 0.02 + i / fps + 0.5 / fps; // mid-frame on the device clock
    const due = AV.frameDue(c, a, fps);
    assert.equal(due, i, `frame ${i}`);
    assert.ok(Math.abs(AV.driftFrames(c, a, due, fps)) < 1e-6);
  }
});

test('fast and reverse runs land on the frames the engine renders', () => {
  const fwd2 = { start: 100, dir: 1, stride: 2 }, back4 = { start: 100, dir: -1, stride: 4 };
  assert.equal(AV.onRun(fwd2, 105), 104); assert.equal(AV.onRun(fwd2, 106), 106); assert.equal(AV.onRun(fwd2, 90), 100);
  assert.equal(AV.onRun(back4, 97), 100); assert.equal(AV.onRun(back4, 96), 96); assert.equal(AV.onRun(back4, 91), 92);
  assert.equal(AV.onRun({ start: 7, dir: 1, stride: 1 }, 42), 42);
});

test('J / K / L shuttle', () => {
  assert.equal(AV.shuttle(0, 'l'), 1); assert.equal(AV.shuttle(1, 'l'), 2); assert.equal(AV.shuttle(2, 'l'), 4); assert.equal(AV.shuttle(4, 'l'), 4);
  assert.equal(AV.shuttle(0, 'j'), -1); assert.equal(AV.shuttle(-1, 'j'), -2); assert.equal(AV.shuttle(-4, 'j'), -4);
  assert.equal(AV.shuttle(2, 'j'), -1, 'the other way first goes to normal speed backwards');
  assert.equal(AV.shuttle(-2, 'l'), 1);
  assert.equal(AV.shuttle(2, 'k'), 0);
});

test('the loop region wraps the playhead', () => {
  const L = { on: true, in: 2, out: 5 };
  assert.equal(AV.wrap(3, L, 10), 3); assert.equal(AV.wrap(5, L, 10), 2); assert.equal(AV.wrap(1, L, 10), 2);
  assert.equal(AV.wrap(10, { on: true, in: null, out: null }, 10), 0, 'no in / out: the whole timeline');
  assert.equal(AV.wrap(10, { on: false }, 10), null, 'loop off: stop at the end');
  assert.equal(AV.wrap(4, { on: false }, 10), 4);
});

console.log(`${n} tests passed`);
