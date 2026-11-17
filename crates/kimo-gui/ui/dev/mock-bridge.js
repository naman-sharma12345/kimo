// DEV/TEST ONLY: a stand-in for the Tauri bridge when studio.html is opened in a plain browser (tests, screenshots,
// design reviews). It is never used inside the app (window.__TAURI__ exists there). It speaks the same commands and
// events as crates/kimo-gui/src/main.rs, with a small in-memory project, so every screen can be seen populated.
// URL switches: ?stream=1 (the real-time preview commands answer) · ?project=empty (no media, empty timeline) · ?setup=show (first-run downloads) · ?welcome=show ·
// ?agents=none (no agent CLI installed) · ?recover=show (crash-recovery prompt) · ?busy=1 (an agent edits on load).
const listeners = {};
const emit = (e, p) => (listeners[e] || []).forEach(f => f(p));
const sleep = ms => new Promise(r => setTimeout(r, ms));
const params = new URLSearchParams(location.search);
const HOME = 'C:\\Users\\you', DATA = HOME + '\\AppData\\Roaming\\tech.getkimo.app';
const BIN = 'C:\\Program Files\\kimo\\kimo-gui.exe';
const providers = [
  { id: 'openai', name: 'OpenAI', wire: 'openai', base_url: 'https://api.openai.com/v1', default_model: 'gpt-4o-mini', key_page: 'https://platform.openai.com/api-keys' },
  { id: 'anthropic', name: 'Anthropic', wire: 'anthropic', base_url: 'https://api.anthropic.com', default_model: 'claude-sonnet-4-5', key_page: 'https://console.anthropic.com/settings/keys' },
  { id: 'gemini', name: 'Google Gemini', wire: 'openai', base_url: 'https://generativelanguage.googleapis.com/v1beta/openai', default_model: 'gemini-2.0-flash', key_page: 'https://aistudio.google.com/apikey' },
  { id: 'custom', name: 'Custom (OpenAI-compatible, e.g. a local server)', wire: 'openai', base_url: 'http://127.0.0.1:11434/v1', default_model: '', key_page: '' },
];
const noAgents = params.get('agents') === 'none';
const M = {
  mcp: false, keys: {}, dirty: false, path: null,
  settings: {
    agent: 'claude-code', agent_models: { 'claude-code': 'sonnet' }, agent_efforts: {}, provider: 'openai', models: {}, base_urls: {}, remembered: [],
    onboarded: params.get('welcome') !== 'show', mcp_enabled: false,
    recent: [HOME + '\\Videos\\kimo\\Studio tour.kimo', HOME + '\\Videos\\kimo\\Podcast ep 12.kimo', HOME + '\\Documents\\Launch teaser.kimo'],
  },
  parts: [
    { id: 'ffmpeg', name: 'FFmpeg', note: 'decodes, cuts and encodes video', size: 79_000_000, optional: false, pinned: true, satisfied: params.get('setup') === 'show' ? null : 'system' },
    { id: 'ffprobe', name: 'FFprobe', note: 'reads media details', size: 3_100_000, optional: false, pinned: true, satisfied: params.get('setup') === 'show' ? null : 'system' },
    { id: 'whisper', name: 'Whisper (speech to text)', note: 'makes captions, offline', size: 4_600_000, optional: true, pinned: true, satisfied: null },
    { id: 'model', name: 'Speech model (whisper small)', note: 'captions offline, on this computer', size: 190_085_487, optional: true, pinned: true, satisfied: null },
  ],
  cli: [
    { id: 'claude-code', name: 'Claude Code', plan: 'Claude Pro or Max', installed: !noAgents, version: '2.3.1', signed_in: noAgents ? null : true, sign_in_note: 'Signed in', registered: true, install_url: 'https://docs.claude.com/en/docs/claude-code/setup' },
    { id: 'codex', name: 'Codex', plan: 'ChatGPT Plus or Pro', installed: !noAgents, version: '0.61.0', signed_in: noAgents ? null : null, sign_in_note: 'Sign-in not checked yet', registered: false, install_url: 'https://developers.openai.com/codex/cli' },
    { id: 'antigravity', name: 'Antigravity', plan: 'Google AI Pro or Ultra', installed: false, version: null, signed_in: null, sign_in_note: '', registered: false, install_url: 'https://antigravity.google' },
  ],
  models: {
    'claude-code': { models: [{ id: 'opus', label: 'Opus (newest)', efforts: [] }, { id: 'sonnet', label: 'Sonnet', efforts: [] }, { id: 'haiku', label: 'Haiku', efforts: [] }], efforts: ['low', 'medium', 'high'], source: 'Claude Code model aliases' },
    codex: { models: [{ id: 'gpt-5.1-codex', label: 'GPT-5.1 Codex', efforts: ['low', 'medium', 'high'], default_effort: 'medium' }, { id: 'gpt-5.1-codex-mini', label: 'GPT-5.1 Codex mini', efforts: ['medium', 'high'] }], efforts: ['low', 'medium', 'high'], source: 'codex debug models' },
    antigravity: { models: [{ id: 'gemini-3-pro', label: 'Gemini 3 Pro' }, { id: 'gemini-3-flash', label: 'Gemini 3 Flash' }], efforts: [], source: 'agy models' },
  },
  changes: [], undo: [], redo: [], attached: {}, edit: { warm: 0, sat: 1, caption: 54, title: false },
};

