// Event bridge: ONE ingestion path (onEvent) behind a thin transport
// abstraction. Bindings, in priority order:
//   1. Tauri IPC     inside the shell: events arrive from the loopback
//                    TCP/JSONL listener in src/main.rs (`engine-event`).
//   2. localhost SSE plain-browser development (tools/sse_feed.py).
//                    Forced with ?feed=sse, custom port with ?sse=PORT.
//   3. simulated     ?feed=sim, or nothing reachable: the demo replay from
//                    ../demo (dev checkout only; it is not part of the shipped
//                    ui/ bundle). Simulated data always labels itself.
// The app makes no network calls beyond loopback.

export function connect(onEvent, onStatus) {
  if (window.__TAURI__) {
    // An engine event is itself proof the link is up: the shell's one-shot
    // "live" status can fire before the webview has registered its listener.
    // After the engine has been live once, the shell going back to 'listening' is a drop: the engine's writer
    // reconnects on its own backoff (kimo-mcp spectator.rs), so the shell only listens and the pill says so.
    // The engine keeps no replay buffer (spectator.rs: events from while the GUI was gone are not replayed).
    let live = false, was = false;
    window.__TAURI__.event.listen('engine-event', e => { if (!live) { live = was = true; onStatus('live', 'live'); } onEvent(e.payload); });
    window.__TAURI__.event.listen('spectator-status', e => { const s = String(e.payload); live = s === 'live'; if (live) was = true; onStatus(live ? 'live' : 'down', !live && was && s.startsWith('listening') ? 'reconnecting' : s); });
    onStatus('down', 'listening');
    return { kind: 'tauri' };
  }
  const params = new URLSearchParams(location.search);
  const forced = params.get('feed');
  const sim = () => { onStatus('sim', 'simulated'); startSimulated(onEvent, params, onStatus); return { kind: 'sim' }; };
  if (forced === 'sim') return sim();
  if (forced === 'off') { onStatus('down', 'no feed'); return { kind: 'off' }; }   // tests inject through window.__kimo.apply

  // SSE: the browser's EventSource retries a dropped stream by itself (the feed sets `retry:`) and sends
  // Last-Event-ID, so the feed replays what was missed (tools/sse_feed.py keeps a ring). Only when the browser
  // gives up (readyState CLOSED: refused / bad status) does one setTimeout reopen it, doubling up to 8 s, and
  // only while down: a connected or idle spectator holds no timer.
  const port = params.get('sse') || '7422';
  let opened = false, es = null, wait = 500, lastId = null;
  const open = () => {
    es = new EventSource(`http://127.0.0.1:${port}/events${lastId != null ? `?last=${encodeURIComponent(lastId)}` : ''}`);
    es.onopen = () => { opened = true; wait = 500; onStatus('live', `sse :${port}`); };
    es.onmessage = e => { if (e.lastEventId) lastId = e.lastEventId; try { onEvent(JSON.parse(e.data)); } catch {} };
    es.onerror = () => {
      if (!opened) { es.close(); if (forced === 'sse') onStatus('down', `sse :${port} refused`); else sim(); return; }
      onStatus('down', 'reconnecting');
      if (es.readyState === EventSource.CLOSED) { setTimeout(open, wait); wait = Math.min(wait * 2, 8000); }
    };
  };
  open();
  return { kind: 'sse', port };
}

async function startSimulated(onEvent, params, onStatus) {
  try {
    const mod = await import('../demo/replay.js');
    mod.play(onEvent, params);
  } catch (err) {
    onStatus('down', 'no feed');
  }
}
