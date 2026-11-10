// Demo replay (dev only — lives outside ui/, never shipped in the bundle).
// Turns demo/scenarios.json into the same engine events the shell forwards,
// so the simulated feed exercises exactly the production ingestion path.
//   ?feed=sim                      cycle all 16 scenarios live
//   ?feed=sim&scenario=m05         one scenario, live
//   ?feed=sim&scenario=m05&still=1 jump to the scenario's end state, clock frozen
const WIPE = new Set(['color', 'grading', 'effects', 'edit']);   // viewer shows the running tool's result wiping in
const clone = o => JSON.parse(JSON.stringify(o));
// The view as it stood before the scenario's changes landed: changed inspector
// values show their old value, so the live replay can flip them when the call ends.
function before(views) {
  const v = clone(views);
  for (const p of Object.values(v)) for (const b of (p && p.blocks) || []) if (b.from != null) { b.v = b.from; delete b.from; }
  return v;
}
export function eventsFor(s, { still = false, live = true } = {}) {
  const ev = [];
  ev.push({ type: 'session', reset: true, project: s.project, agent: s.agent, elapsed_s: s.elapsed, still });
  ev.push(s.activity ? { type: 'mode', mode: s.mode, activity: s.activity } : { type: 'mode', mode: s.mode });
  if (Object.keys(s.stats).length) ev.push({ type: 'stats', ...s.stats });
  const reactive = live && !still;
  ev.push({ type: 'view', mode: s.mode, data: reactive ? before(s.views) : s.views });
  const ph = s.views.tl && s.views.tl.playhead, n = s.calls.length;
  s.calls.forEach(([name, args, dur], i) => {
    const id = `${s.id}-c${i}`;
    const last = i === n - 1;
    const st = { type: 'tool_start', id, name, args };
    if (last && s.final.count) st.count = s.final.count;
    if (last && s.final.progress) st.progress = s.final.progress;
    if (last && s.final.running) st.elapsed_ms = dur;  // how long it has been running at this moment
    ev.push(st);
    if (reactive && ph != null) ev.push({ type: 'view', mode: s.mode, data: { tl: { playhead: +(ph - (n - 1 - i) * 0.35).toFixed(2) } } });
    if (reactive && last) {
      // the running call drives the viewport: progress events, ~16 steps
      const steps = 24, span = 2600;   // long enough to watch
      for (let k = 1; k <= steps; k++) {
        const p = +(k / steps).toFixed(3);
        ev.push({ type: 'tool_progress', id, progress: s.final.progress ? s.final.progress : { pct: Math.round(p * 100) }, _wait: span / steps });
        if (WIPE.has(s.mode)) ev.push({ type: 'view', mode: s.mode, data: { viewer: { wipe: p } } });
      }
      ev.push({ type: 'view', mode: s.mode, data: s.views });   // state lands: inspector values flip a -> b
    }
    if (!(last && s.final.running)) ev.push({ type: 'tool_end', id, name, dur_ms: dur, status: 'ok', _wait: live && !last ? Math.min(dur, 900) : 0 });
  });
  return ev;
}

export async function play(onEvent, params) {
  const url = new URL('./scenarios.json', import.meta.url);
  const all = await (await fetch(url)).json();
  const pick = params.get('scenario');
  const still = params.get('still') === '1';
  const list = pick ? all.filter(s => s.id === pick) : all;
  if (still) { for (const e of eventsFor(list[0], { still: true, live: false })) onEvent(e); return; }
  const gap = +(params.get('gap') || 380), hold = +(params.get('hold') || 3500);
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  for (;;) {
    for (const s of list) {
      for (const e of eventsFor(s)) {
        const w = e._wait; delete e._wait;
        if (e.type === 'tool_start') await sleep(gap);
        if (w) await sleep(w);
        onEvent(e);
      }
      await sleep(hold);
    }
    if (params.get('loop') === '0') return;
  }
}