// ---------- the project ----------
const empty = params.get('project') === 'empty';
const ASSETS = empty ? {} : {
  a1: { id: 'a1', path: HOME + '\\Videos\\golden-hour-wide.mp4', duration: 42.6, width: 3840, height: 2160, fps: 29.97, hue: 24 },
  a2: { id: 'a2', path: HOME + '\\Videos\\interview-cam-a.mp4', duration: 312.4, width: 1920, height: 1080, fps: 25, hue: 210 },
  a3: { id: 'a3', path: HOME + '\\Videos\\interview-cam-b.mp4', duration: 311.9, width: 1920, height: 1080, fps: 25, hue: 190 },
  a4: { id: 'a4', path: HOME + '\\Videos\\city-drone.mov', duration: 18.2, width: 3840, height: 2160, fps: 59.94, hue: 260 },
  a5: { id: 'a5', path: HOME + '\\Music\\warm-tape-loop.wav', duration: 96.0 },
  a6: { id: 'a6', path: HOME + '\\Pictures\\logo-lockup.png', duration: 5, width: 2000, height: 2000 },
};
const clip = (id, start, duration, source, more = {}) => ({ id, start, duration, speed: 1, in_point: 0, source, effects: [], ...more });
const project = {
  fps: 30, width: 1920, height: 1080, assets: ASSETS,
  markers: empty ? [] : [{ time: 6.2, name: 'Hook ends' }, { time: 15.4, name: 'Logo' }],
  tracks: empty ? [{ id: 'A1', name: 'Music', clips: [] }, { id: 'V1', name: 'Video 1', clips: [] }] : [
    { id: 'A1', name: 'Music', clips: [clip('a1_c1', 0, 18.5, { type: 'audio', asset_id: 'a5' }, { volume: .7 })] },
    { id: 'V1', name: 'Main', clips: [clip('v1_c1', 0, 6.0, { type: 'media', asset_id: 'a1' }), clip('v1_c2', 6.0, 5.5, { type: 'media', asset_id: 'a2' }, { in_point: 31.2, transition_in: { kind: 'dissolve', duration: .5 } }), clip('v1_c3', 11.5, 4.0, { type: 'media', asset_id: 'a3' }, { in_point: 64 }), clip('v1_c4', 15.5, 3.0, { type: 'media', asset_id: 'a6' })] },
    { id: 'V2', name: 'B-roll', clips: [clip('v2_c1', 6.0, 4.2, { type: 'media', asset_id: 'a4' }, { effects: [{ id: 'fx1', kind: 'film_grain' }] })] },
    { id: 'T1', name: 'Titles', clips: [clip('t_c1', 0.4, 3.2, { type: 'text', text: 'Golden hour', size: 96, color: [1, 1, 1, 1] }), clip('t_c2', 15.2, 3, { type: 'text', text: 'getkimo.tech', size: 72, color: [1, .8, .4, 1] })] },
  ],
};
const findClip = id => { for (const t of project.tracks) { const c = t.clips.find(c => c.id === id); if (c) return [t, c]; } return [null, null]; };
const snapshot = () => JSON.stringify(project.tracks) + '|' + JSON.stringify(project.markers) + '|' + JSON.stringify(M.edit);
const restore = s => { const [t, m, e] = s.split('|'); project.tracks = JSON.parse(t); project.markers = JSON.parse(m); M.edit = JSON.parse(e); };
const DUR = () => Math.max(1, ...project.tracks.flatMap(t => t.clips.map(c => c.start + c.duration / (c.speed || 1))));

