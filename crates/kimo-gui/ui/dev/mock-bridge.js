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
  undo: [], cards: 0,
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
let callN = 0;
async function engineCall(src, name, action, args, change, ms = 260) {
  const id = 'c' + (++callN), t = Date.now();
  emit('live-event', { source: src, t, ev: { tool_name: action || name, call_id: id, phase: 'start', target: change?.target || {}, payload: { args: { action, args } } } });
  await sleep(ms);
  emit('live-event', { source: src, t: Date.now(), ev: { tool_name: action || name, call_id: id, phase: 'complete', target: change?.target || {}, payload: { dur_ms: ms - 40, ...(change?.payload || {}) } } });
}
const handlers = {
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
      if (label) { M.undo.push(label); card = { id: ++M.cards, depth: M.undo.length, label, tool, summary, undone: false }; }
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
