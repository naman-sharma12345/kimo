// DEV/TEST ONLY: a stand-in for the Tauri bridge when studio.html is opened in a plain browser (tests, screenshots).
// It is never used inside the app (window.__TAURI__ exists there). Numbers that describe the real engine (version,
// tool and action counts) are the ones the real kimo-mcp 8.12.1 answered on 2026-11-06 (17 tools, 540 actions); everything else is fake.
const listeners = {};
const emit = (e, p) => (listeners[e] || []).forEach(f => f(p));
const sleep = ms => new Promise(r => setTimeout(r, ms));
const BIN = '/home/naman/.local/share/kimo/bin/kimo-mcp', FONTS = '/home/naman/.local/share/kimo/fonts';
const json = JSON.stringify({ mcpServers: { kimo: { command: BIN, args: [], env: { KIMO_FONTS: FONTS } } } }, null, 2);
const typed = JSON.stringify({ type: 'stdio', command: BIN, args: [], env: { KIMO_FONTS: FONTS } });
const snippets = {
  launch: { command: BIN, args: [], env: [['KIMO_FONTS', FONTS]] }, json,
  claude_add: `claude mcp add --env KIMO_FONTS=${FONTS} --transport stdio --scope user kimo -- ${BIN}`,
  claude_add_json: `claude mcp add-json --scope user kimo '${typed}'`,
  codex_add: `codex mcp add kimo --env KIMO_FONTS=${FONTS} -- ${BIN}`,
  codex_toml: `[mcp_servers.kimo]\ncommand = "${BIN}"\nargs = []\nenv = { KIMO_FONTS = "${FONTS}" }\n`,
  claude_doc: 'https://docs.claude.com/en/docs/claude-code/mcp.md', codex_doc: 'https://developers.openai.com/codex/mcp',
};
const providers = [
  { id: 'openai', name: 'OpenAI', wire: 'openai', base_url: 'https://api.openai.com/v1', default_model: 'gpt-4o-mini', key_page: 'https://platform.openai.com/api-keys' },
  { id: 'anthropic', name: 'Anthropic', wire: 'anthropic', base_url: 'https://api.anthropic.com', default_model: 'claude-sonnet-4-5', key_page: 'https://console.anthropic.com/settings/keys' },
  { id: 'gemini', name: 'Google Gemini', wire: 'openai', base_url: 'https://generativelanguage.googleapis.com/v1beta/openai', default_model: 'gemini-2.0-flash', key_page: 'https://aistudio.google.com/apikey' },
  { id: 'openrouter', name: 'OpenRouter', wire: 'openai', base_url: 'https://openrouter.ai/api/v1', default_model: 'openai/gpt-4o-mini', key_page: 'https://openrouter.ai/keys' },
  { id: 'groq', name: 'Groq', wire: 'openai', base_url: 'https://api.groq.com/openai/v1', default_model: 'llama-3.3-70b-versatile', key_page: 'https://console.groq.com/keys' },
  { id: 'custom', name: 'Custom (OpenAI-compatible, e.g. a local server)', wire: 'openai', base_url: 'http://127.0.0.1:11434/v1', default_model: '', key_page: '' },
];
const params = new URLSearchParams(location.search);
const M = {
  mcp: false, port: null, keys: {}, settings: { mcp_enabled: false, provider: 'openai', models: {}, base_urls: {}, remembered: [], setup_done: params.get('setup') !== 'show' },
  undo: [], cards: 0, snaps: [],
  edit: { caption: 54, warm: 0, cut: false, title: false, sat: 1 }, empty: params.get('project') === 'empty',
  viewerCalls: { frame: 0, draft: 0 }, viewerDelay: 280, failViewer: null,
  parts: [
    { id: 'ffmpeg', name: 'FFmpeg', note: 'decodes, cuts and encodes video', size: 79_000_000, optional: false, pinned: true, installed: params.get('setup') !== 'show' },
    { id: 'ffprobe', name: 'FFprobe', note: 'reads media details', size: 3_100_000, optional: false, pinned: true, installed: params.get('setup') !== 'show' },
    { id: 'whisper', name: 'Whisper (speech to text)', note: 'makes captions, offline', size: 4_600_000, optional: true, pinned: true, installed: params.get('setup') !== 'show' },
    { id: 'model', name: 'Speech model (whisper small, multilingual)', note: 'captions offline, on this computer', size: 190_085_487, optional: true, pinned: true, installed: false },
  ],
  agents: [
    { id: 'claude', name: 'Claude Code', cli: '/home/naman/.local/bin/claude', config_path: '/home/naman/.claude.json', linked: false, linked_command: null },
    { id: 'codex', name: 'Codex', cli: null, config_path: '/home/naman/.codex/config.toml', linked: false, linked_command: null },
  ],
};
// ---- viewer stand-in: frames painted on a canvas from the mock edit state (the app gets real engine frames) ----
const DUR = 18, FPS = 30;
const revOf = () => { const s = JSON.stringify(M.edit); let h = 2166136261; for (const c of s) h = Math.imul(h ^ c.charCodeAt(0), 16777619) >>> 0; return h.toString(16).padStart(8, '0') + '00000000'; };
function paint(cv, e, t) {
  const W = cv.width, H = cv.height, g = cv.getContext('2d');
  const warm = e.warm || 0, sat = e.sat || 1;
  const sky = g.createLinearGradient(0, 0, 0, H);
  sky.addColorStop(0, `hsl(${24 - warm * 60} ${Math.min(90, 46 * sat)}% ${18 + warm * 20}%)`);
  sky.addColorStop(.62, `hsl(${18 - warm * 40} ${Math.min(95, 68 * sat)}% ${48 + warm * 18}%)`);
  sky.addColorStop(1, `hsl(${30} ${Math.min(80, 40 * sat)}% ${14}%)`);
  g.fillStyle = sky; g.fillRect(0, 0, W, H);
  const sx = W * (.52 + .1 * Math.sin(t / 3)), sy = H * .58;
  const sun = g.createRadialGradient(sx, sy, 0, sx, sy, H * .5);
  sun.addColorStop(0, `rgba(255,${200 + warm * 40},150,.95)`); sun.addColorStop(.18, 'rgba(255,170,110,.55)'); sun.addColorStop(1, 'rgba(255,120,60,0)');
  g.fillStyle = sun; g.fillRect(0, 0, W, H);
  g.fillStyle = 'rgba(20,14,12,.92)';
  g.beginPath(); g.moveTo(0, H * .72);
  for (let x = 0; x <= W; x += W / 12) g.lineTo(x, H * (.70 + .06 * Math.sin(x / W * 7 + 1.3)));
  g.lineTo(W, H); g.lineTo(0, H); g.fill();
  g.beginPath(); g.ellipse(W * .3, H * .66, W * .045, H * .11, 0, 0, 7); g.fill();
  g.beginPath(); g.arc(W * .3, H * .5, H * .055, 0, 7); g.fill();
  if (e.title) { g.fillStyle = '#F4F0E8'; g.font = `italic ${Math.round(H * .11)}px serif`; g.textAlign = 'center'; g.fillText('Golden hour', W / 2, H * .2); }
  const fs = Math.round(H * (e.caption || 54) / 720 * 1.0);
  g.font = `600 ${fs}px sans-serif`; g.textAlign = 'center';
  const cap = t < 6 ? 'Welcome back to the studio' : t < 12 ? 'This light only lasts a minute' : 'So we shoot fast';
  const tw = g.measureText(cap).width;
  g.fillStyle = 'rgba(13,13,16,.62)'; g.beginPath(); g.roundRect(W / 2 - tw / 2 - fs * .4, H * .86 - fs, tw + fs * .8, fs * 1.4, fs * .3); g.fill();
  g.fillStyle = '#F4F0E8'; g.fillText(cap, W / 2, H * .86 + fs * .1);
}
const frameUrl = (e, t) => { const c = document.createElement('canvas'); c.width = 1280; c.height = 720; paint(c, e, t); return c.toDataURL('image/jpeg', .86); };
const VC = { frames: new Map(), lastRev: null, lastFrame: null, beforeRev: null, beforeFallback: null };
async function mockFrame({ time }) {
  M.viewerCalls.frame++;
  await sleep(M.viewerDelay);
  if (M.failViewer) throw M.failViewer;
  const rev = revOf(), empty = M.empty, base = { empty, rev, duration: empty ? 0 : DUR, fps: FPS, width: 1280, height: 720, before: null, changed: false };
  let frame = null;
  if (!empty) {
    const idx = Math.min(Math.round(Math.max(0, time) * FPS), DUR * FPS - 1), key = rev + ':' + idx;
    frame = VC.frames.get(key);
    if (frame) frame = { ...frame, cached: true };
    else { frame = { rev, index: idx, time: idx / FPS, url: frameUrl(M.edit, idx / FPS), sha256: key, width: 1280, height: 720, ms: 640 + (idx % 7) * 90, cached: false }; VC.frames.set(key, frame); }
  }
  if (VC.lastRev && VC.lastRev !== rev) { VC.beforeFallback = VC.lastFrame; VC.beforeRev = VC.lastRev; base.changed = true; }
  if (frame) { base.before = (VC.beforeRev && VC.frames.get(VC.beforeRev + ':' + frame.index)) || VC.beforeFallback; VC.lastFrame = frame; }
  VC.lastRev = rev;
  return { ...base, frame };
}
async function mockDraft({ start, seconds }) {
  M.viewerCalls.draft++;
  if (M.empty) throw 'out_of_range: the timeline is empty';
  const rev = revOf(), e = { ...M.edit }, len = Math.min(1.2, seconds), c = document.createElement('canvas'); c.width = 640; c.height = 360;
  const rec = new MediaRecorder(c.captureStream(30), { mimeType: 'video/webm' }), chunks = [];
  rec.ondataavailable = x => chunks.push(x.data);
  const t0 = performance.now(); let raf;
  const tick = () => { paint(c, e, start + (performance.now() - t0) / 1000); raf = requestAnimationFrame(tick); };
  tick(); rec.start();
  await sleep(len * 1000); rec.stop(); cancelAnimationFrame(raf);
  await new Promise(r => rec.onstop = r);
  return { rev, start, end: Math.min(DUR, start + len), fps: FPS, frames: Math.round(len * FPS), width: 640, height: 360, url: URL.createObjectURL(new Blob(chunks, { type: 'video/webm' })), ms: 2400, first_frame_ms: 300, cached: false };
}
let callN = 0;
async function engineCall(src, name, action, args, change, ms = 260) {
  const id = 'c' + (++callN), t = Date.now();
  emit('live-event', { source: src, t, ev: { tool_name: action || name, call_id: id, phase: 'start', target: change?.target || {}, payload: { args: { action, args } } } });
  await sleep(ms);
  emit('live-event', { source: src, t: Date.now(), ev: { tool_name: action || name, call_id: id, phase: 'complete', target: change?.target || {}, payload: { dur_ms: ms - 40, ...(change?.payload || {}) } } });
}
const applyEdit = a => { const e = M.edit; if (a === 'set_caption_style') e.caption = 72; else if (a === 'ripple_delete') e.cut = true; else if (a === 'set_temperature') e.warm = .12; else e.title = true; };
const handlers = {
  viewer_frame: mockFrame, viewer_draft: mockDraft,
  app_state: () => ({
    version: '9.0.0', engine: { state: 'running', pid: 48213, version: '8.12.1', tools: 17, actions: 540, bin: BIN },
    mcp_enabled: M.mcp, external_port: M.port, snippets, settings: M.settings, providers,
    keys: providers.map(p => ({ provider: p.id, set: !!M.keys[p.id], hint: M.keys[p.id] ? M.keys[p.id].slice(0, 3) + '…' + M.keys[p.id].slice(-4) : null, remembered: M.settings.remembered.includes(p.id) })),
    key_store: 'Secret Service keyring', data_dir: '/home/naman/.local/share/dev.kimo.app', os: 'linux',
  }),
  engine_restart: () => handlers.app_state().engine,
  mcp_set: async ({ enabled }) => {
    await sleep(350);
    M.mcp = M.settings.mcp_enabled = enabled; M.port = enabled ? 7421 : null;
    return enabled ? { enabled, external_port: 7421, snippets, probe: { ok: true, version: '8.12.1', protocol: '2024-11-05', tools: 17, actions: 540, ms: 41 } } : { enabled };
  },
  agents_detect: () => M.agents,
  agent_add: async ({ id }) => { const a = M.agents.find(x => x.id === id); a.linked = true; a.linked_command = BIN; return id === 'claude' ? 'Added stdio MCP server kimo to user config' : 'wrote [mcp_servers.kimo] to /home/naman/.codex/config.toml'; },
  key_set: ({ provider, key, remember }) => { M.keys[provider] = key; M.settings.provider = provider; M.settings.remembered = M.settings.remembered.filter(p => p !== provider); if (remember) M.settings.remembered.push(provider); return { provider, set: true, hint: key.slice(0, 3) + '…' + key.slice(-4), remembered: remember, store: 'Secret Service keyring', note: null }; },
  key_clear: ({ provider }) => { delete M.keys[provider]; return { provider, set: false }; },
  key_test: async ({ provider }) => { await sleep(300); if (!M.keys[provider]) throw 'Paste a key first.'; return 'Connected. The key works (54 models visible).'; },
  settings_set: ({ provider, model, baseUrl }) => { M.settings.provider = provider; if (model != null) M.settings.models[provider] = model; if (baseUrl != null) M.settings.base_urls[provider] = baseUrl; },
  chat_send: async ({ text }) => {
    if (!M.keys[M.settings.provider]) throw 'Add your OpenAI key in Connections first, or link Claude Code / Codex and edit from there.';
    const gui = { kind: 'gui' };
    emit('chat-event', { kind: 'thinking', step: 0 }); await sleep(400);
    const plan = /caption/i.test(text) ? [['text', 'set_caption_style', { size: 72, track: 'T1' }, 'Caption size 54 → 72', { target: { track_id: 'T1', param: 'font_size' }, payload: { from: 54, to: 72 } }]]
      : /pause|cut/i.test(text) ? [['audio', 'detect_silence', { from: 11.5, to: 13 }, null], ['timeline', 'ripple_delete', { clip_id: 'v1_c3', start: 12.0, end: 12.8 }, 'Cut 0.8 s pause at 0:12', { target: { track_id: 'V1', clip_id: 'v1_c3' }, payload: { s: 12.0, e: 12.8, ripple: true } }]]
      : [['color_grade', 'set_temperature', { value: 0.12 }, 'Warmer colour', { target: { clip_id: 'v1_c1', param: 'temperature' }, payload: { from: 0, to: 0.12 } }]];
    for (const [tool, action, args, label, change] of plan) {
      const call = 'tc' + (++callN), summary = `${tool} · ${action} · ` + Object.entries(args).map(([k, v]) => `${k}=${v}`).join(', ');
      emit('chat-event', { kind: 'tool_start', call, tool, args: { action, args }, summary });
      await engineCall(gui, tool, action, args, change);
      let card = null;
      if (label) { M.snaps.push(JSON.stringify(M.edit)); applyEdit(action);
        M.undo.push(label); card = { id: ++M.cards, depth: M.undo.length, label, tool, summary, undone: false }; }
      emit('chat-event', { kind: 'tool_end', call, tool, ok: true, result: 'ok', card });
    }
    emit('chat-event', { kind: 'thinking', step: 1 }); await sleep(300);
    const reply = /caption/i.test(text) ? 'Captions are bigger: font size 54 → 72 on the caption track.' : /pause|cut/i.test(text) ? 'Cut the 0.8 s pause at 0:12 and closed the gap.' : 'Warmed the grade slightly (+0.12 temperature).';
    emit('chat-event', { kind: 'reply', text: reply });
    return reply;
  },
  chat_undo: async ({ card }) => {
    await engineCall({ kind: 'gui' }, 'project', 'undo', {}, { payload: {} }, 120);
    const depth = card; const steps = M.undo.length - depth + 1; M.undo.length = depth - 1;
    M.edit = JSON.parse(M.snaps[depth - 1]); M.snaps.length = depth - 1;
    return { steps, undone: Array.from({ length: steps }, (_, i) => depth + i) };
  },
  chat_reset: () => { M.undo = []; M.cards = 0; },
  project_open: async ({ path }) => { await sleep(200); return 'loaded ' + path; },
  open_spectator: () => null,
  setup_status: () => ({ done: M.settings.setup_done, parts: M.parts, dir: '/home/naman/.local/share/dev.kimo.app' }),
  setup_start: async () => { for (const p of M.parts) { if (p.installed) continue; for (let d = 0; d <= p.size; d += p.size / 10) { emit('setup-progress', { id: p.id, done: Math.round(d), total: p.size, bytes_per_s: 11.4e6, eta_s: (p.size - d) / 11.4e6, phase: 'downloading', mirror: 0 }); await sleep(30); } p.installed = true; emit('setup-progress', { id: p.id, done: p.size, total: p.size, bytes_per_s: 0, eta_s: 0, phase: 'done', mirror: 0 }); } M.settings.setup_done = true; return { ready: true, results: [] }; },
  setup_pause: () => null,
  setup_skip: () => { M.settings.setup_done = true; },
};
export const bridge = {
  kind: 'mock',
  invoke: async (c, a) => { const h = handlers[c]; if (!h) throw 'unknown command ' + c; return h(a || {}); },
  listen: (e, f) => { (listeners[e] = listeners[e] || []).push(f); },
};
// test/screenshot hooks
window.__mock = { emit, M, engineCall };