// ---------- viewer stand-in: frames painted on a canvas (the app gets real engine frames) ----------
const FPS = 30;
const revOf = () => { const s = snapshot(); let h = 2166136261; for (const c of s) h = Math.imul(h ^ c.charCodeAt(0), 16777619) >>> 0; return h.toString(16).padStart(8, '0') + '00000000'; };
function paint(cv, t, hue = 24) {
  const W = cv.width, H = cv.height, g = cv.getContext('2d'), e = M.edit;
  const warm = e.warm || 0, sat = e.sat || 1;
  const sky = g.createLinearGradient(0, 0, 0, H);
  sky.addColorStop(0, `hsl(${hue - warm * 60} ${Math.min(90, 46 * sat)}% ${18 + warm * 20}%)`);
  sky.addColorStop(.62, `hsl(${hue - 6 - warm * 40} ${Math.min(95, 68 * sat)}% ${48 + warm * 18}%)`);
  sky.addColorStop(1, `hsl(${hue + 6} ${Math.min(80, 40 * sat)}% 14%)`);
  g.fillStyle = sky; g.fillRect(0, 0, W, H);
  const sx = W * (.52 + .1 * Math.sin(t / 3)), sy = H * .58;
  const sun = g.createRadialGradient(sx, sy, 0, sx, sy, H * .5);
  sun.addColorStop(0, 'rgba(255,225,170,.95)'); sun.addColorStop(.18, 'rgba(255,170,110,.5)'); sun.addColorStop(1, 'rgba(255,120,60,0)');
  g.fillStyle = sun; g.fillRect(0, 0, W, H);
  g.fillStyle = 'rgba(14,12,12,.92)';
  g.beginPath(); g.moveTo(0, H * .72);
  for (let x = 0; x <= W; x += W / 12) g.lineTo(x, H * (.70 + .06 * Math.sin(x / W * 7 + 1.3)));
  g.lineTo(W, H); g.lineTo(0, H); g.fill();
  g.beginPath(); g.ellipse(W * .3, H * .66, W * .045, H * .11, 0, 0, 7); g.fill();
  g.beginPath(); g.arc(W * .3, H * .5, H * .055, 0, 7); g.fill();
  const title = project.tracks.flatMap(t => t.clips).find(c => c.source.type === 'text' && t >= c.start && t < c.start + c.duration);
  if (title) { g.fillStyle = '#F4F1EA'; g.font = `600 ${Math.round(H * .09)}px sans-serif`; g.textAlign = 'center'; g.fillText(title.source.text, W / 2, H * .22); }
  const fs = Math.round(H * (e.caption || 54) / 720);
  g.font = `600 ${fs}px sans-serif`; g.textAlign = 'center';
  const cap = t < 6 ? 'Welcome back to the studio' : t < 12 ? 'This light only lasts a minute' : 'So we shoot fast';
  const tw = g.measureText(cap).width;
  g.fillStyle = 'rgba(10,10,12,.6)'; g.beginPath(); g.roundRect(W / 2 - tw / 2 - fs * .4, H * .86 - fs, tw + fs * .8, fs * 1.4, fs * .3); g.fill();
  g.fillStyle = '#F4F1EA'; g.fillText(cap, W / 2, H * .86 + fs * .1);
}
const frameUrl = t => { const c = document.createElement('canvas'); c.width = 1280; c.height = 720; paint(c, t); return c.toDataURL('image/jpeg', .86); };
const VC = { frames: new Map(), lastRev: null, lastFrame: null, beforeRev: null, beforeFallback: null };
async function mockFrame({ time }) {
  await sleep(200);
  const rev = revOf(), isEmpty = !project.tracks.some(t => t.clips.length), dur = DUR();
  const base = { empty: isEmpty, rev, duration: isEmpty ? 0 : dur, fps: FPS, width: 1280, height: 720, before: null, changed: false };
  let frame = null;
  if (!isEmpty) {
    const idx = Math.min(Math.round(Math.max(0, time) * FPS), Math.floor(dur * FPS) - 1), key = rev + ':' + idx;
    frame = VC.frames.get(key);
    if (frame) frame = { ...frame, cached: true };
    else { frame = { rev, index: idx, time: idx / FPS, url: frameUrl(idx / FPS), sha256: key, width: 1280, height: 720, ms: 180 + (idx % 7) * 40, cached: false }; VC.frames.set(key, frame); }
  }
  if (VC.lastRev && VC.lastRev !== rev) { VC.beforeFallback = VC.lastFrame; VC.beforeRev = VC.lastRev; base.changed = true; }
  if (frame) { base.before = (VC.beforeRev && VC.frames.get(VC.beforeRev + ':' + frame.index)) || VC.beforeFallback; VC.lastFrame = frame; }
  VC.lastRev = rev;
  return { ...base, frame };
}
async function mockDraft({ start, seconds }) {
  if (!project.tracks.some(t => t.clips.length)) throw 'out_of_range: the timeline is empty';
  await sleep(300);
  const c = document.createElement('canvas'); c.width = 640; c.height = 360;
  const end = Math.min(DUR(), start + seconds), n = Math.round((end - start) * FPS), frame_urls = [];
  for (let i = 0; i < n; i++) { paint(c, start + i / FPS); frame_urls.push(c.toDataURL('image/jpeg', .7)); }
  return { rev: revOf(), start, end, fps: FPS, frames: n, width: 640, height: 360, url: '', frame_urls, ms: 900, first_frame_ms: 200, extract_ms: 80, cached: false };
}
function thumb(path) {
  const a = Object.values(ASSETS).find(x => x.path === path); if (!a || a.hue == null) return null;
  const c = document.createElement('canvas'); c.width = 320; c.height = 200; paint(c, (a.hue % 17), a.hue); return c.toDataURL('image/jpeg', .8);
}

// ---------- the ledger: every engine call is tagged with who made it ----------
// ---- real-time preview stand-in (?stream=1): the engine's preview_stream, frames painted at 320x180, a 1-second sine chunk per second ----
const STREAM = params.get('stream') === '1';
const PS = { gen: 0, start: 0, t: 0, rate: 1, playing: false, rev: null, frames: new Map(), chunks: new Map() };
const psRun = () => ({ start: PS.start, dir: PS.rate < 0 ? -1 : 1, stride: Math.max(1, Math.round(Math.abs(PS.rate))) });
function psFrameUrl(i) {
  const k = revOf() + ':' + i; let u = PS.frames.get(k); if (u) return u;
  const c = document.createElement('canvas'); c.width = 320; c.height = 180; paint(c, i / FPS);
  const g = c.getContext('2d'); g.fillStyle = '#fff'; g.font = '20px monospace'; g.fillText(String(i), 8, 24); // the frame number, for sync checks
  u = c.toDataURL('image/png'); PS.frames.set(k, u); if (PS.frames.size > 600) PS.frames.delete(PS.frames.keys().next().value); return u;
}
function psChunkUrl(k) {
  let u = PS.chunks.get(k); if (u) return u;
  const n = 48000, b = new DataView(new ArrayBuffer(44 + n * 4)), w = (o, str) => [...str].forEach((ch, i) => b.setUint8(o + i, ch.charCodeAt(0)));
  w(0, 'RIFF'); b.setUint32(4, 36 + n * 4, true); w(8, 'WAVEfmt '); b.setUint32(16, 16, true); b.setUint16(20, 1, true); b.setUint16(22, 2, true); b.setUint32(24, 48000, true); b.setUint32(28, 192000, true); b.setUint16(32, 4, true); b.setUint16(34, 16, true); w(36, 'data'); b.setUint32(40, n * 4, true);
  for (let i = 0; i < n; i++) { const v = Math.round(Math.sin(2 * Math.PI * (220 + 20 * k) * i / 48000) * 3000); b.setInt16(44 + i * 4, v, true); b.setInt16(46 + i * 4, v, true); }
  u = URL.createObjectURL(new Blob([b.buffer], { type: 'audio/wav' })); PS.chunks.set(k, u); return u;
}
function psReport() {
  const run = psRun(), last = Math.floor(DUR() * FPS) - 1, f0 = Math.floor(PS.t * FPS + 1e-6), d = (f0 - run.start) * run.dir, q0 = Math.max(0, Math.floor(d / run.stride)), frames = [];
  for (let q = q0; q < q0 + 40; q++) { const i = run.start + run.dir * run.stride * q; if (i < 0 || i > last) break; frames.push({ seq: q, index: i, time: i / FPS, name: `${PS.gen}-${i}`, url: psFrameUrl(i), ms: 20 }); }
  const audio = PS.rate === 1 ? [0, 1, 2].map(j => Math.floor(PS.t) + j).filter(k => k < DUR()).map(k => ({ chunk: k, start: k, end: k + 1, name: `a${revOf()}-${k}`, url: psChunkUrl(k), audible: true, ms: 30 })) : [];
  return { gen: PS.gen, rev: revOf(), run, fps: FPS, duration: DUR(), width: 320, height: 180, scale: 0.5, playing: PS.playing, frames, audio, audio_on: PS.rate === 1, ready_ahead: frames.length, ring: 96, render_fps: 60, skip: 1, rate: PS.rate, first_frame_ms: 40, proxies: [], error: null, audio_error: null };
}
const psNeed = () => { if (!STREAM) throw 'unknown command preview_start'; if (!project.tracks.some(t => t.clips.length)) throw 'out_of_range: the timeline is empty'; };
const psRestart = (t, rate) => { PS.gen++; PS.t = t; PS.rate = rate ?? PS.rate; PS.start = Math.floor(t * FPS + 1e-6); PS.rev = revOf(); };

let callN = 0, changeN = 0;
const UNDOABLE = new Set(['add_clip', 'move_clip', 'trim_clip', 'split_clip', 'duplicate_clip', 'delete_clip', 'ripple_delete', 'add_track', 'delete_track', 'set_track', 'add_marker', 'add_text', 'set_text', 'set_audio', 'set_speed', 'add_effect', 'remove_effect', 'add_transition', 'close_gaps', 'project_set', 'set_temperature', 'set_caption_style', 'import_media', 'set_property', 'auto_captions']);
function apply(action, a) {
  const [t, c] = a.clip_id ? findClip(a.clip_id) : [null, null];
  switch (action) {
    case 'add_clip': { const tr = project.tracks.find(x => x.id === a.track_id) || project.tracks.at(-2); tr.clips.push(clip('c' + (++callN), a.start, a.duration, { type: a.source_type || 'media', asset_id: a.asset_id })); break; }
    case 'add_text': { const tr = project.tracks.find(x => x.id === a.track_id) || project.tracks[0]; tr.clips.push(clip('c' + (++callN), a.start, a.duration, { type: 'text', text: a.text, size: a.size, color: [1, 1, 1, 1] })); break; }
    case 'move_clip': if (c) c.start = a.start; break;
    case 'trim_clip': if (c) { if (a.duration != null) c.duration = a.duration; if (a.in_point != null) c.in_point = a.in_point; } break;
    case 'set_speed': if (c) c.speed = a.speed; break;
    case 'split_clip': if (c && a.at > c.start && a.at < c.start + c.duration) { const d = a.at - c.start; t.clips.push({ ...structuredClone(c), id: c.id + 'b', start: a.at, duration: c.duration - d, in_point: (c.in_point || 0) + d }); c.duration = d; } break;
    case 'duplicate_clip': if (c) t.clips.push({ ...structuredClone(c), id: c.id + 'd' + (++callN), start: a.start }); break;
    case 'delete_clip': case 'ripple_delete': if (c) t.clips = t.clips.filter(x => x !== c); break;
    case 'add_track': project.tracks.unshift({ id: 'V' + (++callN), name: 'Track ' + (project.tracks.length + 1), clips: [] }); break;
    case 'delete_track': project.tracks = project.tracks.filter(x => x.id !== a.track_id); break;
    case 'set_track': { const tr = project.tracks.find(x => x.id === a.track_id); if (tr) Object.assign(tr, Object.fromEntries(Object.entries(a).filter(([k]) => k !== 'track_id'))); break; }
    case 'add_marker': project.markers.push({ time: a.time, name: a.name }); break;
    case 'set_text': if (c) Object.assign(c.source, Object.fromEntries(Object.entries(a).filter(([k]) => k !== 'clip_id'))); break;
    case 'set_audio': if (c) Object.assign(c, Object.fromEntries(Object.entries(a).filter(([k]) => k !== 'clip_id'))); break;
    case 'add_effect': if (c) c.effects.push({ id: 'fx' + (++callN), kind: a.kind }); break;
    case 'remove_effect': if (c) c.effects = c.effects.filter(f => f.id !== a.effect_id); break;
    case 'add_transition': if (c) c.transition_in = { kind: a.kind, duration: a.duration }; break;
    case 'project_set': Object.assign(project, a); break;
    case 'set_temperature': M.edit.warm = a.value; break;
    case 'set_caption_style': M.edit.caption = a.size; break;
    case 'import_media': { const id = 'a' + (Object.keys(ASSETS).length + 1); ASSETS[id] = { id, path: a.path, duration: 12, width: 1920, height: 1080, fps: 30, hue: (Object.keys(ASSETS).length * 47) % 360 }; return { asset_id: id }; }
  }
  return { ok: true };
}
const summarize = (a = {}) => Object.entries(a).map(([k, v]) => `${k}=${typeof v === 'object' ? JSON.stringify(v) : v}`).join(', ');
async function ledgerCall(actor, tool, action, args = {}, ms = 120) {
  const id = 'l' + (++callN), t = Date.now(), summary = summarize(args), ro = !UNDOABLE.has(action);
  emit('live-event', { id, actor, t, tool, action, summary, args, phase: 'start', read_only: ro });
  await sleep(ms);
  let data;
  if (action === 'undo' || action === 'redo') {
    const from = action === 'undo' ? M.undo : M.redo, to = action === 'undo' ? M.redo : M.undo;
    if (!from.length) { emit('live-event', { id, actor, t, tool, action, phase: 'end', ok: false, ms, result: 'nothing to ' + action }); return { ok: false, text: 'Nothing to ' + action + '.' }; }
    to.push(snapshot()); restore(from.pop());
    const live = M.changes.filter(x => !x.undone);
    if (action === 'undo' && live.length) live.at(-1).undone = true;
    if (action === 'redo') { const u = M.changes.find(x => x.undone); if (u) u.undone = false; }
    emit('changes', M.changes.slice());
  } else if (!ro) { M.undo.push(snapshot()); M.redo = []; data = apply(action, args); }
  const findings = action === 'add_text' && args.start < 0.5 ? [{ severity: 'warning', code: 'title_too_early', message: 'The title starts before the first frame settles. Start it at 0.5 s or later.', fix: { tool: 'timeline', args: { action: 'move_clip', args: { clip_id: 'last', start: 0.5 } } } }] : [];
  let change = null;
  if (!ro) {
    change = { id: ++changeN, actor, tool, action, summary, t: Date.now(), undone: false, steps: 1, findings };
    M.changes.push(change); emit('changes', M.changes.slice());
  }
  if (!ro || action === 'undo' || action === 'redo') { M.dirty = true; emit('project-changed', { actor, action }); }
  const counts = findings.length ? { warning: findings.length } : undefined;
  emit('live-event', { id, actor, t, tool, action, phase: 'end', ok: true, ms: ms - 10, result: ro ? 'read' : 'ok', change: change && { id: change.id }, findings, counts });
  return { ok: true, data: data ?? { ok: true } };
}

// ---------- an agent turn: streamed events, real ledger calls ----------
let agentAbort = false;
async function agentTurn(text, turn) {
  const agent = M.settings.agent, ev = e => emit('agent-event', { turn, ev: e });
  agentAbort = false;
  M.attached[agent] = 1; emit('agent-connection', { up: true, client: agent, attached: { ...M.attached } });
  ev({ kind: 'thinking', text: 'Looking at the timeline and the media first. ' }); await sleep(500);
  const plan = /caption/i.test(text) ? [['text', 'set_caption_style', { track_id: 'T1', size: 72 }]]
    : /silence|pause|cut/i.test(text) ? [['audio', 'detect_silence', { clip_id: 'v1_c3', threshold_db: -38 }], ['timeline', 'ripple_delete', { clip_id: 'v1_c3' }]]
    : /title/i.test(text) ? [['text', 'add_text', { track_id: 'T1', text: 'Golden hour', start: 0.2, duration: 3, size: 96 }]]
    : [['project', 'get_project', {}], ['color_grade', 'set_temperature', { clip_id: 'v1_c1', value: 0.12 }]];
  for (const [tool, action, args] of plan) {
    if (agentAbort) throw 'Stopped.';
    const id = 'tu' + (++callN);
    ev({ kind: 'tool_start', id, tool, input: { action, args } });
    const r = await ledgerCall(agent, tool, action, args, 420);
    ev({ kind: 'tool_end', id, ok: r.ok, output: r.ok ? 'ok' : r.text });
  }
  const reply = /caption/i.test(text) ? 'Captions are bigger now: 54 → 72 px on the captions track. Undo it from Changes if it covers too much of the frame.'
    : /silence|pause|cut/i.test(text) ? 'Found a 0.8 s pause in interview B at 0:12 and cut it, closing the gap.'
    : /title/i.test(text) ? 'Added "Golden hour" over the opening shot for 3 seconds.'
    : 'Warmed the opening shot slightly (+0.12 temperature). The rest of the grade is unchanged.';
  for (const w of reply.split(/(?<= )/)) { if (agentAbort) break; ev({ kind: 'delta', text: w }); await sleep(18); }
  ev({ kind: 'done', turns: 1 + plan.length });
  delete M.attached[agent]; emit('agent-connection', { up: false, client: agent, attached: { ...M.attached } });
  return { reply };
}

const handlers = {
  app_state: () => ({
    version: '9.0.0', engine: { state: 'running', pid: 48213, version: '9.0.0', tools: 17, actions: 540, bin: BIN },
    mcp_enabled: M.mcp, external_port: null, settings: M.settings, providers, project_path: M.path, attached: { ...M.attached },
    keys: providers.map(p => ({ provider: p.id, set: !!M.keys[p.id], hint: M.keys[p.id] ? M.keys[p.id].slice(0, 3) + '…' + M.keys[p.id].slice(-4) : null, remembered: M.settings.remembered.includes(p.id) })),
    key_store: 'Windows Credential Manager', data_dir: DATA, os: 'windows',
    snippets: { json: JSON.stringify({ mcpServers: { kimo: { command: BIN, args: ['mcp'] } } }, null, 2) },
  }),
  engine_restart: () => handlers.app_state().engine,
  engine_call: ({ tool, args }) => ledgerCall('you', tool, args?.action, args?.args || {}),
  engine_read: async ({ tool, args }) => {
    const a = args?.action;
    if (a === 'get_project') return { ok: true, data: structuredClone(project) };
    if (a === 'list_transitions') return { ok: true, data: ['dissolve', 'dip_to_black', 'wipe_left', 'slide_up', 'zoom_in', 'whip_pan', 'glitch'] };
    if (a === 'job_status') { EXJ.n = Math.min(EXJ.total, EXJ.n + 90); return { ok: true, data: { state: EXJ.n >= EXJ.total ? 'done' : 'running', frames_done: EXJ.n, frames_total: EXJ.total, fps: 148, eta_s: (EXJ.total - EXJ.n) / 148 } }; }
    return { ok: true, data: {} };
  },
  changes_list: () => M.changes.slice(),
  change_undo: async ({ id }) => {
    const live = M.changes.filter(c => !c.undone && c.id >= id);
    for (let i = 0; i < live.length; i++) { restore(M.undo.pop() || snapshot()); }
    live.forEach(c => c.undone = true); emit('changes', M.changes.slice()); emit('project-changed', { actor: 'you', action: 'undo' });
    return { steps: live.length };
  },
  viewer_frame: mockFrame, viewer_draft: mockDraft, media_thumb: ({ path }) => thumb(path),
  preview_start: async ({ time, rate, playing }) => { psNeed(); psRestart(time, rate || 1); PS.playing = !!playing; return psReport(); },
  preview_update: async ({ time, rate, playing, seek }) => {
    psNeed(); await sleep(5);
    if (playing != null) PS.playing = playing;
    const r = rate ?? PS.rate;
    if (seek || r !== PS.rate || PS.rev !== revOf()) psRestart(time, r); else PS.t = time;
    return psReport();
  },
  preview_frame: async ({ time }) => { psNeed(); await sleep(40); const i = Math.min(Math.floor(DUR() * FPS) - 1, Math.floor(time * FPS + 1e-6)); return { index: i, time: i / FPS, name: `s-${i}`, url: psFrameUrl(i), cached: false, ms: 38, width: 320, height: 180 }; },
  preview_stop: async () => ({ stopped: true }),
  crash_pending: () => [], crash_answer: () => ({}), update_install: () => { throw 'no update in the mock'; },
  cli_status: async () => { await sleep(250); return structuredClone(M.cli); },
  cli_models: async ({ id }) => { await sleep(200); if (!M.models[id]) throw `${id} is not installed`; return M.models[id]; },
  cli_install: async ({ id }) => { await sleep(300); return `Opened a terminal that installs ${id}.`; },
  cli_signin: async ({ id }) => { await sleep(300); return `Opened ${id}'s own sign-in in a terminal.`; },
  cli_register: async ({ id }) => { M.cli.find(c => c.id === id).registered = true; return `Added kimo to ${id}'s MCP list.`; },
  agent_select: ({ agent, model, effort, select }) => {
    if (select !== false) M.settings.agent = agent;
    if (model != null) M.settings.agent_models[agent] = model;
    if (effort != null) M.settings.agent_efforts[agent] = effort;
  },
  agent_send: ({ text, turn }) => agentTurn(text, turn),
  agent_stop: () => { agentAbort = true; },
  agent_reset: () => null,
  key_set: ({ provider, key, remember }) => { M.keys[provider] = key; M.settings.provider = provider; if (remember) M.settings.remembered.push(provider); return { provider, set: true, hint: key.slice(0, 3) + '…' + key.slice(-4), remembered: remember, store: 'Windows Credential Manager', note: null }; },
  key_clear: ({ provider }) => { delete M.keys[provider]; return { provider, set: false }; },
  key_test: async ({ provider }) => { await sleep(300); if (!M.keys[provider]) throw 'Paste a key first.'; return 'Connected. The key works.'; },
  settings_set: ({ provider, model, baseUrl }) => { M.settings.provider = provider; if (model != null) M.settings.models[provider] = model; if (baseUrl != null) M.settings.base_urls[provider] = baseUrl; },
  mcp_set: async ({ enabled }) => { await sleep(300); M.mcp = M.settings.mcp_enabled = enabled; return { enabled }; },
  onboarded_set: () => { M.settings.onboarded = true; },
  open_spectator: () => null, open_url: () => null, reveal: () => null,
  pick: async ({ kind }) => kind === 'media' ? [HOME + '\\Videos\\new-take-' + (++callN) + '.mp4'] : kind === 'project' ? [M.settings.recent[0]] : [HOME + '\\Videos\\export.mp4'],
  project_new: async () => { M.path = null; M.dirty = false; emit('project-changed', { actor: 'you', action: 'new_project' }); },
  project_open: async ({ path }) => { M.path = path; M.dirty = false; emit('project-changed', { actor: 'you', action: 'load_project' }); return 'Opened ' + path; },
  project_save: async ({ path }) => { M.path = path || M.path || M.settings.recent[0]; M.dirty = false; return M.path; },
  recovery_status: () => params.get('recover') === 'show' ? { autosaved_at: Date.now() - 6 * 60e3, project_path: M.settings.recent[0] } : null,
  recovery_restore: () => null, recovery_discard: () => null,
  setup_status: () => ({ ready: M.parts.filter(p => !p.optional).every(p => p.satisfied), done: M.parts.filter(p => !p.optional).every(p => p.satisfied), parts: M.parts, dir: DATA }),
  setup_start: async ({ skipOptional: skip_optional }) => {
    for (const p of M.parts) {
      if (p.satisfied || (skip_optional && p.optional)) continue;
      for (let d = 0; d <= p.size; d += p.size / 12) { emit('setup-progress', { id: p.id, done: Math.round(d), total: p.size, bytes_per_s: 11.4e6, eta_s: (p.size - d) / 11.4e6, phase: 'downloading', mirror: 0 }); await sleep(40); }
      p.satisfied = 'downloaded'; emit('setup-progress', { id: p.id, done: p.size, total: p.size, bytes_per_s: 0, eta_s: 0, phase: 'done', mirror: 0 });
    }
    return { ready: true, results: [] };
  },
  setup_pause: () => null, setup_skip: () => null,
};
const EXJ = { n: 0, total: 555 };
const origCall = handlers.engine_call;
handlers.engine_call = async a => { if (a.args?.action === 'export_job_start') { EXJ.n = 0; await sleep(150); return { ok: true, data: { job_id: 'job1' } }; } return origCall(a); };

export const bridge = {
  kind: 'mock',
  invoke: async (c, a) => { const h = handlers[c]; if (!h) throw `The test bridge has no "${c}" command yet.`; return h(a || {}); },
  listen: (e, f) => { (listeners[e] = listeners[e] || []).push(f); return Promise.resolve(() => {}); },
};
if (params.get('busy') === '1') setTimeout(() => window.__kimo?.sendAgent('Cut the silences'), 1200);
// test/screenshot hooks
window.__mock = { emit, M, project, ledgerCall };
